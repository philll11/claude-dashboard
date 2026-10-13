//! A newer price table from claude-dashboard's repository, checked for once a day when the person
//! switches the refresh on (ADR 0011).
//!
//! The table comes from `pricing/prices.json` on the repository's `main` branch. A table is used
//! only when this version reads its format, it was checked against Anthropic's pricing page no
//! earlier than the table this version ships with, and it keeps every price that table has. The
//! history keeps the table a check accepted, so that every surface prices with it, and when a
//! check last ran, so that the machine checks at most once a day.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::sync::{Arc, Mutex, PoisonError};

use crate::history::{History, HistoryError};
use crate::http;
use crate::pricing::{PriceTable, PriceTableError};

/// Where the table comes from.
pub const URL: &str =
    "https://raw.githubusercontent.com/philll11/claude-dashboard/main/pricing/prices.json";

/// The only host a check reaches, a redirect included, such as after the repository is renamed.
const HOST: &str = "raw.githubusercontent.com";

/// How often the machine checks at most.
pub const EVERY_MS: i64 = 24 * 60 * 60 * 1000;

/// How soon a check that could not reach the repository is tried again.
pub const RETRY_MS: i64 = 60 * 60 * 1000;

/// The shared setting that switches the daily check on, as `on`.
pub const SWITCH_SETTING: &str = "price_refresh";

/// The shared setting that holds the text of the table a check accepted.
pub const TABLE_SETTING: &str = "price_table";

/// The shared setting that holds the accepted table's entity tag, for asking whether it changed.
const ETAG_SETTING: &str = "price_table_etag";

/// The shared setting that holds when a check last ended, in milliseconds since the Unix epoch.
const CHECKED_SETTING: &str = "price_table_checked_ms";

/// The shared setting that holds what came of that check.
const OUTCOME_SETTING: &str = "price_table_outcome";

/// The shared setting that holds what went wrong, for an outcome that needs it.
const DETAIL_SETTING: &str = "price_table_detail";

/// The shared setting that holds the time before which no process checks, in milliseconds since
/// the Unix epoch.
const NEXT_SETTING: &str = "price_table_next_ms";

/// The shared setting that holds when the table a check accepted was downloaded, in milliseconds
/// since the Unix epoch.
const DOWNLOADED_SETTING: &str = "price_table_downloaded_ms";

/// What came of a check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// A newer table arrived, and every surface prices with it.
    Updated,
    /// The repository holds no table newer than the one in use.
    UpToDate,
    /// The repository could not be reached, or answered with an error.
    Unreachable,
    /// The repository's table is in a format that needs a newer claude-dashboard.
    NewerFormat,
    /// The repository's table cannot be used: it is not valid, is older than this version's, or
    /// changes prices this version has.
    Refused,
}

impl Outcome {
    /// The name the history stores the outcome under.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Updated => "updated",
            Self::UpToDate => "up-to-date",
            Self::Unreachable => "unreachable",
            Self::NewerFormat => "newer-format",
            Self::Refused => "refused",
        }
    }

    /// The outcome the history stores under `name`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        [
            Self::Updated,
            Self::UpToDate,
            Self::Unreachable,
            Self::NewerFormat,
            Self::Refused,
        ]
        .into_iter()
        .find(|outcome| outcome.as_str() == name)
    }
}

/// The refresh's state on this machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    /// Whether the person has switched the daily check on.
    pub enabled: bool,
    /// When a check last ended, in milliseconds since the Unix epoch.
    pub checked_ms: Option<i64>,
    /// What came of it.
    pub outcome: Option<Outcome>,
    /// What went wrong, for an outcome that needs it.
    pub detail: Option<String>,
    /// When the table a check accepted was downloaded.
    pub downloaded_ms: Option<i64>,
}

