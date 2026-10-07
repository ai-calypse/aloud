//! Wiring for the Today section: weather, the stock ticker, and (below) the
//! calendar and reminders. Thin over `aloud_today` and `aloud_native::agenda`,
//! which own the parsing, the clocks and the failure modes.

use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use aloud_agenda::{AgendaError, Event, Reminder};
use tauri::AppHandle;

use aloud_today::prefs::{Place, Prefs};
use aloud_today::stocks::{self, Ticker};
use aloud_today::weather::{self, Weather};

/// Open-Meteo updates its model about hourly; asking more often returns the
/// same numbers.
const WEATHER_TTL: Duration = Duration::from_secs(30 * 60);

struct Cached {
    at: Instant,
    fahrenheit: bool,
    weather: Weather,
}

static WEATHER: Mutex<Option<Cached>> = Mutex::new(None);

fn cache() -> std::sync::MutexGuard<'static, Option<Cached>> {
    WEATHER.lock().unwrap_or_else(PoisonError::into_inner)
}

async fn fetch_weather(fahrenheit: bool) -> Result<Weather, String> {
    let place = match Prefs::load().city {
        Some(place) => place,
        None => weather::locate().await.map_err(|e| e.to_string())?,
    };
    let weather = weather::forecast(&place, fahrenheit)
        .await
        .map_err(|e| format!("could not reach the weather service: {e}"))?;
    *cache() = Some(Cached {
        at: Instant::now(),
        fahrenheit,
        weather: weather.clone(),
    });
    Ok(weather)
}

/// The forecast for the saved city, else wherever this connection is. Served
/// from a 30-minute cache unless `refresh`; the webview's `fahrenheit` is
/// part of the key, so flipping units never shows the other unit's numbers.
#[tauri::command]
#[specta::specta]
pub async fn weather(fahrenheit: bool, refresh: bool) -> Result<Weather, String> {
    if !refresh {
        if let Some(c) = cache().as_ref() {
            if c.fahrenheit == fahrenheit && c.at.elapsed() < WEATHER_TTL {
                return Ok(c.weather.clone());
            }
        }
    }
    fetch_weather(fahrenheit).await
}

/// Pins the weather to a city by name, for a VPN or a Mac that is not where
/// its IP says.
#[tauri::command]
#[specta::specta]
pub async fn set_weather_city(name: String, fahrenheit: bool) -> Result<Weather, String> {
    let place: Place = weather::geocode(&name).await.map_err(|e| e.to_string())?;
    let mut prefs = Prefs::load();
    prefs.city = Some(place);
    prefs.save().map_err(|e| e.to_string())?;
    fetch_weather(fahrenheit).await
}

/// Back to locating by IP.
#[tauri::command]
#[specta::specta]
pub async fn clear_weather_city(fahrenheit: bool) -> Result<Weather, String> {
    let mut prefs = Prefs::load();
    prefs.city = None;
    prefs.save().map_err(|e| e.to_string())?;
    fetch_weather(fahrenheit).await
}

/// Quotes for the watchlist; empty when nothing is followed.
#[tauri::command]
#[specta::specta]
pub async fn stock_quotes() -> Vec<Ticker> {
    stocks::quotes(&Prefs::load().stocks).await
}

#[tauri::command]
#[specta::specta]
pub async fn add_stock(symbol: String) -> Result<Vec<Ticker>, String> {
    let symbol = stocks::normalize(&symbol).ok_or("that is not a ticker symbol")?;
    let mut prefs = Prefs::load();
    if !prefs.stocks.contains(&symbol) {
        // ponytail: a dozen symbols is a glance; the cap keeps one view's
        // refresh from fanning out into dozens of requests.
        if prefs.stocks.len() >= 12 {
            return Err("twelve symbols is the most the ticker follows".into());
        }
        prefs.stocks.push(symbol);
        prefs.save().map_err(|e| e.to_string())?;
    }
    Ok(stocks::quotes(&prefs.stocks).await)
}

#[tauri::command]
#[specta::specta]
pub async fn remove_stock(symbol: String) -> Result<Vec<Ticker>, String> {
    let mut prefs = Prefs::load();
    prefs.stocks.retain(|s| s != &symbol);
    prefs.save().map_err(|e| e.to_string())?;
    Ok(stocks::quotes(&prefs.stocks).await)
}

// ---- Calendar and Reminders -------------------------------------------------

/// Today and tomorrow: what the view shows, and what the next-event island
/// looks through.
const CALENDAR_DAYS: i64 = 2;
/// How stale the events may get before the background tick re-reads them.
const CALENDAR_TTL: Duration = Duration::from_secs(10 * 60);

struct Agenda {
    at: Instant,
    /// The local midnight the window began at, so a cache from yesterday is
    /// never mistaken for today's.
    day: i64,
    events: Vec<Event>,
}

static AGENDA: Mutex<Option<Agenda>> = Mutex::new(None);

fn agenda() -> std::sync::MutexGuard<'static, Option<Agenda>> {
    AGENDA.lock().unwrap_or_else(PoisonError::into_inner)
}

