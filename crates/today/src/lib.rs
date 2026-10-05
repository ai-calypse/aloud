//! What the Today section reads from the network, without Tauri. The webview's
//! CSP is `default-src 'self'`, so every fetch happens here and the webview
//! gets typed values.

use std::time::Duration;

pub mod prefs;
pub mod stocks;
pub mod weather;

/// One pooled client: a client per request would redo the TLS handshake on
/// every refresh.
pub(crate) fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            // Yahoo answers a bare client with 429.
            .user_agent("Mozilla/5.0 (Macintosh) Aloud")
            .build()
            .unwrap_or_default()
    })
}
