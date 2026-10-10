//! Totals of the history's requests for reports: by model, by day, by session and by project.
//!
//! API-equivalent cost covers priced requests only; requests to unpriced models are counted
//! apart, so that a missing price never shows as free.

use std::collections::{BTreeMap, HashMap};

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;

use crate::history::{History, HistoryError, ProjectKey, SessionKey, StoredRequest};
use crate::pricing::{self, Cost, PriceTable};
use crate::tokens::Tokens;

/// Requests added up.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Totals {
    pub requests: u64,
    pub tokens: Tokens,
    /// The API-equivalent cost of the priced requests.
    pub cost: Cost,
    /// Requests to models the price table does not list, whose cost is unknown.
    pub unpriced_requests: u64,
    pub web_search_requests: u64,
}

impl Totals {
    fn add(&mut self, request: &StoredRequest<'_>, cost: Option<Cost>) {
        self.requests += 1;
        self.tokens.add(request.tokens);
        self.web_search_requests += request.web_search_requests;
        match cost {
            Some(cost) => self.cost.add(cost),
            None => self.unpriced_requests += 1,
        }
    }

    fn merge(&mut self, other: &Self) {
        self.requests += other.requests;
        self.tokens.add(other.tokens);
        self.cost.add(other.cost);
        self.unpriced_requests += other.unpriced_requests;
        self.web_search_requests += other.web_search_requests;
    }
}

/// One model's totals.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelTotals {
    /// The model id, as Claude Code records it.
    pub model: String,
    pub totals: Totals,
}

/// One day's totals, with the models used that day.
#[derive(Debug, Clone, PartialEq)]
pub struct DayTotals {
    pub day: Date,
    pub totals: Totals,
    /// Model ids, ordered by API-equivalent cost, highest first.
    pub models: Vec<ModelTotals>,
}

/// A stretch of time to report on: from `from_ms` up to, but not including, `to_ms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub from_ms: i64,
    pub to_ms: i64,
}

impl Range {
    /// The whole of the given days in a time zone, from the start of `first` to the end of
    /// `last`.
    #[must_use]
    pub fn days(first: Date, last: Date, zone: &TimeZone) -> Self {
        let start = |day: Date| {
            day.to_zoned(zone.clone())
                .map_or(i64::MIN, |zoned| zoned.timestamp().as_millisecond())
        };
        let end = last.tomorrow().map_or(i64::MAX, start);
        Self {
            from_ms: start(first),
            to_ms: end,
        }
    }
}

fn price(table: &PriceTable, request: &StoredRequest<'_>) -> Option<Cost> {
    table.cost(&pricing::Request {
        model: request.model,
        timestamp_ms: request.timestamp_ms,
        tokens: request.tokens,
        speed: request.speed,
        inference_geo: request.inference_geo,
        web_search_requests: request.web_search_requests,
    })
}

fn by_cost(models: HashMap<String, Totals>) -> Vec<ModelTotals> {
    let mut models: Vec<ModelTotals> = models
        .into_iter()
        .map(|(model, totals)| ModelTotals { model, totals })
        .collect();
    models.sort_by(|a, b| {
        b.totals
            .cost
            .total()
            .total_cmp(&a.totals.cost.total())
            .then_with(|| b.totals.requests.cmp(&a.totals.requests))
            .then_with(|| a.model.cmp(&b.model))
    });
    models
}

/// The totals of the requests in `range`, by model, highest API-equivalent cost first.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be read.
pub fn by_model(
    history: &History,
    table: &PriceTable,
    range: Range,
) -> Result<Vec<ModelTotals>, HistoryError> {
    let mut models: HashMap<String, Totals> = HashMap::new();
    history.for_each_request(range.from_ms, range.to_ms, |request| {
        models
            .entry(request.model.to_owned())
            .or_default()
            .add(request, price(table, request));
    })?;
    Ok(by_cost(models))
}

