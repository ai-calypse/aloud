//! A track's cover as a `data:` URI, the only image the overlay's CSP loads.
//! The webview cannot fetch Spotify's CDN itself, so Rust does.

use std::sync::Mutex;
use std::time::Duration;

use base64::Engine;

use crate::now_playing::{music_artwork, NowPlaying, Player};

/// A cover is ~100 KB; a bigger body is not a cover and is not worth holding.
const MAX_BYTES: usize = 2 * 1024 * 1024;

/// The last track's cover, so a re-read of the same track costs nothing.
/// ponytail: one entry; the island and the view both ask for the track that
/// is playing, and a second player's cover is never wanted at the same time.
static LAST: Mutex<Option<(String, Option<String>)>> = Mutex::new(None);

/// The cover for `now`, or None when it has none or the fetch failed. A
/// failure is remembered for the track too, so a dead URL is tried once.
pub async fn artwork(now: &NowPlaying) -> Option<String> {
    let key = now.key();
    if let Some((k, uri)) = LAST.lock().ok()?.as_ref() {
        if *k == key {
            return uri.clone();
        }
    }
    let uri = match (now.player, &now.artwork_url) {
        (Player::Spotify, Some(url)) => fetch(url).await,
        (Player::Music, _) => tokio::task::spawn_blocking(music_artwork)
            .await
            .ok()
            .flatten()
            .and_then(|bytes| data_uri(&bytes)),
        _ => None,
    };
    if let Ok(mut last) = LAST.lock() {
        *last = Some((key, uri.clone()));
    }
    uri
}

async fn fetch(url: &str) -> Option<String> {
    // Only Spotify's own https CDN: the URL came out of another app's
    // scripting reply, and this must not become a fetch-anything primitive.
    if !url.starts_with("https://") {
        return None;
    }
    let response = reqwest::Client::new()
        .get(url)
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let bytes = response.bytes().await.ok()?;
    data_uri(&bytes)
}

/// `bytes` as an image `data:` URI. The type comes from the file's first
/// bytes, not a header (Music's raw data has none, and a CDN's can be generic);
/// a body that is neither a PNG nor a JPEG is dropped.
fn data_uri(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return None;
    }
    let sniffed = if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else {
        return None;
    };
    Some(format!(
        "data:{sniffed};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_png_and_jpeg_bodies_become_uris() {
        assert!(data_uri(&[0xFF, 0xD8, 0xFF, 0xE0, 1])
            .unwrap()
            .starts_with("data:image/jpeg;base64,"));
        assert!(data_uri(&[0x89, b'P', b'N', b'G', 1])
            .unwrap()
            .starts_with("data:image/png;base64,"));
        assert_eq!(data_uri(b"<html>not an image</html>"), None);
        assert_eq!(data_uri(&[]), None);
    }
}