fn read_events() -> Result<Vec<Event>, AgendaError> {
    let day = aloud_agenda::day_start(0);
    let events = aloud_agenda::events(day, aloud_agenda::day_start(CALENDAR_DAYS))?;
    *agenda() = Some(Agenda {
        at: Instant::now(),
        day,
        events: events.clone(),
    });
    Ok(events)
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn sync_island(app: &AppHandle) {
    let island = agenda()
        .as_ref()
        .and_then(|a| aloud_agenda::island(&a.events, now_ms()));
    match island {
        Some(island) => crate::islands::push(app, island),
        None => crate::islands::remove(app, aloud_agenda::NEXT_ISLAND_ID),
    }
}

/// Today's and tomorrow's events. `refresh` re-reads Calendar (about a
/// second); otherwise a read from the last ten minutes is reused.
///
/// This is also the only thing that may start Calendar.app: scripting an app
/// that is not running launches it, which the background tick never does.
#[tauri::command]
#[specta::specta]
pub async fn calendar_events(app: AppHandle, refresh: bool) -> Result<Vec<Event>, AgendaError> {
    if !refresh {
        let cached = agenda().as_ref().and_then(|a| {
            (a.day == aloud_agenda::day_start(0) && a.at.elapsed() < CALENDAR_TTL)
                .then(|| a.events.clone())
        });
        if let Some(events) = cached {
            return Ok(events);
        }
    }
    let events = tauri::async_runtime::spawn_blocking(read_events)
        .await
        .map_err(|e| AgendaError::Failed {
            message: e.to_string(),
        })??;
    sync_island(&app);
    Ok(events)
}

/// What the last read saw, from memory or the last run's file: the view paints
/// this immediately and `reminders_due` corrects it. `None` only before the
/// first read there has ever been.
#[tauri::command]
#[specta::specta]
pub fn reminders_cached() -> Option<Vec<Reminder>> {
    aloud_agenda::cache::reminders().get()
}

/// Incomplete reminders due by the end of today, overdue ones first. EventKit
/// when granted (this is the call that may raise its one-time prompt), else a
/// script; either way the result becomes the cache.
#[tauri::command]
#[specta::specta]
pub async fn reminders_due() -> Result<Vec<Reminder>, AgendaError> {
    let read = tauri::async_runtime::spawn_blocking(|| {
        aloud_agenda::reminders(aloud_agenda::day_start(1), true)
    })
    .await
    .map_err(|e| AgendaError::Failed {
        message: e.to_string(),
    })?;
    if let Ok(items) = &read {
        aloud_agenda::cache::reminders().put(items.clone());
        // The user has seen Reminders answer, so the nudge tick may ask too.
        crate::nudges::REMINDERS_ALLOWED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    read
}

/// One day's events for the month card's date picker. `day` is any instant on
/// the day, in unix ms (a number, not a bigint: it is a day, not a count).
#[tauri::command]
#[specta::specta]
pub async fn calendar_day(day: f64) -> Result<Vec<Event>, AgendaError> {
    tauri::async_runtime::spawn_blocking(move || aloud_agenda::events_on(day as i64))
        .await
        .map_err(|e| AgendaError::Failed {
            message: e.to_string(),
        })?
}

/// Ticks a reminder off. The only thing in Today that writes to the account.
#[tauri::command]
#[specta::specta]
pub async fn complete_reminder(id: String) -> Result<(), AgendaError> {
    let done = {
        let id = id.clone();
        tauri::async_runtime::spawn_blocking(move || aloud_agenda::complete_reminder(&id))
            .await
            .map_err(|e| AgendaError::Failed {
                message: e.to_string(),
            })?
    };
    if done.is_ok() {
        aloud_agenda::cache::reminders().remove(&id);
    }
    done
}

/// Opens an event's video-call link in the browser. Re-checks the host: the
/// webview names the URL, and this must not open anything but a call.
#[tauri::command]
#[specta::specta]
pub fn open_meeting_link(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let url = aloud_agenda::join_url(Some(&url), None).ok_or("that is not a call link")?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Keeps the next-event island true, once a minute. It re-reads Calendar only
/// while Calendar.app is already open — the island is a courtesy, and it must
/// not be the reason an app launches — and the view's own fetch covers the
/// rest.
pub fn init(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut ticks = 0u32;
        loop {
            // Parked while the notch is off: the agenda and its nudges are
            // only ever shown there, and a read can launch Calendar's scripting.
            if !crate::labs::notch() {
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }
            let due = agenda().as_ref().is_none_or(|a| {
                a.day != aloud_agenda::day_start(0) || a.at.elapsed() >= CALENDAR_TTL
            });
            if due && aloud_agenda::app_running("Calendar") {
                // A failure (no grant yet) leaves the cache empty and is
                // retried in ten minutes, not every tick.
                let read = tauri::async_runtime::spawn_blocking(read_events).await;
                if !matches!(read, Ok(Ok(_))) {
                    *agenda() = Some(Agenda {
                        at: Instant::now(),
                        day: aloud_agenda::day_start(0),
                        events: Vec::new(),
                    });
                }
            }
            sync_island(&app);
            let events = agenda()
                .as_ref()
                .map(|a| a.events.clone())
                .unwrap_or_default();
            crate::nudges::calendar_tick(&app, &events, now_ms());
            ticks += 1;
            if ticks.is_multiple_of(2) {
                crate::nudges::reminders_tick(&app, now_ms()).await;
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}
