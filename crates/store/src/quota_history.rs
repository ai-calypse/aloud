//! A week of plan-usage readings, for the Usage view's charts.
//!
//! The usage endpoint only says where each window is now. The time series is
//! the app's own record of it: one sample per poll, thinned to one per
//! [`MIN_GAP_SECS`], kept for the length of the longest window.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// A poll every minute would be 10,000 points a week; one per five is plenty
/// for a line a few hundred pixels wide.
const MIN_GAP_SECS: i64 = 300;
/// The weekly window's length: older samples describe a window that is over.
const KEEP_SECS: i64 = 7 * 86_400;

/// One reading of the two windows that have a chart. See
/// [`crate::history::Entry`] on why the i64 exports as `number`.
///
/// Each window carries the reset time it was reading. Two samples with the
/// same reset belong to one window, where use only climbs, so the chart can
/// join them across the hours the view was closed; a different reset means the
/// window rolled over in between, and the line must break there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct QuotaSample {
    /// Unix seconds.
    #[specta(type = specta_typescript::Number)]
    pub at: i64,
    pub five_hour: Option<f64>,
    /// Unix seconds. Absent in samples saved before this field existed.
    #[serde(default)]
    #[specta(type = Option<specta_typescript::Number>)]
    pub five_hour_resets_at: Option<i64>,
    pub seven_day: Option<f64>,
    #[serde(default)]
    #[specta(type = Option<specta_typescript::Number>)]
    pub seven_day_resets_at: Option<i64>,
}

/// `<app-data>/Aloud/quota-history.json`. Its own file: rewritten each poll,
/// and a torn write must not take settings with it.
pub fn path() -> Result<PathBuf> {
    Ok(aloud_notes::data_dir()?.join("quota-history.json"))
}

/// Samples on disk, oldest first; empty when absent or corrupt, since losing a
/// chart must never fail a usage read.
pub fn load() -> Vec<QuotaSample> {
    path().map(|p| load_from(&p)).unwrap_or_default()
}

/// Adds `sample` to the file and returns the whole series.
pub fn record(sample: QuotaSample) -> Result<Vec<QuotaSample>> {
    record_to(&path()?, sample)
}

fn load_from(path: &Path) -> Vec<QuotaSample> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn record_to(path: &Path, sample: QuotaSample) -> Result<Vec<QuotaSample>> {
    let mut series = load_from(path);
    let due = series
        .last()
        .is_none_or(|last| sample.at - last.at >= MIN_GAP_SECS);
    if due {
        series.push(sample.clone());
    }
    series.retain(|s| sample.at - s.at <= KEEP_SECS);
    if due {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let temp = path.with_extension("tmp");
        fs::write(&temp, serde_json::to_string(&series)?)?;
        fs::rename(&temp, path)?;
    }
    Ok(series)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: i64) -> QuotaSample {
        QuotaSample {
            at: secs,
            five_hour: Some(10.0),
            five_hour_resets_at: None,
            seven_day: None,
            seven_day_resets_at: None,
        }
    }

    /// Guards: a poll every minute writing a point every minute, and the file
    /// growing for ever.
    #[test]
    fn thins_polls_and_drops_what_a_week_has_outlived() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("quota-history.json");

        record_to(&file, at(0)).unwrap();
        assert_eq!(record_to(&file, at(60)).unwrap().len(), 1, "inside the gap");
        assert_eq!(record_to(&file, at(MIN_GAP_SECS)).unwrap().len(), 2);

        let later = record_to(&file, at(KEEP_SECS + 1)).unwrap();
        assert_eq!(
            later.iter().map(|s| s.at).collect::<Vec<_>>(),
            [MIN_GAP_SECS, KEEP_SECS + 1]
        );
        assert_eq!(
            load_from(&file),
            later,
            "what was returned is what was saved"
        );
    }

    /// Guards: a history file written before the reset fields existed failing
    /// to load, which would drop a week of readings on upgrade.
    #[test]
    fn samples_saved_before_the_reset_fields_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("quota-history.json");
        fs::write(&file, r#"[{"at":5,"fiveHour":10.0,"sevenDay":null}]"#).unwrap();
        let series = load_from(&file);
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].five_hour_resets_at, None);
    }

    #[test]
    fn an_unreadable_file_reads_as_no_history() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("quota-history.json");
        fs::write(&file, "{nope").unwrap();
        assert!(load_from(&file).is_empty());
        assert_eq!(record_to(&file, at(5)).unwrap().len(), 1);
    }
}
