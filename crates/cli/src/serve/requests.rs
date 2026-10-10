//! Answers to the client's requests, from the history and the core's queries.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use claude_dashboard_core::history::{self, History, HistoryError, ProjectKey};
use claude_dashboard_core::import;
use claude_dashboard_core::limits::{self, FIVE_HOUR, FIVE_HOURS_MS, LimitSnapshot, Source};
use claude_dashboard_core::pricing;
use claude_dashboard_core::reconcile;
use claude_dashboard_core::report::{self, Filter, Range};
use claude_dashboard_core::roots::{self, RootSource, RootStatus};
use claude_dashboard_core::tokens;
use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use lsp_server::{ErrorCode, Request, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::protocol::{
    Access, Choices, Cost, CostPoint, Credit, Day, DayCost, Days, Endpoint, EndpointParams,
    FiveHourWindow, Folder, FolderParams, FolderState, Health, HistoryFacts, ImportDone,
    ImportOffer, Imported, Level, LongPrompt, Model, ModelDay, ModelRow, Overview, OverviewParams,
    Pace, PriceDrift, PriceRow, Prices, PricesParams, Project, ProjectRow, Provenance,
    SessionDetail, SessionParams, SessionRow, SessionsPage, SessionsParams, Settings,
    SettingsChange, Status, StatusLine, StatusLineApply, StatusLineParams, StatusLinePlan, Tokens,
    Totals, UsePoint, WhoRow, WindowReading, WindowView,
};
use super::workers::Triggers;
use super::{Shared, lock};
use crate::app::{self, App};
use crate::breakdown::project_names;
use crate::report::model_name;
use crate::statusline::{self, Connection};

/// How long a weekly window lasts.
const WEEK_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// How far back a weekly window's pace is measured.
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// How far back the recent five-hour windows go.
const RECENT_WINDOWS_MS: i64 = 7 * DAY_MS;
/// The most sessions one page holds.
const LONGEST_PAGE: u32 = 500;

/// Why a request could not be answered.
struct Fault {
    code: i32,
    message: String,
}

impl Fault {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidParams as i32,
            message: message.into(),
        }
    }
}

impl From<HistoryError> for Fault {
    fn from(error: HistoryError) -> Self {
        Self {
            code: ErrorCode::InternalError as i32,
            message: error.to_string(),
        }
    }
}

pub struct Server {
    app: App,
    shared: Arc<Mutex<Shared>>,
    triggers: Triggers,
    history: Option<History>,
}

impl Server {
    pub fn new(app: App, shared: Arc<Mutex<Shared>>, triggers: Triggers) -> Self {
        Self {
            app,
            shared,
            triggers,
            history: None,
        }
    }

    pub fn answer(&mut self, request: Request) -> Response {
        let Request { id, method, params } = request;
        let app = self.shared_app().at(app::now());
        let answer = match method.as_str() {
            "status" => self.status(&app).and_then(json),
            "overview" => parse(params)
                .and_then(|p| self.overview(&app, &p))
                .and_then(json),
            "sessions" => parse(params)
                .and_then(|p| self.sessions(&app, &p))
                .and_then(json),
            "session" => parse(params)
                .and_then(|p| self.session(&app, &p))
                .and_then(json),
            "choices" => self.choices(&app).and_then(json),
            "limits" => self.limits(&app).and_then(json),
            "prices" => parse(params)
                .and_then(|p| self.prices(&app, &p))
                .and_then(json),
            "health" => json(self.health(&app)),
            "refresh" => {
                let _ = self.triggers.read.send(());
                Ok(Value::Null)
            }
            "endpoint/set" => parse(params)
                .and_then(|p| self.set_endpoint(&p))
                .and_then(json),
            "statusLine/plan" => parse(params)
                .map(|p| plan_status_line(&app, &p))
                .and_then(json),
            "statusLine/apply" => parse(params)
                .and_then(|p| apply_status_line(&app, &p))
                .and_then(json),
            "settings" => json(settings(&app)),
            "settings/set" => parse(params)
                .and_then(|p| self.set_settings(&p))
                .and_then(json),
            "folders/add" => parse(params)
                .and_then(|p| self.add_folder(&app, &p))
                .and_then(json),
            "folders/remove" => parse(params)
                .and_then(|p| self.remove_folder(&p))
                .and_then(json),
            "import/offer" => self.import_offer(&app).and_then(json),
            "import/run" => self.run_import(&app).and_then(json),
            "import/dismiss" => self.dismiss_import().and_then(json),
            other => Err(Fault {
                code: ErrorCode::MethodNotFound as i32,
                message: format!("claude-dashboard has no method {other}"),
            }),
        };
        match answer {
            Ok(value) => Response::new_ok(id, value),
            Err(fault) => Response::new_err(id, fault.code, fault.message),
        }
    }

