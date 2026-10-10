//! Claude Code's statistics file, `stats-cache.json`, whose daily totals outlive the transcripts
//! they were counted from.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::Deserialize;

/// A model's tokens on one day, without request detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyTotal {
    /// The day as Claude Code dates it, `YYYY-MM-DD`.
    pub day: String,
    pub model: String,
    pub tokens: u64,
}

/// Why the statistics file cannot be read.
#[derive(Debug, thiserror::Error)]
pub enum StatsCacheError {
    #[error("cannot read {path}: {source}")]
    File { path: String, source: io::Error },
    #[error("cannot read {path}: {source}")]
    Json {
        path: String,
        source: serde_json::Error,
    },
}

#[derive(Deserialize)]
struct StatsCache {
    #[serde(rename = "dailyModelTokens", default)]
    daily_model_tokens: Vec<Day>,
}

#[derive(Deserialize)]
struct Day {
    date: String,
    #[serde(rename = "tokensByModel", default)]
    tokens_by_model: BTreeMap<String, u64>,
}

/// The daily totals in a data root's statistics file; none when the file does not exist.
///
/// # Errors
///
/// [`StatsCacheError`] when the file exists but cannot be read.
pub fn daily_totals(root: &Path) -> Result<Vec<DailyTotal>, StatsCacheError> {
    let path = root.join("stats-cache.json");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(StatsCacheError::File {
                path: path.display().to_string(),
                source,
            });
        }
    };
    let stats: StatsCache =
        serde_json::from_str(&text).map_err(|source| StatsCacheError::Json {
            path: path.display().to_string(),
            source,
        })?;
    Ok(stats
        .daily_model_tokens
        .into_iter()
        .filter(|day| is_day(&day.date))
        .flat_map(|day| {
            day.tokens_by_model
                .into_iter()
                .map(move |(model, tokens)| DailyTotal {
                    day: day.date.clone(),
                    model,
                    tokens,
                })
        })
        .collect())
}

fn is_day(date: &str) -> bool {
    date.parse::<jiff::civil::Date>().is_ok() && date.len() == 10
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_tokens_per_model_per_day() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("stats-cache.json"),
            r#"{"version": 5, "lastComputedDate": "2026-10-01",
                "dailyModelTokens": [
                    {"date": "2026-07-31", "tokensByModel": {"claude-opus-5": 1200, "claude-sonnet-5": 300}},
                    {"date": "not a day", "tokensByModel": {"claude-opus-5": 9}},
                    {"date": "2026-08-02", "tokensByModel": {}}
                ],
                "dailyActivity": [], "hourCounts": {"0": 1}}"#,
        )
        .unwrap();

        assert_eq!(
            daily_totals(dir.path()).unwrap(),
            [
                DailyTotal {
                    day: "2026-07-31".into(),
                    model: "claude-opus-5".into(),
                    tokens: 1200
                },
                DailyTotal {
                    day: "2026-07-31".into(),
                    model: "claude-sonnet-5".into(),
                    tokens: 300
                },
            ]
        );
    }

    #[test]
    fn a_root_without_statistics_has_no_daily_totals() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(daily_totals(dir.path()).unwrap(), []);
    }

    #[test]
    fn a_damaged_statistics_file_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("stats-cache.json"), "{ not json").unwrap();
        assert!(matches!(
            daily_totals(dir.path()),
            Err(StatsCacheError::Json { .. })
        ));
    }
}
