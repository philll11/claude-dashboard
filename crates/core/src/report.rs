//! Totals of the history's requests for reports: by model, by day, by session and by project.
//!
//! API-equivalent cost covers priced requests only; requests to unpriced models are counted
//! apart, so that a missing price never shows as free.

use std::collections::{BTreeMap, HashMap, HashSet};

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
    /// Requests made in fast mode.
    pub fast_requests: u64,
}

impl Totals {
    fn add(&mut self, request: &StoredRequest<'_>, cost: Option<Cost>) {
        self.requests += 1;
        self.tokens.add(request.tokens);
        self.web_search_requests += request.web_search_requests;
        if request.speed == Some("fast") {
            self.fast_requests += 1;
        }
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
        self.fast_requests += other.fast_requests;
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
    /// How many subagents made requests.
    pub subagent_count: u64,
    /// The models it used, highest API-equivalent cost first.
    pub models: Vec<ModelTotals>,
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
    sessions_matching(history, table, range, &Filter::default())
}

/// A session's totals while they are added up, with its models and subagents so far.
type SessionSoFar = (SessionTotals, HashMap<String, Totals>, HashSet<String>);

/// The totals of the requests in `range` that `filter` admits, by session, most recent first.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be read.
pub fn sessions_matching(
    history: &History,
    table: &PriceTable,
    range: Range,
    filter: &Filter,
) -> Result<Vec<SessionTotals>, HistoryError> {
    let mut sessions: HashMap<SessionKey, SessionSoFar> = HashMap::new();
    history.for_each_request(range.from_ms, range.to_ms, |request| {
        if !filter.admits(request) {
            return;
        }
        let cost = price(table, request);
        let (entry, models, subagents) = sessions.entry(request.session).or_insert_with(|| {
            (
                SessionTotals {
                    session: request.session,
                    project: request.project,
                    first_ms: request.timestamp_ms,
                    last_ms: request.timestamp_ms,
                    totals: Totals::default(),
                    subagents: Totals::default(),
                    subagent_count: 0,
                    models: Vec::new(),
                },
                HashMap::new(),
                HashSet::new(),
            )
        });
        entry.first_ms = entry.first_ms.min(request.timestamp_ms);
        entry.last_ms = entry.last_ms.max(request.timestamp_ms);
        entry.totals.add(request, cost);
        models
            .entry(request.model.to_owned())
            .or_default()
            .add(request, cost);
        if let Some(subagent) = request.subagent_id {
            entry.subagents.add(request, cost);
            subagents.insert(subagent.to_owned());
        }
    })?;
    let mut sessions: Vec<SessionTotals> = sessions
        .into_values()
        .map(|(mut session, models, subagents)| {
            session.models = by_cost(models);
            session.subagent_count = subagents.len() as u64;
            session
        })
        .collect();
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
    let mut projects: HashMap<ProjectKey, (ProjectTotals, HashSet<SessionKey>)> = HashMap::new();
    history.for_each_request(range.from_ms, range.to_ms, |request| {
        let (entry, sessions) = projects.entry(request.project).or_insert_with(|| {
            (
                ProjectTotals {
                    project: request.project,
                    sessions: 0,
                    last_ms: request.timestamp_ms,
                    totals: Totals::default(),
                },
                HashSet::new(),
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

/// The requests in `range` that `filter` admits, added up.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be read.
pub fn totals(
    history: &History,
    table: &PriceTable,
    range: Range,
    filter: &Filter,
) -> Result<Totals, HistoryError> {
    let mut totals = Totals::default();
    history.for_each_request(range.from_ms, range.to_ms, |request| {
        if filter.admits(request) {
            totals.add(request, price(table, request));
        }
    })?;
    Ok(totals)
}

/// Which requests a report covers, besides its range.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// Only this project's requests.
    pub project: Option<ProjectKey>,
    /// Only this model's requests, by model id.
    pub model: Option<String>,
}

impl Filter {
    fn admits(&self, request: &StoredRequest<'_>) -> bool {
        self.project
            .is_none_or(|project| project == request.project)
            && self
                .model
                .as_deref()
                .is_none_or(|model| model == request.model)
    }
}

/// One project's totals, with its API-equivalent cost on each day it has requests.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSummary {
    pub totals: ProjectTotals,
    pub daily_cost: BTreeMap<Date, f64>,
}

/// What a dashboard shows for a range, read in one pass over its requests.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Summary {
    pub totals: Totals,
    /// The days with requests, oldest first, each with its models by cost.
    pub days: Vec<DayTotals>,
    /// Models, highest API-equivalent cost first.
    pub models: Vec<ModelTotals>,
    /// Projects, highest API-equivalent cost first.
    pub projects: Vec<ProjectSummary>,
}

/// The requests in `range` that `filter` admits, added up in every way a dashboard shows them,
/// with days in `zone`.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be read.
pub fn summary(
    history: &History,
    table: &PriceTable,
    range: Range,
    filter: &Filter,
    zone: &TimeZone,
) -> Result<Summary, HistoryError> {
    let mut totals = Totals::default();
    let mut days: BTreeMap<Date, HashMap<String, Totals>> = BTreeMap::new();
    let mut models: HashMap<String, Totals> = HashMap::new();
    let mut projects: HashMap<ProjectKey, (ProjectSummary, HashSet<SessionKey>)> = HashMap::new();
    history.for_each_request(range.from_ms, range.to_ms, |request| {
        if !filter.admits(request) {
            return;
        }
        let Ok(at) = Timestamp::from_millisecond(request.timestamp_ms) else {
            return;
        };
        let day = at.to_zoned(zone.clone()).date();
        let cost = price(table, request);
        totals.add(request, cost);
        days.entry(day)
            .or_default()
            .entry(request.model.to_owned())
            .or_default()
            .add(request, cost);
        models
            .entry(request.model.to_owned())
            .or_default()
            .add(request, cost);
        let (project, sessions) = projects.entry(request.project).or_insert_with(|| {
            (
                ProjectSummary {
                    totals: ProjectTotals {
                        project: request.project,
                        sessions: 0,
                        last_ms: request.timestamp_ms,
                        totals: Totals::default(),
                    },
                    daily_cost: BTreeMap::new(),
                },
                HashSet::new(),
            )
        });
        project.totals.last_ms = project.totals.last_ms.max(request.timestamp_ms);
        project.totals.totals.add(request, cost);
        if let Some(cost) = cost {
            *project.daily_cost.entry(day).or_default() += cost.total();
        }
        sessions.insert(request.session);
    })?;
    let days = days
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
        .collect();
    let mut projects: Vec<ProjectSummary> = projects
        .into_values()
        .map(|(mut project, sessions)| {
            project.totals.sessions = sessions.len() as u64;
            project
        })
        .collect();
    projects.sort_by(|a, b| {
        b.totals
            .totals
            .cost
            .total()
            .total_cmp(&a.totals.totals.cost.total())
            .then_with(|| b.totals.last_ms.cmp(&a.totals.last_ms))
    });
    Ok(Summary {
        totals,
        days,
        models: by_cost(models),
        projects,
    })
}

/// A subagent's part of a session.
#[derive(Debug, Clone, PartialEq)]
pub struct SubagentTotals {
    pub subagent_id: String,
    /// When its first request was made.
    pub first_ms: i64,
    pub totals: Totals,
    /// Its models, highest API-equivalent cost first.
    pub models: Vec<ModelTotals>,
}

/// Requests credited to one skill, or to one MCP server and tool.
#[derive(Debug, Clone, PartialEq)]
pub struct Credit {
    /// The skill, or the MCP server.
    pub name: String,
    /// The MCP tool, for an MCP server.
    pub tool: Option<String>,
    pub totals: Totals,
}

/// Everything about one session.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionDetail {
    pub first_ms: i64,
    pub last_ms: i64,
    pub totals: Totals,
    /// The session's own requests, without its subagents'.
    pub own: Totals,
    /// The session's own models, highest API-equivalent cost first.
    pub own_models: Vec<ModelTotals>,
    /// Subagents in the order they started.
    pub subagents: Vec<SubagentTotals>,
    /// The API-equivalent cost so far after each priced request, as (time, cost).
    pub cumulative_cost: Vec<(i64, f64)>,
    pub skills: Vec<Credit>,
    pub mcp_tools: Vec<Credit>,
    /// Requests credited to neither a skill nor an MCP server.
    pub uncredited: Totals,
    /// The Claude Code version and entry point of its latest request.
    pub claude_code_version: Option<String>,
    pub entrypoint: Option<String>,
}

/// One session's requests, its subagents' included, added up; `None` when it has none.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be read.
pub fn session_detail(
    history: &History,
    table: &PriceTable,
    session: SessionKey,
) -> Result<Option<SessionDetail>, HistoryError> {
    let mut detail: Option<SessionDetail> = None;
    let mut own_models: HashMap<String, Totals> = HashMap::new();
    let mut subagents: HashMap<String, (i64, Totals, HashMap<String, Totals>)> = HashMap::new();
    let mut skills: HashMap<String, Totals> = HashMap::new();
    let mut mcp_tools: HashMap<(String, Option<String>), Totals> = HashMap::new();
    let mut running = 0.0;
    history.for_each_request_in_session(session, |request| {
        let cost = price(table, request);
        let detail = detail.get_or_insert_with(|| SessionDetail {
            first_ms: request.timestamp_ms,
            last_ms: request.timestamp_ms,
            totals: Totals::default(),
            own: Totals::default(),
            own_models: Vec::new(),
            subagents: Vec::new(),
            cumulative_cost: Vec::new(),
            skills: Vec::new(),
            mcp_tools: Vec::new(),
            uncredited: Totals::default(),
            claude_code_version: None,
            entrypoint: None,
        });
        detail.first_ms = detail.first_ms.min(request.timestamp_ms);
        detail.last_ms = detail.last_ms.max(request.timestamp_ms);
        detail.totals.add(request, cost);
        if let Some(cost) = cost {
            running += cost.total();
            detail.cumulative_cost.push((request.timestamp_ms, running));
        }
        if let Some(id) = request.subagent_id {
            let (first_ms, totals, models) = subagents
                .entry(id.to_owned())
                .or_insert_with(|| (request.timestamp_ms, Totals::default(), HashMap::new()));
            *first_ms = (*first_ms).min(request.timestamp_ms);
            totals.add(request, cost);
            models
                .entry(request.model.to_owned())
                .or_default()
                .add(request, cost);
        } else {
            detail.own.add(request, cost);
            own_models
                .entry(request.model.to_owned())
                .or_default()
                .add(request, cost);
        }
        match (request.skill, request.mcp_server) {
            (Some(skill), _) => skills
                .entry(skill.to_owned())
                .or_default()
                .add(request, cost),
            (None, Some(server)) => mcp_tools
                .entry((server.to_owned(), request.mcp_tool.map(str::to_owned)))
                .or_default()
                .add(request, cost),
            (None, None) => detail.uncredited.add(request, cost),
        }
        if let Some(version) = request.claude_code_version {
            detail.claude_code_version = Some(version.to_owned());
        }
        if let Some(entrypoint) = request.entrypoint {
            detail.entrypoint = Some(entrypoint.to_owned());
        }
    })?;
    Ok(detail.map(|mut detail| {
        detail.own_models = by_cost(own_models);
        let mut subagents: Vec<SubagentTotals> = subagents
            .into_iter()
            .map(|(subagent_id, (first_ms, totals, models))| SubagentTotals {
                subagent_id,
                first_ms,
                totals,
                models: by_cost(models),
            })
            .collect();
        subagents.sort_by(|a, b| {
            a.first_ms
                .cmp(&b.first_ms)
                .then_with(|| a.subagent_id.cmp(&b.subagent_id))
        });
        detail.subagents = subagents;
        detail.skills = credits(
            skills
                .into_iter()
                .map(|(name, totals)| (name, None, totals)),
        );
        detail.mcp_tools = credits(
            mcp_tools
                .into_iter()
                .map(|((name, tool), totals)| (name, tool, totals)),
        );
        detail
    }))
}

/// Credits, highest API-equivalent cost first.
fn credits(credits: impl Iterator<Item = (String, Option<String>, Totals)>) -> Vec<Credit> {
    let mut credits: Vec<Credit> = credits
        .map(|(name, tool, totals)| Credit { name, tool, totals })
        .collect();
    credits.sort_by(|a, b| {
        b.totals
            .cost
            .total()
            .total_cmp(&a.totals.cost.total())
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.tool.cmp(&b.tool))
    });
    credits
}