/// The refresh's state, from the settings every surface shares.
#[must_use]
pub fn state<S: BuildHasher>(settings: &HashMap<String, String, S>) -> State {
    State {
        enabled: settings
            .get(SWITCH_SETTING)
            .is_some_and(|value| value == "on"),
        checked_ms: settings
            .get(CHECKED_SETTING)
            .and_then(|value| value.parse().ok()),
        outcome: settings
            .get(OUTCOME_SETTING)
            .and_then(|name| Outcome::from_name(name)),
        detail: settings.get(DETAIL_SETTING).cloned(),
        downloaded_ms: settings
            .get(DOWNLOADED_SETTING)
            .and_then(|value| value.parse().ok()),
    }
}

/// When a check happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// When the refresh is on and the machine's daily check is due, as the server checks.
    Due,
    /// Now, whether the refresh is on or not, as the person asks with `prices update`.
    Now,
}

/// What a refresh did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// The refresh is off.
    Off,
    /// A process checked within the day, or the last check failed within the hour.
    Waiting { until_ms: i64 },
    /// This process checked, with this outcome.
    Checked(Outcome),
}

/// Switches the daily check on or off. Switched on, the next check is due at once.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be written.
pub fn switch(history: &mut History, on: bool) -> Result<(), HistoryError> {
    history.write(|w| {
        w.set_setting(SWITCH_SETTING, on.then_some("on"))?;
        if on {
            w.set_setting(NEXT_SETTING, None)?;
        }
        Ok(())
    })
}

/// Checks the repository for a newer price table and records what comes of it.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be read or written. Whatever goes wrong with the
/// request or the table is recorded as the outcome instead.
pub fn refresh(history: &mut History, now_ms: i64, when: When) -> Result<Check, HistoryError> {
    refresh_with(history, now_ms, when, fetch)
}

/// [`refresh`], with the request made by `fetch`, which gets the entity tag to send.
///
/// # Errors
///
/// As for [`refresh`].
pub fn refresh_with(
    history: &mut History,
    now_ms: i64,
    when: When,
    fetch: impl FnOnce(Option<&str>) -> Result<http::Answer, String>,
) -> Result<Check, HistoryError> {
    let settings = history.settings()?;
    let next_ms = |settings: &HashMap<String, String>| {
        settings
            .get(NEXT_SETTING)
            .and_then(|value| value.parse::<i64>().ok())
    };
    match when {
        When::Due => {
            if !state(&settings).enabled {
                return Ok(Check::Off);
            }
            if let Some(until_ms) = next_ms(&settings).filter(|&until_ms| until_ms > now_ms) {
                return Ok(Check::Waiting { until_ms });
            }
            if !history.write(|w| w.take_turn(NEXT_SETTING, now_ms, RETRY_MS))? {
                let until_ms = next_ms(&history.settings()?).unwrap_or(now_ms);
                return Ok(Check::Waiting { until_ms });
            }
        }
        When::Now => history.write(|w| {
            w.set_setting(
                NEXT_SETTING,
                Some(&now_ms.saturating_add(RETRY_MS).to_string()),
            )
        })?,
    }
    let stored = settings.get(TABLE_SETTING).map(String::as_str);
    // A stored table this version cannot read is fetched whole, whatever its tag says.
    let etag = stored
        .filter(|text| PriceTable::parse(text).is_ok())
        .and(settings.get(ETAG_SETTING).map(String::as_str));
    let checked = Checked::of(fetch(etag), &effective(stored));
    history.write(|w| {
        if let Some((text, etag)) = &checked.keep {
            w.set_setting(TABLE_SETTING, Some(text))?;
            w.set_setting(ETAG_SETTING, etag.as_deref())?;
            w.set_setting(DOWNLOADED_SETTING, Some(&now_ms.to_string()))?;
        }
        w.set_setting(CHECKED_SETTING, Some(&now_ms.to_string()))?;
        w.set_setting(OUTCOME_SETTING, Some(checked.outcome.as_str()))?;
        w.set_setting(DETAIL_SETTING, checked.detail.as_deref())?;
        let wait = if checked.outcome == Outcome::Unreachable {
            RETRY_MS
        } else {
            EVERY_MS
        };
        w.set_setting(NEXT_SETTING, Some(&now_ms.saturating_add(wait).to_string()))
    })?;
    Ok(Check::Checked(checked.outcome))
}

