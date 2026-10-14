//! Reconciliation: comparing the API-equivalent cost of each session with the cost Claude Code
//! reports for it, so that a stale or missing price shows up as a warning instead of a silently
//! wrong number (ADR 0004).
//!
//! Only sessions and models whose tokens agree on both sides are compared, so that the
//! comparison measures prices rather than coverage. The tokens disagree where Claude Code reports
//! calls that no transcript records, such as the ones that give sessions their titles, and where
//! it wrote its last cost before a session's last requests.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::BuildHasher;

use crate::history::{History, HistoryError, SessionKey, StoredReportedCost};
use crate::pricing::{self, PriceTable};
use crate::report::ModelTotals;
use crate::tokens::Tokens;

/// How old a session's last request must be before the session is compared, so that Claude Code
/// has written its final cost for it.
const SETTLED_AFTER_MS: i64 = 60 * 60 * 1000;

/// How far a model's API-equivalent cost may drift from the cost Claude Code reports before its
/// prices may be out of date. Matching sessions agree within about 1%, because Claude Code counts
/// a few input tokens that transcripts do not record.
pub const DRIFT_LIMIT: f64 = 0.02;

/// The smallest reported cost that a model's drift is judged on, so that a handful of small
/// requests cannot raise a warning.
pub const DRIFT_MINIMUM_USD: f64 = 1.0;

/// One model's API-equivalent cost next to the cost Claude Code reports for it, over the sessions
/// compared.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelReconciliation {
    /// The model id, as the history's requests record it.
    pub model: String,
    pub sessions: u64,
    /// The API-equivalent cost of the model's requests in those sessions; `None` for an unpriced
    /// model.
    pub api_equivalent_usd: Option<f64>,
    pub reported_usd: f64,
}

impl ModelReconciliation {
    /// How far the API-equivalent cost is from the reported cost, as a fraction of the reported
    /// cost; `None` for an unpriced model or a reported cost of zero.
    #[must_use]
    pub fn drift(&self) -> Option<f64> {
        relative(self.api_equivalent_usd?, self.reported_usd)
    }
}

/// How the history's API-equivalent costs compare with the costs Claude Code reports.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reconciliation {
    /// Sessions with at least one model compared.
    pub sessions: u64,
    /// By model, ordered by model id.
    pub models: Vec<ModelReconciliation>,
    /// Session and model pairs left out because their tokens disagree.
    pub not_comparable: u64,
}

impl Reconciliation {
    /// How far the total API-equivalent cost of the priced models is from the total Claude Code
    /// reports for them, as a fraction of the reported total; `None` with nothing to compare.
    #[must_use]
    pub fn drift(&self) -> Option<f64> {
        let (ours, reported) = self
            .models
            .iter()
            .filter_map(|model| Some((model.api_equivalent_usd?, model.reported_usd)))
            .fold((0.0, 0.0), |(ours, reported), (a, b)| {
                (ours + a, reported + b)
            });
        relative(ours, reported)
    }

    /// The models whose prices may be out of date: those drifting beyond [`DRIFT_LIMIT`] on at
    /// least [`DRIFT_MINIMUM_USD`] of reported cost.
    pub fn out_of_line(&self) -> impl Iterator<Item = &ModelReconciliation> {
        self.models.iter().filter(|model| {
            model.reported_usd >= DRIFT_MINIMUM_USD
                && model.drift().is_some_and(|drift| drift.abs() > DRIFT_LIMIT)
        })
    }
}

fn relative(ours: f64, reported: f64) -> Option<f64> {
    (reported > 0.0).then(|| (ours - reported) / reported)
}

/// What Claude Code reports for one model of a session, over the processes that have written
/// their cost: what they cost, and the tokens they counted, which do not tell five-minute and
/// one-hour cache writes apart.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Reported {
    pub cost_usd: f64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

struct Ours {
    /// `None` once a request turns out to be unpriced.
    cost: Option<f64>,
    tokens: Tokens,
}

/// Whether the history and Claude Code count the same requests: output, cache reads and cache
/// writes within 2% or 50 tokens of each other. Input is left out, because Claude Code's count
/// includes calls that transcripts do not record.
fn same_requests(ours: &Tokens, reported: &Reported) -> bool {
    let close = |a: u64, b: u64| a.abs_diff(b) <= 50.max(a.max(b) / 50);
    close(ours.output, reported.output)
        && close(ours.cache_read, reported.cache_read)
        && close(
            ours.cache_write_5m.saturating_add(ours.cache_write_1h),
            reported.cache_write,
        )
}

/// Claude Code reports a model's context variant under its own name, such as
/// `claude-opus-5[1m]`, while transcripts record the model id the API returned.
fn without_variant(model: &str) -> &str {
    model.split_once('[').map_or(model, |(id, _)| id)
}

