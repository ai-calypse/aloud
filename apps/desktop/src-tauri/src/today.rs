//! Wiring for the Today section: weather, the stock ticker, and (below) the
//! calendar and reminders. Thin over `aloud_today` and `aloud_native::agenda`,
//! which own the parsing, the clocks and the failure modes.

use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

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
