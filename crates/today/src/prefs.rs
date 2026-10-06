//! `today.json`: the city the weather is for and the symbols the ticker
//! follows. Its own small file rather than a corner of settings.json, so this
//! section can change without touching what every other feature reads.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// None means "wherever this IP is", which is coarse and wrong on a VPN.
    pub city: Option<Place>,
    pub stocks: Vec<String>,
}

pub fn path() -> Result<PathBuf> {
    Ok(aloud_notes::data_dir()?.join("today.json"))
}

impl Prefs {
    pub fn load() -> Self {
        path().map(|p| Self::load_from(&p)).unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&path()?)
    }

    /// Defaults for a missing or corrupt file: losing the watchlist must not
    /// stop the section opening.
    fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Temp + rename, so a crash mid-write cannot leave half a file.
    fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_survives_a_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("today.json");
        assert_eq!(Prefs::load_from(&file), Prefs::default());

        let prefs = Prefs {
            city: Some(Place {
                name: "Toronto".into(),
                latitude: 43.7,
                longitude: -79.4,
            }),
            stocks: vec!["AAPL".into()],
        };
        prefs.save_to(&file).unwrap();
        assert_eq!(Prefs::load_from(&file), prefs);

        std::fs::write(&file, "{ not json").unwrap();
        assert_eq!(Prefs::load_from(&file), Prefs::default());
    }
}