/// What Claude Code reports for each session, by model as the history's requests name it.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be read.
pub fn reported(
    history: &History,
) -> Result<HashMap<SessionKey, HashMap<String, Reported>>, HistoryError> {
    let mut reported: HashMap<SessionKey, HashMap<String, Reported>> = HashMap::new();
    for cost in history.reported_costs()? {
        add(reported.entry(cost.session).or_default(), &cost);
    }
    Ok(reported)
}

/// What Claude Code reports for one session, by model as the history's requests name it: empty
/// when it reports nothing for the session.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be read.
pub fn reported_for(
    history: &History,
    session: SessionKey,
) -> Result<HashMap<String, Reported>, HistoryError> {
    let mut reported = HashMap::new();
    for cost in history.session_reported_costs(session)? {
        add(&mut reported, &cost);
    }
    Ok(reported)
}

/// Adds one of Claude Code's figures to a session's, under the model's name without its variant.
fn add(models: &mut HashMap<String, Reported>, cost: &StoredReportedCost) {
    let entry = models
        .entry(without_variant(&cost.model).to_owned())
        .or_default();
    entry.cost_usd += cost.cost_usd;
    entry.output += cost.output;
    entry.cache_read += cost.cache_read;
    entry.cache_write += cost.cache_write;
}

/// How far a session's API-equivalent cost is from Claude Code's own figure for the same
/// requests, as a fraction of that figure: `models` are the session's models as the history
/// holds them, and `reported` what Claude Code reports for the session.
///
/// Claude Code writes its figure as each of its processes ends, so the figure counts other
/// requests while a process of the session still runs, or where the transcripts miss some of its
/// calls; then, as for an unpriced model, there is nothing to compare.
pub fn session_drift<'a, S: BuildHasher>(
    models: impl IntoIterator<Item = &'a ModelTotals>,
    reported: &HashMap<String, Reported, S>,
) -> Option<f64> {
    let (mut ours, mut theirs) = (0.0, 0.0);
    for model in models {
        let figure = reported.get(&model.model)?;
        if model.totals.unpriced_requests > 0 || !same_requests(&model.totals.tokens, figure) {
            return None;
        }
        ours += model.totals.cost.total();
        theirs += figure.cost_usd;
    }
    relative(ours, theirs)
}

/// Compares the history's API-equivalent costs with the costs Claude Code reports, as of
/// `now_ms`.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be read.
pub fn reconcile(
    history: &History,
    table: &PriceTable,
    now_ms: i64,
) -> Result<Reconciliation, HistoryError> {
    let reported: HashMap<(SessionKey, String), Reported> = reported(history)?
        .into_iter()
        .flat_map(|(session, models)| {
            models
                .into_iter()
                .map(move |(model, figure)| ((session, model), figure))
        })
        .collect();

    let mut ours: HashMap<(SessionKey, String), Ours> = HashMap::new();
    let mut last_request: HashMap<SessionKey, i64> = HashMap::new();
    history.for_each_request(i64::MIN, i64::MAX, |request| {
        let last = last_request.entry(request.session).or_insert(i64::MIN);
        *last = (*last).max(request.timestamp_ms);
        let key = (request.session, request.model.to_owned());
        if !reported.contains_key(&key) {
            return;
        }
        let cost = table.cost(&pricing::Request {
            model: request.model,
            timestamp_ms: request.timestamp_ms,
            tokens: request.tokens,
            speed: request.speed,
            inference_geo: request.inference_geo,
            web_search_requests: request.web_search_requests,
        });
        let entry = ours.entry(key).or_insert(Ours {
            cost: Some(0.0),
            tokens: Tokens::default(),
        });
        entry.tokens.add(request.tokens);
        entry.cost = entry
            .cost
            .zip(cost)
            .map(|(total, cost)| total + cost.total());
    })?;

    let settled_before = now_ms.saturating_sub(SETTLED_AFTER_MS);
    let mut sessions = HashSet::new();
    let mut models: BTreeMap<String, ModelReconciliation> = BTreeMap::new();
    let mut not_comparable = 0;
    for ((session, model), reported) in reported {
        if last_request
            .get(&session)
            .is_some_and(|&last| last > settled_before)
        {
            continue;
        }
        let Some(ours) = ours
            .get(&(session, model.clone()))
            .filter(|ours| same_requests(&ours.tokens, &reported))
        else {
            not_comparable += 1;
            continue;
        };
        sessions.insert(session);
        let entry = models
            .entry(model.clone())
            .or_insert_with(|| ModelReconciliation {
                model,
                sessions: 0,
                api_equivalent_usd: Some(0.0),
                reported_usd: 0.0,
            });
        entry.sessions += 1;
        entry.api_equivalent_usd = entry
            .api_equivalent_usd
            .zip(ours.cost)
            .map(|(total, cost)| total + cost);
        entry.reported_usd += reported.cost_usd;
    }

    Ok(Reconciliation {
        sessions: sessions.len() as u64,
        models: models.into_values().collect(),
        not_comparable,
    })
}
