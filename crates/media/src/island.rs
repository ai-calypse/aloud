//! The closed notch's media islands: one per source that is playing, each with
//! its own id, cover and Pause button, so Spotify and a YouTube tab playing
//! together are two things beside the camera and not one that flickers between
//! them. What is playing is [`crate::now_playing::read`]'s; this decides what
//! the queue should hold, and `src-tauri/src/media.rs` applies it.

use std::collections::HashMap;

use aloud_islands::{Island, IslandAction, IslandKind, IslandPriority};

use crate::now_playing::{NowPlaying, PlayState, Player};

/// The most the closed notch is worth spending on one kind of thing: the pill
/// draws four before it counts the rest.
pub const MAX: usize = 4;

/// Stable for as long as the source plays, whatever it plays: the island is
/// the source, not the track. A tab is its window and tab index, as
/// `Source::id` has it.
pub fn id(now: &NowPlaying) -> String {
    let s = &now.source;
    match now.player {
        Player::Spotify => "media:spotify".into(),
        Player::Music => "media:music".into(),
        Player::Browser => format!(
            "media:browser:{}:{}",
            s.app.replace(' ', "-"),
            s.tab.as_deref().unwrap_or_default().replace('/', ":")
        ),
        Player::MediaRemote => format!(
            "media:mediaremote:{}",
            s.bundle_id.as_deref().unwrap_or(&s.app)
        ),
    }
}

/// The island's Pause button, naming the source it drives so `actions.ts`
/// pauses this one and not whichever the view happens to be showing: the
/// app's name for the players and browsers, `mediaRemote` for the rest, and a
/// browser's tab and host besides.
fn pause(now: &NowPlaying) -> IslandAction {
    let action = IslandAction::new("Pause", "media.toggle");
    match now.player {
        Player::MediaRemote => action.arg("player", "mediaRemote"),
        Player::Browser => {
            let mut action = action.arg("player", now.source.app.as_str());
            if let Some(tab) = &now.source.tab {
                action = action.arg("tab", tab.as_str());
            }
            if let Some(host) = &now.source.host {
                action = action.arg("host", host.as_str());
            }
            action
        }
        Player::Spotify | Player::Music => action.arg("player", now.source.app.as_str()),
    }
}

pub fn island(now: &NowPlaying, art: Option<String>) -> Island {
    Island {
        id: id(now),
        kind: IslandKind::Live,
        priority: IslandPriority::Low,
        icon: art.or_else(|| {
            matches!(now.player, Player::Spotify | Player::Music).then(|| "music".into())
        }),
        // The row in Now › Overview reads artist over song; the pill, which
        // has one short value, shows the song.
        title: if now.artist.is_empty() {
            now.title.clone()
        } else {
            now.artist.clone()
        },
        detail: Some(now.title.clone()),
        actions: vec![pause(now)],
        expires: None,
        dismissible: false,
        ..Island::default()
    }
}

/// What to do to the queue, and the covers to go and fetch.
#[derive(Debug, Default, PartialEq)]
pub struct Sync {
    pub remove: Vec<String>,
    pub push: Vec<Island>,
    /// Tracks whose cover is new or has just changed. The cover can take a
    /// network round trip, so the island goes up without it and takes it when
    /// it arrives.
    pub art: Vec<NowPlaying>,
}

/// What the notch is showing, for the next poll to be compared with.
#[derive(Debug, Default)]
pub struct Shown {
    by_id: HashMap<String, NowPlaying>,
}

