//! What media commands need decided before a script runs.

/// A Spotify link as the `spotify:` URI the app's `play track` takes. A share
/// link (`open.spotify.com/playlist/37i9…?si=…`, with or without an `intl-xx/`
/// segment) is what people copy; the app only plays URIs. None for anything
/// that is neither.
pub(crate) fn spotify_uri(link: &str) -> Option<String> {
    let link = link.trim();
    if link.starts_with("spotify:") {
        return Some(link.to_string());
    }
    let path = link
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .strip_prefix("open.spotify.com/")?;
    let mut parts = path.split(['/', '?']).filter(|p| !p.is_empty());
    let mut kind = parts.next()?;
    if kind.starts_with("intl-") {
        kind = parts.next()?;
    }
    let id = parts.next()?;
    matches!(
        kind,
        "track" | "album" | "playlist" | "artist" | "show" | "episode"
    )
    .then(|| format!("spotify:{kind}:{id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_links_become_uris_and_uris_pass_through() {
        assert_eq!(
            spotify_uri("https://open.spotify.com/playlist/37i9dQZEVXcJZyENOWUFo7?si=abc")
                .as_deref(),
            Some("spotify:playlist:37i9dQZEVXcJZyENOWUFo7")
        );
        assert_eq!(
            spotify_uri("open.spotify.com/intl-de/track/4uLU6hMCjMI75M1A2tKUQC").as_deref(),
            Some("spotify:track:4uLU6hMCjMI75M1A2tKUQC")
        );
        assert_eq!(
            spotify_uri("spotify:album:1DFixLWuPkv3KT3TnV35m3").as_deref(),
            Some("spotify:album:1DFixLWuPkv3KT3TnV35m3")
        );
    }

    #[test]
    fn other_links_and_words_are_not_uris() {
        assert_eq!(spotify_uri("https://youtube.com/watch?v=x"), None);
        assert_eq!(spotify_uri("discover weekly"), None);
        assert_eq!(spotify_uri("https://open.spotify.com/user/someone"), None);
    }
}