    fn history(&mut self) -> Result<&History, Fault> {
        if self.history.is_none() {
            self.history = Some(self.app.open_history().map_err(|message| Fault {
                code: ErrorCode::InternalError as i32,
                message,
            })?);
        }
        Ok(self.history.as_ref().expect("opened above"))
    }

    /// The app with the settings every surface shares, as the history holds them now.
    fn shared_app(&mut self) -> App {
        if self.history.is_none() {
            self.history = self.app.open_history().ok();
        }
        match &self.history {
            Some(history) => self.app.with_shared(history),
            None => self.app.clone(),
        }
    }

    fn history_mut(&mut self) -> Result<&mut History, Fault> {
        self.history()?;
        Ok(self.history.as_mut().expect("opened above"))
    }

    fn status(&mut self, app: &App) -> Result<Status, Fault> {
        let history = self.history()?;
        let today = app.now_zoned().date();
        let week_start = app.start_of_week(today);
        let totals_of = |first: Date| {
            report::totals(
                history,
                app.prices,
                Range::days(first, today, &app.zone),
                &Filter::default(),
            )
        };
        let today_totals = totals(&totals_of(today)?);
        let week_totals = totals(&totals_of(week_start)?);
        let first_day = history
            .first_request_ms()?
            .and_then(|ms| Timestamp::from_millisecond(ms).ok())
            .map(|at| at.to_zoned(app.zone.clone()).date().to_string());
        let limits = history
            .latest_limits()?
            .iter()
            .map(|reading| window_reading(app, reading))
            .collect();
        let access = access(history);
        let shared = lock(&self.shared);
        Ok(Status {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            time_zone: app.zone.iana_name().unwrap_or("UTC").to_owned(),
            today: today.to_string(),
            first_day,
            read_ms: shared.read_ms,
            reading: shared.reading,
            access,
            limits,
            today_totals,
            week_totals,
            week_start: week_start_name(app).to_owned(),
        })
    }

    fn overview(&mut self, app: &App, params: &OverviewParams) -> Result<Overview, Fault> {
        let range = range(app, &params.days)?;
        let history = self.history()?;
        let names = project_names(history);
        let filter = filter(&names, &params.filters)?;
        let summary = report::summary(history, app.prices, range, &filter, &app.zone)?;
        Ok(Overview {
            days: params.days.clone(),
            totals: totals(&summary.totals),
            by_day: summary
                .days
                .iter()
                .map(|day| Day {
                    date: day.day.to_string(),
                    totals: totals(&day.totals),
                    models: day
                        .models
                        .iter()
                        .map(|model| ModelDay {
                            model: model.model.clone(),
                            totals: totals(&model.totals),
                        })
                        .collect(),
                })
                .collect(),
            models: summary
                .models
                .iter()
                .map(|model| ModelRow {
                    model: model_of(app, &model.model),
                    totals: totals(&model.totals),
                })
                .collect(),
            projects: summary
                .projects
                .iter()
                .map(|project| ProjectRow {
                    project: project_of(&names, project.totals.project),
                    sessions: project.totals.sessions,
                    last_ms: project.totals.last_ms,
                    totals: totals(&project.totals.totals),
                    daily_cost: project
                        .daily_cost
                        .iter()
                        .map(|(day, usd)| DayCost {
                            date: day.to_string(),
                            usd: *usd,
                        })
                        .collect(),
                })
                .collect(),
        })
    }

