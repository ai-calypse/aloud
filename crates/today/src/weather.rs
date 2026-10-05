//! Weather from Open-Meteo (free, no key). Where you are comes from your
//! saved city, else a coarse IP lookup — no CoreLocation, so no permission.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::client;
use crate::prefs::Place;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Sky {
    Clear,
    PartlyCloudy,
    Cloudy,
    Fog,
    Drizzle,
    Rain,
    Snow,
    Storm,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    /// `YYYY-MM-DD` in the place's own timezone.
    pub day: String,
    pub sky: Sky,
    pub high: f64,
    pub low: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Weather {
    pub place: String,
    pub temperature: f64,
    pub feels_like: f64,
    pub sky: Sky,
    /// "Partly cloudy".
    pub summary: String,
    pub is_day: bool,
    /// Today first, five days.
    pub days: Vec<Day>,
}

/// WMO weather interpretation codes, as Open-Meteo sends them.
pub fn sky(code: u32) -> (Sky, &'static str) {
    match code {
        0 => (Sky::Clear, "Clear"),
        1 => (Sky::Clear, "Mostly clear"),
        2 => (Sky::PartlyCloudy, "Partly cloudy"),
        3 => (Sky::Cloudy, "Overcast"),
        45 | 48 => (Sky::Fog, "Fog"),
        51..=55 => (Sky::Drizzle, "Drizzle"),
        56 | 57 => (Sky::Drizzle, "Freezing drizzle"),
        61 | 63 | 65 => (Sky::Rain, "Rain"),
        66 | 67 => (Sky::Rain, "Freezing rain"),
        71..=77 => (Sky::Snow, "Snow"),
        80..=82 => (Sky::Rain, "Showers"),
        85 | 86 => (Sky::Snow, "Snow showers"),
        95..=99 => (Sky::Storm, "Thunderstorm"),
        // A code added later reads as nothing rather than as wrong.
        _ => (Sky::Cloudy, "Cloudy"),
    }
}

#[derive(Deserialize)]
struct Forecast {
    current: Current,
    daily: Daily,
}

#[derive(Deserialize)]
struct Current {
    temperature_2m: f64,
    apparent_temperature: f64,
    weather_code: u32,
    is_day: u8,
}

#[derive(Deserialize)]
struct Daily {
    time: Vec<String>,
    weather_code: Vec<u32>,
    temperature_2m_max: Vec<f64>,
    temperature_2m_min: Vec<f64>,
}

fn parse(place: &str, body: &str) -> Result<Weather> {
    let f: Forecast = serde_json::from_str(body).context("reading the forecast")?;
    let (current_sky, summary) = sky(f.current.weather_code);
    let d = &f.daily;
    let days = d
        .time
        .iter()
        .enumerate()
        .filter_map(|(i, day)| {
            Some(Day {
                day: day.clone(),
                sky: sky(*d.weather_code.get(i)?).0,
                high: *d.temperature_2m_max.get(i)?,
                low: *d.temperature_2m_min.get(i)?,
            })
        })
        .collect();
    Ok(Weather {
        place: place.to_string(),
        temperature: f.current.temperature_2m,
        feels_like: f.current.apparent_temperature,
        sky: current_sky,
        summary: summary.to_string(),
        is_day: f.current.is_day == 1,
        days,
    })
}

pub async fn forecast(place: &Place, fahrenheit: bool) -> Result<Weather> {
    let unit = if fahrenheit { "fahrenheit" } else { "celsius" };
    let body = client()
        .get("https://api.open-meteo.com/v1/forecast")
        .query(&[
            ("latitude", place.latitude.to_string()),
            ("longitude", place.longitude.to_string()),
            (
                "current",
                "temperature_2m,apparent_temperature,weather_code,is_day".into(),
            ),
            (
                "daily",
                "weather_code,temperature_2m_max,temperature_2m_min".into(),
            ),
            ("temperature_unit", unit.into()),
            ("timezone", "auto".into()),
            ("forecast_days", "5".into()),
        ])
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    parse(&place.name, &body)
}

/// A city by name, for "that is not where I am".
pub async fn geocode(name: &str) -> Result<Place> {
    #[derive(Deserialize)]
    struct Found {
        results: Option<Vec<Hit>>,
    }
    #[derive(Deserialize)]
    struct Hit {
        name: String,
        latitude: f64,
        longitude: f64,
        admin1: Option<String>,
    }
    let found: Found = client()
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .query(&[("name", name.trim()), ("count", "1")])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let hit = found
        .results
        .and_then(|r| r.into_iter().next())
        .ok_or_else(|| anyhow!("no place called \"{}\"", name.trim()))?;
    Ok(Place {
        name: hit
            .admin1
            .map_or(hit.name.clone(), |a| format!("{}, {a}", hit.name)),
        latitude: hit.latitude,
        longitude: hit.longitude,
    })
}

/// Roughly where this connection is. City-level at best and wrong on a VPN,
/// which is why the view offers a city field.
///
/// ponytail: one free, keyless IP service. If it ever dies, the saved-city
/// path still works; a second provider is the upgrade.
pub async fn locate() -> Result<Place> {
    #[derive(Deserialize)]
    struct Ip {
        success: bool,
        city: Option<String>,
        latitude: Option<f64>,
        longitude: Option<f64>,
    }
    let ip: Ip = client()
        .get("https://ipwho.is/")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    match (ip.success, ip.latitude, ip.longitude) {
        (true, Some(latitude), Some(longitude)) => Ok(Place {
            name: ip.city.unwrap_or_else(|| "Here".into()),
            latitude,
            longitude,
        }),
        _ => Err(anyhow!("could not tell where you are")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wmo_codes_group_into_what_the_icon_shows() {
        assert_eq!(sky(0).0, Sky::Clear);
        assert_eq!(sky(2), (Sky::PartlyCloudy, "Partly cloudy"));
        assert_eq!(sky(66).1, "Freezing rain");
        assert_eq!(sky(81).0, Sky::Rain);
        assert_eq!(sky(96).0, Sky::Storm);
        assert_eq!(sky(1234).0, Sky::Cloudy);
    }

    /// A trimmed live response. A daily array shorter than `time` must not
    /// panic or misalign the days.
    #[test]
    fn parses_a_forecast_and_drops_a_ragged_day() {
        let body = r#"{"current":{"time":"2026-10-04T16:45","temperature_2m":20.2,
            "apparent_temperature":17.7,"weather_code":51,"is_day":1},
            "daily":{"time":["2026-10-04","2026-10-05","2026-10-06"],
            "weather_code":[51,3],"temperature_2m_max":[20.2,14.3],"temperature_2m_min":[10.1,8.1]}}"#;
        let w = parse("Toronto", body).unwrap();
        assert_eq!((w.temperature, w.feels_like, w.is_day), (20.2, 17.7, true));
        assert_eq!(w.summary, "Drizzle");
        assert_eq!(w.days.len(), 2);
        assert_eq!(
            w.days[1],
            Day {
                day: "2026-10-05".into(),
                sky: Sky::Cloudy,
                high: 14.3,
                low: 8.1
            }
        );
        assert!(parse("x", "{}").is_err());
    }
}
