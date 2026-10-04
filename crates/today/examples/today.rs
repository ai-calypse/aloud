//! Hits the real services: Open-Meteo for the weather, the IP lookup for a
//! place, Yahoo for quotes.
//!
//!   cargo run -p aloud-today --example today -- [Toronto] [AAPL NVDA BTC-USD]

use aloud_today::{stocks, weather};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let place = match args
        .first()
        .filter(|a| a.chars().all(|c| c.is_alphabetic() || c == ' '))
    {
        Some(city) => weather::geocode(city).await,
        None => weather::locate().await,
    };
    match place {
        Ok(place) => match weather::forecast(&place, false).await {
            Ok(w) => println!("{w:#?}"),
            Err(e) => println!("forecast failed: {e:#}"),
        },
        Err(e) => println!("place failed: {e:#}"),
    }
    let symbols: Vec<String> = args
        .iter()
        .skip(1)
        .filter_map(|s| stocks::normalize(s))
        .collect();
    let symbols = if symbols.is_empty() {
        vec![
            "AAPL".into(),
            "NVDA".into(),
            "BTC-USD".into(),
            "NOPE1234".into(),
        ]
    } else {
        symbols
    };
    for t in stocks::quotes(&symbols).await {
        println!("{t:?}");
    }
}