/// Asks the repository for its table, unless it still has the entity tag `etag`.
fn fetch(etag: Option<&str>) -> Result<http::Answer, String> {
    let mut headers = Vec::new();
    if let Some(etag) = etag {
        headers.push(("If-None-Match", etag));
    }
    http::get(&http::Get {
        url: URL,
        headers: &headers,
        redirect_hosts: &[HOST],
    })
}

/// What one check found, ready to record.
struct Checked {
    outcome: Outcome,
    detail: Option<String>,
    /// The table to keep, with its entity tag.
    keep: Option<(String, Option<String>)>,
}

impl Checked {
    fn outcome(outcome: Outcome, detail: Option<String>) -> Self {
        Self {
            outcome,
            detail,
            keep: None,
        }
    }

    fn of(fetched: Result<http::Answer, String>, in_use: &PriceTable) -> Self {
        let answer = match fetched {
            Ok(answer) => answer,
            Err(error) => return Self::outcome(Outcome::Unreachable, Some(error)),
        };
        match answer.status {
            304 => Self::outcome(Outcome::UpToDate, None),
            200 => match accept(&answer.body) {
                Ok(table) => Self {
                    outcome: if table == *in_use {
                        Outcome::UpToDate
                    } else {
                        Outcome::Updated
                    },
                    detail: None,
                    keep: Some((answer.body, answer.etag)),
                },
                Err(Refusal::NewerFormat(format)) => {
                    Self::outcome(Outcome::NewerFormat, Some(format!("format {format}")))
                }
                Err(Refusal::Unusable(reason)) => Self::outcome(Outcome::Refused, Some(reason)),
            },
            status => Self::outcome(
                Outcome::Unreachable,
                Some(format!("it answered with HTTP status {status}")),
            ),
        }
    }
}

/// Why a table cannot be used.
enum Refusal {
    NewerFormat(u32),
    Unusable(String),
}

/// The table in `text`, when it can replace the one this version ships with.
fn accept(text: &str) -> Result<PriceTable, Refusal> {
    let table = PriceTable::parse(text).map_err(|error| match error {
        PriceTableError::NewerFormat(format) => Refusal::NewerFormat(format),
        error => Refusal::Unusable(error.to_string()),
    })?;
    let bundled = PriceTable::bundled();
    if table.checked() < bundled.checked() {
        return Err(Refusal::Unusable(format!(
            "it was checked on {}, before this version's table of {}",
            table.checked(),
            bundled.checked()
        )));
    }
    table.keeps(bundled).map_err(Refusal::Unusable)?;
    Ok(table)
}

/// The table every surface prices with, from the settings every surface shares.
#[must_use]
pub fn in_use<S: BuildHasher>(settings: &HashMap<String, String, S>) -> Arc<PriceTable> {
    effective(settings.get(TABLE_SETTING).map(String::as_str))
}

/// The table a check accepted, when this version can still use it, or else the one this version
/// ships with. A table is read once, however often it is asked for.
fn effective(stored: Option<&str>) -> Arc<PriceTable> {
    static CHOSEN: Mutex<Option<(String, Arc<PriceTable>)>> = Mutex::new(None);
    let Some(text) = stored else {
        return PriceTable::bundled_shared();
    };
    let mut chosen = CHOSEN.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((known, table)) = chosen.as_ref()
        && known == text
    {
        return Arc::clone(table);
    }
    let table = accept(text).map_or_else(|_| PriceTable::bundled_shared(), Arc::new);
    *chosen = Some((text.to_owned(), Arc::clone(&table)));
    table
}

#[cfg(test)]
mod tests {
    use super::Outcome;

    #[test]
    fn outcomes_keep_their_stored_names() {
        for outcome in [
            Outcome::Updated,
            Outcome::UpToDate,
            Outcome::Unreachable,
            Outcome::NewerFormat,
            Outcome::Refused,
        ] {
            assert_eq!(Outcome::from_name(outcome.as_str()), Some(outcome));
        }
        assert_eq!(Outcome::from_name("later"), None);
    }
}