/// The totals of the requests in `range`, by day in `zone`, oldest day first. Days without
/// requests are left out.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be read.
pub fn by_day(
    history: &History,
    table: &PriceTable,
    range: Range,
    zone: &TimeZone,
) -> Result<Vec<DayTotals>, HistoryError> {
    let mut days: BTreeMap<Date, HashMap<String, Totals>> = BTreeMap::new();
    history.for_each_request(range.from_ms, range.to_ms, |request| {
        let Ok(at) = Timestamp::from_millisecond(request.timestamp_ms) else {
            return;
        };
        days.entry(at.to_zoned(zone.clone()).date())
            .or_default()
            .entry(request.model.to_owned())
            .or_default()
            .add(request, price(table, request));
    })?;
    Ok(days
        .into_iter()
        .map(|(day, models)| {
            let mut totals = Totals::default();
            for model in models.values() {
                totals.merge(model);
            }
            DayTotals {
                day,
                totals,
                models: by_cost(models),
            }
        })
        .collect())
}

/// One session's totals.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionTotals {
    pub session: SessionKey,
    pub project: ProjectKey,
    /// When its first and last requests in the range were made.
    pub first_ms: i64,
    pub last_ms: i64,
    pub totals: Totals,
    /// Requests that subagents made, a part of `totals`.
    pub subagents: Totals,
}

/// The totals of the requests in `range`, by session, most recent first.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be read.
pub fn by_session(
    history: &History,
    table: &PriceTable,
    range: Range,
) -> Result<Vec<SessionTotals>, HistoryError> {
    let mut sessions: HashMap<SessionKey, SessionTotals> = HashMap::new();
    history.for_each_request(range.from_ms, range.to_ms, |request| {
        let cost = price(table, request);
        let entry = sessions
            .entry(request.session)
            .or_insert_with(|| SessionTotals {
                session: request.session,
                project: request.project,
                first_ms: request.timestamp_ms,
                last_ms: request.timestamp_ms,
                totals: Totals::default(),
                subagents: Totals::default(),
            });
        entry.first_ms = entry.first_ms.min(request.timestamp_ms);
        entry.last_ms = entry.last_ms.max(request.timestamp_ms);
        entry.totals.add(request, cost);
        if request.subagent_id.is_some() {
            entry.subagents.add(request, cost);
        }
    })?;
    let mut sessions: Vec<SessionTotals> = sessions.into_values().collect();
    sessions.sort_by_key(|session| std::cmp::Reverse(session.last_ms));
    Ok(sessions)
}

/// One project's totals.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectTotals {
    pub project: ProjectKey,
    pub sessions: u64,
    pub last_ms: i64,
    pub totals: Totals,
}

/// The totals of the requests in `range`, by project, highest API-equivalent cost first.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be read.
pub fn by_project(
    history: &History,
    table: &PriceTable,
    range: Range,
) -> Result<Vec<ProjectTotals>, HistoryError> {
    let mut projects: HashMap<ProjectKey, (ProjectTotals, std::collections::HashSet<SessionKey>)> =
        HashMap::new();
    history.for_each_request(range.from_ms, range.to_ms, |request| {
        let (entry, sessions) = projects.entry(request.project).or_insert_with(|| {
            (
                ProjectTotals {
                    project: request.project,
                    sessions: 0,
                    last_ms: request.timestamp_ms,
                    totals: Totals::default(),
                },
                std::collections::HashSet::new(),
            )
        });
        entry.last_ms = entry.last_ms.max(request.timestamp_ms);
        entry.totals.add(request, price(table, request));
        sessions.insert(request.session);
    })?;
    let mut projects: Vec<ProjectTotals> = projects
        .into_values()
        .map(|(mut project, sessions)| {
            project.sessions = sessions.len() as u64;
            project
        })
        .collect();
    projects.sort_by(|a, b| {
        b.totals
            .cost
            .total()
            .total_cmp(&a.totals.cost.total())
            .then_with(|| b.last_ms.cmp(&a.last_ms))
    });
    Ok(projects)
}