    fn sessions(&mut self, app: &App, params: &SessionsParams) -> Result<SessionsPage, Fault> {
        let range = range(app, &params.days)?;
        let history = self.history()?;
        let names = project_names(history);
        let filter = filter(&names, &params.filters)?;
        let stored: HashMap<_, _> = history
            .sessions()?
            .into_iter()
            .map(|session| (session.key, (session.session_id, session.title)))
            .collect();
        let reported = reported_costs(history)?;
        let search = params
            .search
            .as_deref()
            .map(str::trim)
            .filter(|search| !search.is_empty())
            .map(str::to_lowercase);
        let mut matching: Vec<SessionRow> =
            report::sessions_matching(history, app.prices, range, &filter)?
                .into_iter()
                .filter_map(|session| {
                    let (id, title) = stored.get(&session.session)?.clone();
                    let project = project_of(&names, session.project);
                    let found = search.as_deref().is_none_or(|search| {
                        [
                            Some(id.as_str()),
                            title.as_deref(),
                            Some(project.name.as_str()),
                        ]
                        .into_iter()
                        .flatten()
                        .any(|text| text.to_lowercase().contains(search))
                    });
                    found.then(|| SessionRow {
                        id,
                        title,
                        project,
                        first_ms: session.first_ms,
                        last_ms: session.last_ms,
                        models: session
                            .models
                            .iter()
                            .map(|model| model_of(app, &model.model))
                            .collect(),
                        subagents: session.subagent_count,
                        totals: totals(&session.totals),
                        reported_usd: reported.get(&session.session).copied(),
                    })
                })
                .collect();
        matching.sort_by(|a, b| b.first_ms.cmp(&a.first_ms).then_with(|| a.id.cmp(&b.id)));
        let limit = params.limit.clamp(1, LONGEST_PAGE) as usize;
        Ok(SessionsPage {
            total: matching.len() as u64,
            rows: matching
                .into_iter()
                .skip(params.offset as usize)
                .take(limit)
                .collect(),
        })
    }

    fn session(&mut self, app: &App, params: &SessionParams) -> Result<SessionDetail, Fault> {
        let history = self.history()?;
        let stored = history
            .sessions()?
            .into_iter()
            .find(|session| session.session_id == params.id)
            .ok_or_else(|| Fault::invalid(format!("The history holds no session {}", params.id)))?;
        let detail = report::session_detail(history, app.prices, stored.key)?
            .ok_or_else(|| Fault::invalid(format!("Session {} has no requests", params.id)))?;
        let names = project_names(history);
        let reported = reported_costs(history)?;
        let agent_types = history.subagent_types(stored.key)?;
        let mut who: Vec<WhoRow> = detail
            .own_models
            .iter()
            .map(|model| WhoRow {
                subagent: None,
                agent_type: None,
                model: model_of(app, &model.model),
                first_ms: detail.first_ms,
                last_ms: detail.last_ms,
                totals: totals(&model.totals),
            })
            .collect();
        for subagent in &detail.subagents {
            who.extend(subagent.models.iter().map(|model| WhoRow {
                subagent: Some(subagent.subagent_id.clone()),
                agent_type: agent_types.get(&subagent.subagent_id).cloned(),
                model: model_of(app, &model.model),
                first_ms: subagent.first_ms,
                last_ms: subagent.last_ms,
                totals: totals(&model.totals),
            }));
        }
        let project = project_of(&names, stored.project);
        let transcript = app
            .roots
            .iter()
            .map(|root| {
                root.path
                    .join("projects")
                    .join(&project.id)
                    .join(format!("{}.jsonl", stored.session_id))
            })
            .find(|path| path.is_file())
            .map(|path| path.display().to_string());
        let credits = |credits: &[report::Credit]| -> Vec<Credit> {
            credits
                .iter()
                .map(|credit| Credit {
                    name: credit.name.clone(),
                    tool: credit.tool.clone(),
                    totals: totals(&credit.totals),
                })
                .collect()
        };
        Ok(SessionDetail {
            id: stored.session_id,
            title: stored.title,
            project,
            transcript,
            first_ms: detail.first_ms,
            last_ms: detail.last_ms,
            claude_code_version: detail.claude_code_version.clone(),
            entrypoint: detail.entrypoint.clone(),
            totals: totals(&detail.totals),
            own: totals(&detail.own),
            reported_usd: reported.get(&stored.key).copied(),
            cumulative_cost: detail
                .cumulative_cost
                .iter()
                .map(|&(ms, usd)| CostPoint { ms, usd })
                .collect(),
            who,
            skills: credits(&detail.skills),
            mcp_tools: credits(&detail.mcp_tools),
            uncredited: totals(&detail.uncredited),
        })
    }