impl Shown {
    /// `playing` is `Reading::playing`: most recently started first. The first
    /// [`MAX`] distinct sources get an island; a source that paused or went
    /// away, or that a newer one pushed past the cap, loses its.
    pub fn sync(&mut self, playing: &[NowPlaying]) -> Sync {
        let mut wanted: Vec<(String, &NowPlaying)> = Vec::new();
        for now in playing.iter().filter(|n| n.state == PlayState::Playing) {
            let id = id(now);
            if wanted.len() < MAX && !wanted.iter().any(|(w, _)| *w == id) {
                wanted.push((id, now));
            }
        }

        let mut out = Sync::default();
        self.by_id.retain(|id, _| {
            let keep = wanted.iter().any(|(w, _)| w == id);
            if !keep {
                out.remove.push(id.clone());
            }
            keep
        });
        out.remove.sort();

        for (id, now) in wanted {
            let before = self.by_id.get(&id);
            let same_track = before.is_some_and(|s| s.key() == now.key());
            // A MediaRemote cover arrives after the title: same track, new cover.
            let same_cover = before.is_some_and(|s| s.artwork_url == now.artwork_url);
            if same_track && same_cover {
                continue;
            }
            if !same_track {
                out.push.push(island(now, None));
            }
            out.art.push(now.clone());
            self.by_id.insert(id, now.clone());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::now_playing::Source;

    fn playing(player: Player, app: &str, title: &str) -> NowPlaying {
        NowPlaying {
            player,
            source: Source {
                app: app.into(),
                bundle_id: None,
                host: None,
                tab: None,
            },
            state: PlayState::Playing,
            title: title.into(),
            artist: "Artist".into(),
            album: String::new(),
            duration_ms: 200_000,
            position_ms: 0,
            speed_pct: 100,
            artwork_url: None,
            song: None,
        }
    }

    fn spotify(title: &str) -> NowPlaying {
        playing(Player::Spotify, "Spotify", title)
    }

    fn tab(id: &str, title: &str) -> NowPlaying {
        let mut n = playing(Player::Browser, "Brave Browser", title);
        n.source.tab = Some(id.into());
        n.source.host = Some("youtube.com".into());
        n
    }

    fn pushed(sync: &Sync) -> Vec<&str> {
        sync.push.iter().map(|i| i.id.as_str()).collect()
    }

    /// The failure: Spotify and a YouTube tab playing together showed as one
    /// island that swapped between them.
    #[test]
    fn two_sources_playing_are_two_islands_with_their_own_ids() {
        let mut shown = Shown::default();
        let sync = shown.sync(&[tab("1/2", "Video"), spotify("Song")]);
        assert_eq!(
            pushed(&sync),
            ["media:browser:Brave-Browser:1:2", "media:spotify"]
        );
        assert_eq!(sync.art.len(), 2);
        assert!(sync.remove.is_empty());
    }

    #[test]
    fn a_source_that_pauses_loses_only_its_own_island() {
        let mut shown = Shown::default();
        shown.sync(&[tab("1/2", "Video"), spotify("Song")]);

        let mut paused = tab("1/2", "Video");
        paused.state = PlayState::Paused;
        let sync = shown.sync(&[spotify("Song")]);
        assert_eq!(sync.remove, ["media:browser:Brave-Browser:1:2"]);
        assert!(
            sync.push.is_empty() && sync.art.is_empty(),
            "Spotify is untouched"
        );

        // A paused source still listed does not hold its island either.
        let mut shown = Shown::default();
        shown.sync(&[tab("1/2", "Video")]);
        assert_eq!(
            shown.sync(&[paused]).remove,
            ["media:browser:Brave-Browser:1:2"]
        );
    }

    #[test]
    fn a_poll_that_changes_nothing_changes_nothing() {
        let mut shown = Shown::default();
        shown.sync(&[spotify("Song")]);
        let mut later = spotify("Song");
        later.position_ms = 30_000;
        assert_eq!(shown.sync(&[later]), Sync::default());
    }

    /// The cover arriving late updates that island's art and nothing else.
    #[test]
    fn a_late_cover_refetches_without_replacing_the_island() {
        let mut shown = Shown::default();
        shown.sync(&[spotify("Song"), tab("1/1", "Video")]);
        let mut covered = spotify("Song");
        covered.artwork_url = Some("https://i.scdn.co/x".into());
        let sync = shown.sync(&[covered.clone(), tab("1/1", "Video")]);
        assert!(sync.push.is_empty());
        assert_eq!(sync.art, [covered]);
    }

    #[test]
    fn a_new_track_on_the_same_source_updates_its_island_in_place() {
        let mut shown = Shown::default();
        shown.sync(&[spotify("One")]);
        let sync = shown.sync(&[spotify("Two")]);
        assert_eq!(pushed(&sync), ["media:spotify"]);
        assert_eq!(sync.push[0].detail.as_deref(), Some("Two"));
        assert!(sync.remove.is_empty());
    }

    /// A source listed twice is still one island. (MediaRemote and AppleScript
    /// describing one app is `now_playing`'s to dedupe, and is tested there.)
    #[test]
    fn one_source_is_one_island_however_often_it_is_listed() {
        let mut shown = Shown::default();
        let sync = shown.sync(&[spotify("Song"), tab("1/1", "Video"), spotify("Song")]);
        assert_eq!(
            pushed(&sync),
            ["media:spotify", "media:browser:Brave-Browser:1:1"]
        );
    }

    #[test]
    fn at_most_four_islands_the_most_recently_started_kept() {
        let mut shown = Shown::default();
        let five = |first: &str| {
            vec![
                tab(first, "e"),
                tab("1/4", "d"),
                tab("1/3", "c"),
                tab("1/2", "b"),
                tab("1/1", "a"),
            ]
        };
        let sync = shown.sync(&five("1/5"));
        assert_eq!(sync.push.len(), MAX);
        assert!(
            !pushed(&sync).contains(&"media:browser:Brave-Browser:1:1"),
            "the oldest is the one left out"
        );

        // A newcomer at the head pushes the oldest island out.
        let mut shown = Shown::default();
        shown.sync(&five("1/5")[1..]);
        let sync = shown.sync(&five("1/5"));
        assert_eq!(pushed(&sync), ["media:browser:Brave-Browser:1:5"]);
        assert_eq!(sync.remove, ["media:browser:Brave-Browser:1:1"]);
    }

    #[test]
    fn media_islands_rank_low() {
        assert_eq!(island(&spotify("Song"), None).priority, IslandPriority::Low);
    }

    /// The failure: every island's button pausing whichever source the view
    /// was showing. The button must say which one it means.
    #[test]
    fn an_islands_button_names_its_source() {
        let args = |n: &NowPlaying| island(n, None).actions[0].args.clone();
        let s = args(&spotify("Song"));
        assert_eq!(s["player"], "Spotify");
        assert!(!s.contains_key("tab"));

        let t = args(&tab("2/3", "Video"));
        assert_eq!(t["player"], "Brave Browser");
        assert_eq!(t["tab"], "2/3");
        assert_eq!(t["host"], "youtube.com");

        let mr = playing(Player::MediaRemote, "VLC", "Film");
        assert_eq!(args(&mr)["player"], "mediaRemote");

        let a = island(&tab("2/3", "Video"), None).actions[0].clone();
        assert_eq!(
            (a.label.as_str(), a.action.as_str()),
            ("Pause", "media.toggle")
        );
    }
}