    fn limits(&mut self, app: &App) -> Result<super::protocol::Limits, Fault> {
        let now_ms = app.now.as_millisecond();
        let history = self.history()?;
        let mut windows = Vec::new();
        for reading in history.latest_limits()? {
            let length = if reading.window == FIVE_HOUR {
                FIVE_HOURS_MS
            } else {
                WEEK_MS
            };
            let start_ms = reading.resets_at_ms.map(|resets| resets - length);
            let series: Vec<UsePoint> = match (start_ms, reading.resets_at_ms) {
                (Some(start_ms), Some(resets_at_ms)) => history
                    .limit_snapshots(&reading.window, start_ms, now_ms.saturating_add(1))?
                    .into_iter()
                    .filter(|point| {
                        point.resets_at_ms.is_some_and(|resets| {
                            resets.div_euclid(60_000) == resets_at_ms.div_euclid(60_000)
                        })
                    })
                    .map(|point| UsePoint {
                        ms: point.observed_ms,
                        used: point.used_percentage,
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let pace = match (start_ms, reading.resets_at_ms) {
                (Some(start_ms), Some(resets_at_ms)) if !reading.expired(now_ms) => {
                    let since = if reading.window == FIVE_HOUR {
                        (start_ms, 0.0)
                    } else {
                        // The reading nearest to a day before, or the window's start.
                        let target = reading.observed_ms - DAY_MS;
                        series
                            .iter()
                            .rev()
                            .find(|point| point.ms <= target)
                            .map_or((start_ms.max(target), 0.0), |point| (point.ms, point.used))
                    };
                    limits::pace(
                        since,
                        (reading.observed_ms, reading.used_percentage),
                        resets_at_ms,
                    )
                    .map(|pace| Pace {
                        at_reset: pace.at_reset,
                        limit_reached_ms: pace.limit_reached_ms,
                        since_ms: since.0,
                    })
                }
                _ => None,
            };
            windows.push(WindowView {
                reading: window_reading(app, &reading),
                start_ms,
                series,
                pace,
            });
        }
        let recent = limits::recent_five_hour_windows(
            history,
            app.prices,
            now_ms - RECENT_WINDOWS_MS,
            now_ms,
        )?
        .into_iter()
        .map(|(window, window_totals)| FiveHourWindow {
            start_ms: window.start_ms,
            resets_at_ms: window.resets_at_ms,
            highest: window.highest,
            totals: totals(&window_totals),
        })
        .collect();
        let state = history.usage_endpoint()?;
        Ok(super::protocol::Limits {
            windows,
            recent,
            status_line: match statusline::connection(app) {
                Connection::Connected { wrapping } => StatusLine::Connected { wrapping },
                Connection::NotConnected { .. } => StatusLine::NotConnected,
                Connection::Unknown(reason) => StatusLine::Unknown { reason },
            },
            endpoint: Endpoint {
                enabled: state.enabled,
                asked_ms: state.asked_ms,
                outcome: state.outcome.map(|outcome| outcome.as_str().to_owned()),
                problem: crate::limits::problem(app, &state),
                detail: state.detail,
                next_ask_ms: state.next_ask_ms,
                consent: crate::limits::CONSENT.map(str::to_owned).to_vec(),
            },
        })
    }

    /// Turns the usage endpoint on or off; once on, it is asked as soon as its rules allow.
    fn set_endpoint(&mut self, params: &EndpointParams) -> Result<Value, Fault> {
        let history = self.history_mut()?;
        history
            .write(|w| w.set_usage_endpoint(params.enabled))
            .map_err(Fault::from)?;
        if params.enabled {
            let _ = self.triggers.endpoint.send(());
        }
        Ok(Value::Null)
    }

    fn set_settings(&mut self, change: &SettingsChange) -> Result<Value, Fault> {
        let zone = match change.time_zone.as_deref() {
            None => None,
            Some("") => Some(None),
            Some(name) => {
                let zone = TimeZone::get(name)
                    .map_err(|_| Fault::invalid(format!("{name} is not a time zone")))?;
                Some(Some(zone.iana_name().unwrap_or(name).to_owned()))
            }
        };
        let week_start = match change.week_start.as_deref() {
            None => None,
            Some(name) => Some(
                app::weekday(name)
                    .ok_or_else(|| Fault::invalid(format!("{name} is not a day of the week")))?,
            ),
        };
        self.history_mut()?
            .write(|w| {
                if let Some(zone) = &zone {
                    w.set_setting(app::TIME_ZONE_SETTING, zone.as_deref())?;
                }
                if let Some(day) = week_start {
                    let name = app::WEEKDAYS
                        .iter()
                        .find(|(_, weekday)| *weekday == day)
                        .map(|(name, _)| *name);
                    w.set_setting(app::WEEK_START_SETTING, name)?;
                }
                Ok(())
            })
            .map_err(Fault::from)?;
        Ok(Value::Null)
    }

    fn add_folder(&mut self, app: &App, params: &FolderParams) -> Result<Value, Fault> {
        let path = std::path::Path::new(params.path.trim());
        if !path.is_absolute() {
            return Err(Fault::invalid(format!(
                "{} is not a full path to a folder",
                path.display()
            )));
        }
        let now_ms = app.now.as_millisecond();
        let path = path.display().to_string();
        self.history_mut()?
            .write(|w| w.add_data_folder(&path, now_ms))
            .map_err(Fault::from)?;
        let _ = self.triggers.read.send(());
        Ok(Value::Null)
    }

    fn remove_folder(&mut self, params: &FolderParams) -> Result<Value, Fault> {
        self.history_mut()?
            .write(|w| w.remove_data_folder(&params.path))
            .map_err(Fault::from)?;
        Ok(Value::Null)
    }

    /// What the claude-usage dashboard's database in a data folder would add, if there is one.
    fn import_offer(&mut self, app: &App) -> Result<Option<ImportOffer>, Fault> {
        let Some(database) = app
            .roots
            .iter()
            .find_map(|root| import::claude_usage_database(&root.path))
        else {
            return Ok(None);
        };
        let history = self.history()?;
        let preview = import::preview_claude_usage(history, &database).map_err(|error| Fault {
            code: ErrorCode::InternalError as i32,
            message: error.to_string(),
        })?;
        let day = |ms: i64| {
            Timestamp::from_millisecond(ms)
                .ok()
                .map(|at| at.to_zoned(app.zone.clone()).date().to_string())
        };
        let dismissed = history
            .settings()?
            .get(IMPORT_DISMISSED_SETTING)
            .is_some_and(|source| source == import::CLAUDE_USAGE);
        Ok(Some(ImportOffer {
            path: database.display().to_string(),
            requests: preview.requests,
            first_day: preview.span_ms.and_then(|(first, _)| day(first)),
            last_day: preview.span_ms.and_then(|(_, last)| day(last)),
            dismissed,
        }))
    }

    fn run_import(&mut self, app: &App) -> Result<ImportDone, Fault> {
        let database = app
            .roots
            .iter()
            .find_map(|root| import::claude_usage_database(&root.path))
            .ok_or_else(|| Fault::invalid("There is no claude-usage database to import."))?;
        let now_ms = app.now.as_millisecond();
        let report = import::import_claude_usage(self.history_mut()?, &database, now_ms).map_err(
            |error| Fault {
                code: ErrorCode::InternalError as i32,
                message: error.to_string(),
            },
        )?;
        Ok(ImportDone {
            imported: report.imported,
            already_held: report.already_held,
            unreadable: report.unreadable,
        })
    }

    fn dismiss_import(&mut self) -> Result<Value, Fault> {
        self.history_mut()?
            .write(|w| w.set_setting(IMPORT_DISMISSED_SETTING, Some(import::CLAUDE_USAGE)))
            .map_err(Fault::from)?;
        Ok(Value::Null)
    }

    fn choices(&mut self, app: &App) -> Result<Choices, Fault> {
        let history = self.history()?;
        let names = project_names(history);
        let mut projects: Vec<Project> = names.keys().map(|&key| project_of(&names, key)).collect();
        projects.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        let mut models: Vec<Model> = history
            .models()?
            .iter()
            .map(|model| model_of(app, model))
            .collect();
        models.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        Ok(Choices { projects, models })
    }

    fn prices(&mut self, app: &App, params: &PricesParams) -> Result<Prices, Fault> {
        let history = self.history()?;
        let reconciliation = reconcile::reconcile(history, app.prices, app.now.as_millisecond())?;
        let mut seen = HashSet::new();
        let models = params
            .models
            .iter()
            .filter(|model| seen.insert(model.as_str()))
            .filter_map(|model| {
                let rates = app.prices.rates(&pricing::Request {
                    model,
                    timestamp_ms: app.now.as_millisecond(),
                    tokens: tokens::Tokens::default(),
                    speed: None,
                    inference_geo: None,
                    web_search_requests: 0,
                })?;
                let long_prompt =
                    app.prices
                        .long_prompt(model, app.now.as_millisecond())
                        .map(|(above, long)| LongPrompt {
                            above,
                            input: long.input,
                            output: long.output,
                            cache_write_5m: long.cache_write_5m,
                            cache_write_1h: long.cache_write_1h,
                            cache_read: long.cache_read,
                        });
                Some(PriceRow {
                    model: model_of(app, model),
                    input: rates.input,
                    output: rates.output,
                    cache_write_5m: rates.cache_write_5m,
                    cache_write_1h: rates.cache_write_1h,
                    cache_read: rates.cache_read,
                    long_prompt,
                })
            })
            .collect();
        let out_of_line = reconciliation
            .out_of_line()
            .filter_map(|model| {
                Some(PriceDrift {
                    model: model_of(app, &model.model),
                    drift: model.drift()?,
                    sessions: model.sessions,
                })
            })
            .collect();
        Ok(Prices {
            checked: app.prices.checked().to_string(),
            source: app.prices.source().to_owned(),
            models,
            drift: reconciliation.drift(),
            sessions_compared: reconciliation.sessions,
            out_of_line,
        })
    }

    fn health(&mut self, app: &App) -> Health {
        let folders = app
            .roots
            .iter()
            .map(|root| Folder {
                path: root.path.display().to_string(),
                source: match root.source {
                    RootSource::Default => "default",
                    RootSource::Environment => "environment",
                    RootSource::Added => "added",
                }
                .to_owned(),
                state: match roots::inspect(&root.path) {
                    RootStatus::Found { transcripts, bytes } => {
                        FolderState::Found { transcripts, bytes }
                    }
                    RootStatus::Missing => FolderState::Missing,
                    RootStatus::Unreachable { reason } => FolderState::Unreachable { reason },
                    RootStatus::Unreadable { reason } => FolderState::Unreadable { reason },
                },
            })
            .collect();
        let history_path = app
            .history_path
            .as_ref()
            .map(|path| path.display().to_string());
        let (access, newer_schema) = match self.history() {
            Ok(history) => match history.access() {
                history::Access::ReadWrite => (Access::ReadWrite, None),
                history::Access::ReadOnly { schema } => (Access::ReadOnly, Some(schema)),
            },
            Err(_) => (Access::ReadOnly, None),
        };
        let mut problems = lock(&self.shared).problems.clone();
        if let Err(fault) = self.history() {
            problems.push(fault.message);
        }
        let facts = self.history().ok().and_then(|history| {
            let counts = history.counts().ok()?;
            let bytes = std::fs::metadata(history.path()).map_or(0, |meta| meta.len());
            let imported = history
                .import(import::CLAUDE_USAGE)
                .ok()
                .flatten()
                .map(|import| Imported {
                    requests: import.requests,
                    imported_ms: import.imported_ms,
                });
            Some(HistoryFacts {
                bytes,
                requests: counts.requests,
                sessions: counts.sessions,
                imported,
            })
        });
        Health {
            folders,
            history_path,
            history: facts,
            cleanup_days: statusline::cleanup_period_days(app),
            access,
            newer_schema,
            problems,
        }
    }
}

fn parse<T: DeserializeOwned>(params: Value) -> Result<T, Fault> {
    serde_json::from_value(params).map_err(|error| Fault::invalid(error.to_string()))
}

fn json(value: impl Serialize) -> Result<Value, Fault> {
    serde_json::to_value(value).map_err(|error| Fault {
        code: ErrorCode::InternalError as i32,
        message: error.to_string(),
    })
}

fn range(app: &App, days: &Days) -> Result<Range, Fault> {
    let day = |text: &str| {
        text.parse::<Date>()
            .map_err(|_| Fault::invalid(format!("{text} is not a day such as 2026-10-11")))
    };
    let (first, last) = (day(&days.first)?, day(&days.last)?);
    if first > last {
        return Err(Fault::invalid("The first day comes after the last"));
    }
    Ok(Range::days(first, last, &app.zone))
}

type Names = HashMap<ProjectKey, (String, Option<String>, String)>;

fn filter(names: &Names, filters: &super::protocol::Filters) -> Result<Filter, Fault> {
    let project = match &filters.project {
        None => None,
        Some(id) => Some(
            names
                .iter()
                .find(|(_, (_, _, folder))| folder == id)
                .map(|(key, _)| *key)
                .ok_or_else(|| Fault::invalid(format!("The history holds no project {id}")))?,
        ),
    };
    Ok(Filter {
        project,
        model: filters.model.clone(),
    })
}

/// The shared setting that records an import the person chose not to make.
const IMPORT_DISMISSED_SETTING: &str = "import_dismissed";

fn settings(app: &App) -> Settings {
    let chosen = app.zone.iana_name().unwrap_or("UTC");
    let system = app.system_zone.iana_name().unwrap_or("UTC");
    Settings {
        time_zone: (app.zone != app.system_zone).then(|| chosen.to_owned()),
        system_time_zone: system.to_owned(),
        week_start: week_start_name(app).to_owned(),
    }
}

fn week_start_name(app: &App) -> &'static str {
    app::WEEKDAYS
        .iter()
        .find(|(_, day)| *day == app.week_start)
        .map_or("monday", |(name, _)| *name)
}

fn status_line_plan(app: &App, connect: bool) -> statusline::Plan {
    if connect {
        statusline::plan_connect(app)
    } else {
        statusline::plan_disconnect(app)
    }
}

fn plan_status_line(app: &App, params: &StatusLineParams) -> StatusLinePlan {
    match status_line_plan(app, params.connect) {
        statusline::Plan::Change(change) => StatusLinePlan::Change {
            path: change.path.display().to_string(),
            before: change.old_members(),
            after: change.new_members(),
            exists: change.backed_up(),
            effect: change.effect.to_owned(),
        },
        statusline::Plan::Nothing(reason) => StatusLinePlan::Nothing { reason },
        statusline::Plan::Refused(reason) => StatusLinePlan::Refused { reason },
    }
}

/// Makes the change the person saw, as long as Claude Code's settings still call for it.
fn apply_status_line(app: &App, params: &StatusLineApply) -> Result<Value, Fault> {
    let statusline::Plan::Change(change) = status_line_plan(app, params.connect) else {
        return Err(Fault::invalid(
            "Claude Code's settings changed since the change was shown; look at it again."
                .to_owned(),
        ));
    };
    if change.old_members() != params.before || change.new_members() != params.after {
        return Err(Fault::invalid(
            "Claude Code's settings changed since the change was shown; look at it again."
                .to_owned(),
        ));
    }
    change.make().map_err(|reason| Fault {
        code: ErrorCode::InternalError as i32,
        message: format!("Cannot save the change: {reason}."),
    })?;
    Ok(Value::Null)
}

fn project_of(names: &Names, key: ProjectKey) -> Project {
    names.get(&key).map_or_else(
        || Project {
            id: String::new(),
            name: String::new(),
            path: None,
        },
        |(name, path, folder)| Project {
            id: folder.clone(),
            name: name.clone(),
            path: path.clone(),
        },
    )
}

fn model_of(app: &App, id: &str) -> Model {
    Model {
        id: id.to_owned(),
        name: model_name(app, id),
    }
}

fn totals(totals: &report::Totals) -> Totals {
    let tokens::Tokens {
        input,
        output,
        cache_read,
        cache_write_5m,
        cache_write_1h,
    } = totals.tokens;
    let pricing::Cost {
        input: input_cost,
        output: output_cost,
        cache_read: cache_read_cost,
        cache_write_5m: five_minute_writes,
        cache_write_1h: one_hour_writes,
        web_search,
    } = totals.cost;
    Totals {
        requests: totals.requests,
        tokens: Tokens {
            input,
            output,
            cache_read,
            cache_write_5m,
            cache_write_1h,
        },
        cost: Cost {
            total: totals.cost.total(),
            input: input_cost,
            output: output_cost,
            cache_read: cache_read_cost,
            cache_write_5m: five_minute_writes,
            cache_write_1h: one_hour_writes,
            web_search,
        },
        unpriced_requests: totals.unpriced_requests,
        web_search_requests: totals.web_search_requests,
        fast_requests: totals.fast_requests,
    }
}

fn window_reading(app: &App, reading: &LimitSnapshot) -> WindowReading {
    let now_ms = app.now.as_millisecond();
    WindowReading {
        window: reading.window.clone(),
        model: limits::weekly_model(&reading.window).map(str::to_owned),
        used_percentage: reading.used_percentage,
        resets_at_ms: reading.resets_at_ms,
        observed_ms: reading.observed_ms,
        provenance: match reading.source {
            Source::StatusLine => Provenance::Official,
            Source::UsageEndpoint => Provenance::Experimental,
        },
        level: match reading.level() {
            limits::Level::Normal => Level::Normal,
            limits::Level::Warning => Level::Warning,
            limits::Level::Critical => Level::Critical,
        },
        stale: reading.stale(now_ms),
        expired: reading.expired(now_ms),
    }
}

fn access(history: &History) -> Access {
    match history.access() {
        history::Access::ReadWrite => Access::ReadWrite,
        history::Access::ReadOnly { .. } => Access::ReadOnly,
    }
}

fn reported_costs(history: &History) -> Result<HashMap<history::SessionKey, f64>, Fault> {
    let mut reported: HashMap<history::SessionKey, f64> = HashMap::new();
    for cost in history.reported_costs()? {
        *reported.entry(cost.session).or_default() += cost.cost_usd;
    }
    Ok(reported)
}
