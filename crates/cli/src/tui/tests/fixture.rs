//! Made-up answers for the interface's tests: thirty days of four models' use, projects, sessions,
//! a session's detail and plan limits, shaped like the design's terminal boards, and the states the
//! boards draw.

use claude_dashboard_server::protocol::{
    Access, Choices, Cost, CostPoint, Credit, Day, DayCost, Days, Endpoint, FastPrices,
    FiveHourWindow, Folder, FolderState, Health, HistoryFacts, ImportOffer, Level, Limits,
    LongPrompt, Model, ModelDay, ModelRow, Overview, Pace, PriceDrift, PriceRow, Prices, Progress,
    Project, ProjectRow, Provenance, SessionDetail, SessionRow, SessionsPage, Status, StatusLine,
    Tokens, Totals, UsePoint, WhoRow, WindowReading, WindowView,
};
use jiff::civil::{Date, date};
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan};
use serde_json::{Value, json};

/// The states of the boards, besides the normal one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Normal,
    Reading,
    Empty,
    Import,
    LimitsOff,
    Stale,
    Endpoint,
    Unpriced,
    ManyModels,
    Drift,
    Folder,
    ReadOnly,
    TooNew,
    Error,
}

pub const ZONE: &str = "Pacific/Auckland";

/// The four models, with their share of the requests per dollar.
const MODELS: [(&str, &str, f64); 4] = [
    ("claude-fable-5-1", "Fable 5.1", 9.0),
    ("claude-opus-5-5", "Opus 5.5", 14.0),
    ("claude-sonnet-5-5", "Sonnet 5.5", 24.0),
    ("claude-haiku-5-5", "Haiku 5.5", 60.0),
];

const UNPRICED: &str = "glm-4.6";

/// Older models, besides the four, with their cost in the thirty days.
const OLDER: [(&str, &str, f64); 4] = [
    ("claude-opus-5", "Opus 5", 31.2),
    ("claude-sonnet-5", "Sonnet 5", 12.4),
    ("claude-opus-4-5", "Opus 4.5", 3.85),
    ("claude-haiku-4-5", "Haiku 4.5", 0.42),
];

/// The projects and their share of the cost; those under 2% end up together.
const PROJECTS: [(&str, &str, f64, u64); 11] = [
    ("C--code-claude-dashboard", "claude-dashboard", 0.38, 96),
    ("C--code-api-gateway", "api-gateway", 0.22, 41),
    ("C--code-website", "website", 0.15, 28),
    ("C--code-infra", "infra", 0.12, 22),
    ("C--code-notes", "notes", 0.05, 9),
    ("C--code-scratch", "scratch", 0.018, 4),
    ("C--code-dotfiles", "dotfiles", 0.017, 4),
    ("C--code-homelab", "homelab", 0.015, 3),
    ("C--code-recipes", "recipes", 0.012, 3),
    ("C--code-cv", "cv", 0.01, 2),
    ("C--code-misc", "misc", 0.008, 2),
];

const TITLES: [&str; 30] = [
    "Refactor ingest batching",
    "Fix flaky serve tests",
    "Desktop updater signing",
    "Add weekly and monthly reports",
    "Review pull request 47",
    "Price table for Opus 5.5",
    "Status line colours on light themes",
    "Investigate the WebView2 driver",
    "Session detail layout",
    "Explain the dist workflow",
    "Sparkline widths at 80 columns",
    "Limits pace estimate",
    "Import the claude-usage history",
    "",
    "Windows key events",
    "Projects trend column",
    "Cache efficiency note",
    "Benchmark the first read",
    "Doctor output for WSL",
    "D-Bus session in CI",
    "Light terminal palette",
    "Model picker",
    "Session search",
    "Rate limit back-off",
    "Time zone setting",
    "Release notes for 0.2.0",
    "Sidebar width",
    "Ingest lock on Windows",
    "Snapshot test harness",
    "Retry policy for the endpoint",
];

/// How many sessions the history holds in the thirty days.
const SESSIONS: usize = 214;

pub struct Fixture {
    pub variant: Variant,
    pub now: Timestamp,
    zone: TimeZone,
    /// Each day's cost per model, oldest first.
    days: Vec<(Date, [f64; 4])>,
}

/// The session's tokens and cost by type, as the board shows them.
fn session_totals() -> Totals {
    Totals {
        requests: 249,
        tokens: Tokens {
            input: 18_000,
            output: 96_000,
            cache_read: 1_400_000,
            cache_write_5m: 210_000,
            cache_write_1h: 40_000,
        },
        cost: Cost {
            total: 14.82,
            input: 0.27,
            output: 7.2,
            cache_read: 2.18,
            cache_write_5m: 3.94,
            cache_write_1h: 1.2,
            web_search: 0.03,
        },
        unpriced_requests: 0,
        web_search_requests: 3,
        fast_requests: 0,
    }
}

/// Requests credited to a skill, or to an MCP server and tool.
fn credit(name: &str, tool: Option<&str>, requests: u64, cost: f64) -> Credit {
    Credit {
        name: name.to_owned(),
        tool: tool.map(str::to_owned),
        totals: Totals {
            requests,
            ..totals(cost, 20.0, 0.0)
        },
    }
}

/// The boards' generator of made-up numbers.
struct Random(i64);

impl Random {
    fn next(&mut self) -> f64 {
        self.0 = self.0 * 16_807 % 2_147_483_647;
        #[allow(clippy::cast_precision_loss)]
        let value = (self.0 - 1) as f64 / 2_147_483_646.0;
        value
    }
}

fn ms(at: &str) -> i64 {
    at.parse::<jiff::Zoned>()
        .map(|at| at.timestamp().as_millisecond())
        .unwrap_or_default()
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn whole(value: f64) -> u64 {
    value.round().max(0.0) as u64
}

/// What `cost` dollars of a model's use look like.
fn totals(cost: f64, requests_per_dollar: f64, fast_share: f64) -> Totals {
    let requests = whole(cost * requests_per_dollar);
    Totals {
        requests,
        tokens: Tokens {
            input: whole(cost * 1_200.0),
            output: whole(cost * 21_000.0),
            cache_read: whole(cost * 290_000.0),
            cache_write_5m: whole(cost * 52_000.0),
            cache_write_1h: whole(cost * 9_000.0),
        },
        cost: Cost {
            total: cost,
            input: cost * 0.02,
            output: cost * 0.5,
            cache_read: cost * 0.15,
            cache_write_5m: cost * 0.25,
            cache_write_1h: cost * 0.08,
            web_search: 0.0,
        },
        unpriced_requests: 0,
        web_search_requests: 0,
        fast_requests: whole(f64::from(u32::try_from(requests).unwrap_or(0)) * fast_share),
    }
}

fn add(sum: &mut Totals, more: &Totals) {
    sum.requests += more.requests;
    sum.tokens.input += more.tokens.input;
    sum.tokens.output += more.tokens.output;
    sum.tokens.cache_read += more.tokens.cache_read;
    sum.tokens.cache_write_5m += more.tokens.cache_write_5m;
    sum.tokens.cache_write_1h += more.tokens.cache_write_1h;
    sum.cost.total += more.cost.total;
    sum.cost.input += more.cost.input;
    sum.cost.output += more.cost.output;
    sum.cost.cache_read += more.cost.cache_read;
    sum.cost.cache_write_5m += more.cost.cache_write_5m;
    sum.cost.cache_write_1h += more.cost.cache_write_1h;
    sum.unpriced_requests += more.unpriced_requests;
    sum.fast_requests += more.fast_requests;
}

fn model(index: usize) -> Model {
    Model {
        id: MODELS[index].0.to_owned(),
        name: MODELS[index].1.to_owned(),
    }
}

fn project(index: usize) -> Project {
    let (id, name, _, _) = PROJECTS[index];
    Project {
        id: id.to_owned(),
        name: name.to_owned(),
        path: Some(format!(r"C:\code\{name}")),
    }
}

fn json_of(value: impl serde::Serialize) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

impl Fixture {
    pub fn new(variant: Variant) -> Self {
        let mut random = Random(11);
        let first = date(2026, 9, 11);
        let mut days = Vec::new();
        for offset in 0..30 {
            let day = first.checked_add(offset.days()).unwrap_or(first);
            let weekend = matches!(
                day.weekday(),
                jiff::civil::Weekday::Saturday | jiff::civil::Weekday::Sunday
            );
            let scale = if weekend { 0.4 } else { 1.0 };
            let mut costs = [
                scale * (1.4 + random.next() * 4.2),
                scale * (2.8 + random.next() * 6.4),
                scale * (0.7 + random.next() * 2.4),
                scale * (0.1 + random.next() * 0.45),
            ];
            if variant == Variant::Reading && offset < 23 {
                costs = [0.0; 4];
            }
            days.push((day, costs));
        }
        Self {
            variant,
            now: Timestamp::from_millisecond(ms("2026-10-10T14:55:00+13:00[Pacific/Auckland]"))
                .unwrap_or(Timestamp::UNIX_EPOCH),
            zone: TimeZone::get(ZONE).unwrap_or(TimeZone::UTC),
            days,
        }
    }

    fn now_ms(&self) -> i64 {
        self.now.as_millisecond()
    }

    fn at(&self, day: Date, hour: i8, minute: i8) -> i64 {
        day.at(hour, minute, 0, 0)
            .to_zoned(self.zone.clone())
            .map_or(0, |at| at.timestamp().as_millisecond())
    }

    /// The answer to a request, as the server would give it.
    pub fn answer(&self, method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "status" => self.status(),
            "health" => json_of(self.health()),
            "choices" => json_of(self.choices()),
            "overview" => json_of(self.overview(params)),
            "limits" => json_of(self.limits()),
            "prices" => json_of(self.prices(params)),
            "import/offer" => json_of(self.import_offer()),
            "sessions" => json_of(self.sessions(params)),
            "session" => json_of(self.session()),
            other => Err(format!("the fixture has no answer to {other}")),
        }
    }

    fn status(&self) -> Result<Value, String> {
        match self.variant {
            Variant::TooNew => {
                return Err("a newer claude-dashboard has upgraded the history (schema 5); update claude-dashboard to read it".to_owned());
            }
            Variant::Error => {
                return Err(r#"SQLite cannot keep a write-ahead log for \\nas\you\history.sqlite (journal mode "delete"); the history must be on a local disk"#.to_owned());
            }
            _ => {}
        }
        let empty = self.variant == Variant::Empty;
        let reading = self.variant == Variant::Reading;
        let today = self.days.last().map_or(date(2026, 10, 10), |(day, _)| *day);
        let mut today_totals = Totals::default();
        let mut week_totals = Totals::default();
        for (day, costs) in &self.days {
            for (index, cost) in costs.iter().enumerate() {
                let part = totals(*cost, MODELS[index].2, 0.0);
                if *day == today {
                    add(&mut today_totals, &part);
                }
                if *day >= date(2026, 10, 5) {
                    add(&mut week_totals, &part);
                }
            }
        }
        json_of(Status {
            version: "0.3.0".to_owned(),
            time_zone: ZONE.to_owned(),
            today: today.to_string(),
            first_day: (!empty).then(|| "2026-09-11".to_owned()),
            read_ms: (!reading).then(|| self.now_ms() - 20_000),
            reading: reading.then_some(Progress {
                done: 412,
                total: 2_748,
            }),
            access: if self.variant == Variant::ReadOnly {
                Access::ReadOnly
            } else {
                Access::ReadWrite
            },
            limits: if empty || self.variant == Variant::LimitsOff {
                Vec::new()
            } else {
                self.windows()
                    .into_iter()
                    .map(|view| view.reading)
                    .collect()
            },
            today_totals,
            week_totals,
            week_start: "monday".to_owned(),
        })
    }

    fn health(&self) -> Health {
        let mut folders = vec![Folder {
            path: r"C:\Users\you\.claude".to_owned(),
            source: "default".to_owned(),
            state: if self.variant == Variant::Empty {
                FolderState::Missing
            } else {
                FolderState::Found {
                    transcripts: 2_748,
                    bytes: 2_900_000_000,
                }
            },
        }];
        if self.variant == Variant::Folder {
            folders.push(Folder {
                path: r"\\wsl.localhost\Ubuntu\home\you\.claude".to_owned(),
                source: "added".to_owned(),
                state: FolderState::Unreachable {
                    reason: "WSL is not running".to_owned(),
                },
            });
        }
        let unreadable = matches!(self.variant, Variant::TooNew | Variant::Error);
        Health {
            folders,
            history_path: Some(
                r"C:\Users\you\AppData\Local\claude-dashboard\history.sqlite".to_owned(),
            ),
            history: (!unreadable).then_some(HistoryFacts {
                bytes: 310_000_000,
                requests: 147_079,
                sessions: 2_412,
                imported: None,
            }),
            cleanup_days: 30,
            access: if matches!(
                self.variant,
                Variant::ReadOnly | Variant::TooNew | Variant::Error
            ) {
                Access::ReadOnly
            } else {
                Access::ReadWrite
            },
            newer_schema: match self.variant {
                Variant::ReadOnly | Variant::TooNew => Some(5),
                _ => None,
            },
            problems: Vec::new(),
        }
    }

    fn models(&self) -> Vec<Model> {
        let mut models: Vec<Model> = (0..MODELS.len()).map(model).collect();
        if self.variant == Variant::Unpriced {
            models.push(Model {
                id: UNPRICED.to_owned(),
                name: UNPRICED.to_owned(),
            });
        }
        if self.variant == Variant::ManyModels {
            models.extend(OLDER.iter().map(|(id, name, _)| Model {
                id: (*id).to_owned(),
                name: (*name).to_owned(),
            }));
        }
        models
    }

    fn choices(&self) -> Choices {
        let mut projects: Vec<Project> = (0..PROJECTS.len()).map(project).collect();
        projects.sort_by(|a, b| a.name.cmp(&b.name));
        let mut models = self.models();
        models.sort_by(|a, b| a.name.cmp(&b.name));
        Choices { projects, models }
    }

    /// The share of the cost a project filter keeps.
    fn project_share(id: &str) -> f64 {
        PROJECTS
            .iter()
            .find(|(project, ..)| *project == id)
            .map_or(1.0, |(_, _, share, _)| *share)
    }

    fn overview(&self, params: &Value) -> Overview {
        let first: Date = params["days"]["first"]
            .as_str()
            .and_then(|day| day.parse().ok())
            .unwrap_or(date(2026, 9, 11));
        let last: Date = params["days"]["last"]
            .as_str()
            .and_then(|day| day.parse().ok())
            .unwrap_or(date(2026, 10, 10));
        let project = params["filters"]["project"].as_str();
        let model_filter = params["filters"]["model"].as_str();
        let share = project.map_or(1.0, Self::project_share);
        let mut by_day = Vec::new();
        let mut model_totals: Vec<Totals> = vec![Totals::default(); MODELS.len()];
        let mut all = Totals::default();
        for (day, costs) in self
            .days
            .iter()
            .filter(|(day, _)| (first..=last).contains(day))
        {
            let mut day_totals = Totals::default();
            let mut parts: Vec<ModelDay> = Vec::new();
            for (index, cost) in costs.iter().enumerate() {
                if model_filter.is_some_and(|id| id != MODELS[index].0) || *cost <= 0.0 {
                    continue;
                }
                let part = totals(
                    cost * share,
                    MODELS[index].2,
                    if index == 1 { 0.12 } else { 0.0 },
                );
                add(&mut day_totals, &part);
                add(&mut model_totals[index], &part);
                parts.push(ModelDay {
                    model: MODELS[index].0.to_owned(),
                    totals: part,
                });
            }
            if day_totals.requests == 0 {
                continue;
            }
            parts.sort_by(|a, b| b.totals.cost.total.total_cmp(&a.totals.cost.total));
            add(&mut all, &day_totals);
            by_day.push(Day {
                date: day.to_string(),
                totals: day_totals,
                models: parts,
            });
        }
        let mut models: Vec<ModelRow> = model_totals
            .into_iter()
            .enumerate()
            .filter(|(_, totals)| totals.requests > 0)
            .map(|(index, totals)| ModelRow {
                model: model(index),
                totals,
            })
            .collect();
        if model_filter.is_none() {
            for row in self.more_models(share) {
                add(&mut all, &row.totals);
                models.push(row);
            }
        }
        models.sort_by(|a, b| b.totals.cost.total.total_cmp(&a.totals.cost.total));
        let projects = if project.is_some() {
            Vec::new()
        } else {
            self.project_rows(&by_day, all.cost.total)
        };
        Overview {
            days: Days {
                first: first.to_string(),
                last: last.to_string(),
            },
            totals: all,
            by_day,
            models,
            projects,
        }
    }

    /// The models besides the four that the variant has, in the share of the cost a project
    /// filter keeps: none in the chart, so that only the tables show them.
    fn more_models(&self, share: f64) -> Vec<ModelRow> {
        let row = |id: &str, name: &str, totals: Totals| ModelRow {
            model: Model {
                id: id.to_owned(),
                name: name.to_owned(),
            },
            totals,
        };
        match self.variant {
            Variant::Unpriced => vec![row(
                UNPRICED,
                UNPRICED,
                Totals {
                    requests: 12,
                    tokens: Tokens {
                        input: 2_000,
                        output: 8_000,
                        cache_read: 25_000,
                        cache_write_5m: 0,
                        cache_write_1h: 0,
                    },
                    unpriced_requests: 12,
                    ..Totals::default()
                },
            )],
            Variant::ManyModels => OLDER
                .iter()
                .map(|(id, name, cost)| row(id, name, totals(cost * share, 12.0, 0.0)))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn project_rows(&self, by_day: &[Day], total: f64) -> Vec<ProjectRow> {
        let base = self.at(date(2026, 10, 10), 13, 42);
        PROJECTS
            .iter()
            .enumerate()
            .filter(|_| total > 0.0)
            .map(|(index, (_, _, share, sessions))| {
                let wave = |day: usize| {
                    #[allow(clippy::cast_precision_loss)]
                    let at = (day * (index + 2)) as f64 * 0.37;
                    0.6 + 0.8 * at.sin().abs()
                };
                let daily_cost: Vec<DayCost> = by_day
                    .iter()
                    .enumerate()
                    .map(|(day, entry)| DayCost {
                        date: entry.date.clone(),
                        usd: entry.totals.cost.total * share * wave(day),
                    })
                    .collect();
                let cost = total * share;
                ProjectRow {
                    project: project(index),
                    sessions: *sessions,
                    last_ms: base - i64::try_from(index).unwrap_or(0) * 26 * 60 * 60 * 1000,
                    totals: totals(cost, 25.0, 0.0),
                    daily_cost,
                }
            })
            .collect()
    }

    /// Each plan window: its reading, use so far and pace.
    fn windows(&self) -> Vec<WindowView> {
        let stale = self.variant == Variant::Stale;
        let week_resets = self.at(date(2026, 10, 15), 9, 0);
        let five_resets = if stale {
            self.at(date(2026, 10, 7), 18, 20)
        } else {
            self.now_ms() + (2 * 60 + 5) * 60_000
        };
        let observed = if stale {
            self.at(date(2026, 10, 7), 17, 55)
        } else {
            self.now_ms() - 2 * 60_000
        };
        let provenance = if self.variant == Variant::Endpoint {
            Provenance::Experimental
        } else {
            Provenance::Official
        };
        let reading = |window: &str, model: Option<&str>, used: f64, resets: i64, level: Level| {
            WindowReading {
                window: window.to_owned(),
                model: model.map(str::to_owned),
                used_percentage: used,
                resets_at_ms: Some(resets),
                observed_ms: observed,
                provenance,
                level,
                stale,
                expired: stale && resets <= self.now_ms(),
            }
        };
        let series = |start: i64, used: f64, points: i64| -> Vec<UsePoint> {
            (1..=points)
                .map(|point| {
                    #[allow(clippy::cast_precision_loss)]
                    let share = point as f64 / points as f64;
                    UsePoint {
                        ms: start + (observed - start) * point / points,
                        used: used * share * share.sqrt(),
                    }
                })
                .collect()
        };
        let five_start = five_resets - 5 * 60 * 60_000;
        let week_start = week_resets - 7 * 24 * 60 * 60_000;
        let pace = |at_reset: f64, since: i64| Pace {
            at_reset,
            limit_reached_ms: None,
            since_ms: since,
        };
        vec![
            WindowView {
                reading: reading(
                    "five_hour",
                    None,
                    if stale { 64.0 } else { 42.0 },
                    five_resets,
                    Level::Normal,
                ),
                start_ms: Some(five_start),
                series: series(five_start, 42.0, 12),
                pace: (!stale).then(|| pace(61.0, five_start)),
            },
            WindowView {
                reading: reading(
                    "seven_day",
                    None,
                    if stale { 58.0 } else { 71.0 },
                    week_resets,
                    if stale { Level::Normal } else { Level::Warning },
                ),
                start_ms: Some(week_start),
                series: series(week_start, 71.0, 40),
                pace: (!stale).then(|| pace(88.0, week_start)),
            },
            WindowView {
                reading: reading(
                    "seven_day:Fable",
                    Some("Fable"),
                    if stale { 12.0 } else { 18.0 },
                    week_resets,
                    Level::Normal,
                ),
                start_ms: Some(week_start),
                series: series(week_start, 18.0, 30),
                pace: (!stale).then(|| pace(31.0, week_start)),
            },
        ]
    }

    fn limits(&self) -> Limits {
        let empty = matches!(self.variant, Variant::Empty | Variant::LimitsOff);
        let recent = [
            ((2026, 10, 10), (9, 20), 42.0, 5.10),
            ((2026, 10, 9), (18, 5), 64.0, 8.12),
            ((2026, 10, 9), (11, 40), 88.0, 11.04),
            ((2026, 10, 8), (13, 15), 53.0, 6.70),
            ((2026, 10, 7), (9, 0), 37.0, 4.42),
            ((2026, 10, 6), (15, 30), 71.0, 8.90),
            ((2026, 10, 5), (10, 10), 29.0, 3.35),
        ]
        .into_iter()
        .map(|((year, month, day), (hour, minute), highest, cost)| {
            let start = self.at(date(year, month, day), hour, minute);
            FiveHourWindow {
                start_ms: start,
                resets_at_ms: start + 5 * 60 * 60_000,
                highest,
                totals: totals(cost, 20.0, 0.0),
            }
        })
        .collect();
        Limits {
            windows: if empty { Vec::new() } else { self.windows() },
            recent: if empty { Vec::new() } else { recent },
            status_line: if self.variant == Variant::LimitsOff {
                StatusLine::NotConnected
            } else {
                StatusLine::Connected {
                    wrapping: Some("jq -r .model.display_name".to_owned()),
                }
            },
            endpoint: Endpoint {
                enabled: self.variant == Variant::Endpoint,
                asked_ms: (self.variant == Variant::Endpoint)
                    .then(|| self.at(date(2026, 10, 10), 10, 5)),
                outcome: (self.variant == Variant::Endpoint).then(|| "login-expired".to_owned()),
                detail: None,
                next_ask_ms: 0,
                problem: None,
                consent: Vec::new(),
            },
        }
    }

    fn prices(&self, params: &Value) -> Prices {
        let wanted: Vec<&str> = params["models"]
            .as_array()
            .map(|models| models.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let rates = [
            (10.0, 50.0, 12.5, 20.0, 0.25),
            (4.0, 20.0, 5.0, 8.0, 0.2),
            (2.0, 10.0, 2.5, 4.0, 0.1),
            (0.1, 0.5, 0.125, 0.2, 0.01),
        ];
        let models = (0..MODELS.len())
            .filter(|index| wanted.contains(&MODELS[*index].0))
            .map(|index| {
                let (input, output, five_minute_writes, one_hour_writes, reads) = rates[index];
                PriceRow {
                    model: model(index),
                    input,
                    output,
                    cache_write_5m: five_minute_writes,
                    cache_write_1h: one_hour_writes,
                    cache_read: reads,
                    long_prompt: (index == 3).then_some(LongPrompt {
                        above: 100_000,
                        input: 0.5,
                        output: 2.5,
                        cache_write_5m: 0.625,
                        cache_write_1h: 1.0,
                        cache_read: 0.05,
                    }),
                    fast: (index == 1).then_some(FastPrices {
                        input: 8.0,
                        output: 40.0,
                    }),
                }
            })
            .collect();
        let drift = self.variant == Variant::Drift;
        Prices {
            checked: "2026-10-10".to_owned(),
            source: "https://platform.claude.com/docs/en/about-claude/pricing".to_owned(),
            models,
            drift: Some(if drift { -0.062 } else { 0.004 }),
            sessions_compared: 412,
            out_of_line: if drift {
                vec![PriceDrift {
                    model: model(1),
                    drift: -0.062,
                    sessions: 31,
                }]
            } else {
                Vec::new()
            },
        }
    }

    fn import_offer(&self) -> Option<ImportOffer> {
        (self.variant == Variant::Import).then(|| ImportOffer {
            path: r"C:\Users\you\.claude\usage.db".to_owned(),
            requests: 26_297,
            first_day: Some("2026-07-31".to_owned()),
            last_day: Some("2026-09-10".to_owned()),
            dismissed: false,
        })
    }

    fn session_rows(&self) -> Vec<SessionRow> {
        let newest = self.at(date(2026, 10, 10), 13, 42);
        (0..SESSIONS)
            .map(|index| {
                let step = i64::try_from(index).unwrap_or(0);
                let minutes = 12 + (step * 37) % 95;
                #[allow(clippy::cast_precision_loss)]
                let cost = 0.6 + ((index as f64) * 7.3) % 14.2;
                let first_ms = newest - step * 110 * 60_000;
                let title = TITLES[index % TITLES.len()];
                let project_index = [0, 0, 1, 0, 2, 0, 3, 4][index % 8];
                let reported = (index % 5 != 4).then(|| {
                    if index == 6 {
                        cost * 0.93
                    } else {
                        #[allow(clippy::cast_precision_loss)]
                        let off = ((index % 3) as f64 - 1.0) * 0.002;
                        cost * (1.0 + off)
                    }
                });
                SessionRow {
                    id: format!("4f1c2a3b-0000-4000-8000-{index:012}"),
                    title: (!title.is_empty()).then(|| title.to_owned()),
                    project: project(project_index),
                    first_ms,
                    last_ms: first_ms + minutes * 60_000,
                    models: vec![model(1)],
                    subagents: if index % 4 == 0 {
                        2
                    } else {
                        u64::from(index % 3 == 0)
                    },
                    totals: Totals {
                        requests: 40 + (u64::try_from(index).unwrap_or(0) * 53) % 220,
                        ..totals(cost, 20.0, 0.0)
                    },
                    reported_usd: reported,
                    drift: reported.map(|reported| (cost - reported) / reported),
                }
            })
            .collect()
    }

    fn sessions(&self, params: &Value) -> SessionsPage {
        let search = params["search"].as_str().map(str::to_lowercase);
        let rows: Vec<SessionRow> = self
            .session_rows()
            .into_iter()
            .filter(|row| {
                search.as_deref().is_none_or(|search| {
                    row.title
                        .as_deref()
                        .is_some_and(|title| title.to_lowercase().contains(search))
                        || row.project.name.contains(search)
                })
            })
            .collect();
        let offset = usize::try_from(params["offset"].as_u64().unwrap_or(0)).unwrap_or(0);
        let limit = usize::try_from(params["limit"].as_u64().unwrap_or(500)).unwrap_or(500);
        SessionsPage {
            total: u64::try_from(rows.len()).unwrap_or(0),
            rows: rows.into_iter().skip(offset).take(limit).collect(),
        }
    }

    fn session(&self) -> SessionDetail {
        let first = self.at(date(2026, 10, 10), 13, 42);
        let last = self.at(date(2026, 10, 10), 14, 54);
        let mut running = 0.0;
        let mut cumulative = Vec::new();
        for step in 0..60 {
            let at = f64::from(step) * 0.7;
            running += 0.08 + 0.42 * at.sin().abs() * if step % 9 < 6 { 1.0 } else { 0.2 };
            cumulative.push(running);
        }
        let scale = 14.82 / running;
        let cumulative_cost = cumulative
            .iter()
            .enumerate()
            .map(|(step, usd)| CostPoint {
                ms: first + (last - first) * i64::try_from(step + 1).unwrap_or(0) / 60,
                usd: usd * scale,
            })
            .collect();
        let who = |subagent: Option<&str>,
                   agent: Option<&str>,
                   index: usize,
                   (from, to): (i64, i64),
                   requests: u64,
                   cost: f64| WhoRow {
            subagent: subagent.map(str::to_owned),
            agent_type: agent.map(str::to_owned),
            model: model(index),
            first_ms: from,
            last_ms: to,
            totals: Totals {
                requests,
                ..totals(cost, 20.0, 0.0)
            },
        };
        SessionDetail {
            id: "4f1c2a3b-0000-4000-8000-000000000000".to_owned(),
            title: Some("Refactor ingest batching".to_owned()),
            project: project(0),
            transcript: None,
            first_ms: first,
            last_ms: last,
            claude_code_version: Some("2.4.1".to_owned()),
            entrypoint: Some("cli".to_owned()),
            own: Totals {
                requests: 186,
                ..totals(11.64, 16.0, 0.0)
            },
            totals: session_totals(),
            reported_usd: Some(14.80),
            drift: Some(0.0014),
            cumulative_cost,
            who: vec![
                who(None, None, 1, (first, last), 186, 11.64),
                who(
                    Some("a1"),
                    Some("Explore"),
                    3,
                    (
                        self.at(date(2026, 10, 10), 13, 55),
                        self.at(date(2026, 10, 10), 13, 58),
                    ),
                    22,
                    0.31,
                ),
                who(
                    Some("a2"),
                    Some("general-purpose"),
                    2,
                    (
                        self.at(date(2026, 10, 10), 14, 10),
                        self.at(date(2026, 10, 10), 14, 21),
                    ),
                    41,
                    2.87,
                ),
            ],
            skills: vec![
                credit("dataviz", None, 6, 0.92),
                credit("domain-modeling", None, 2, 0.18),
            ],
            mcp_tools: vec![
                credit("github", Some("get_file_contents"), 9, 0.41),
                credit("context7", Some("query-docs"), 3, 0.12),
            ],
            uncredited: Totals {
                requests: 229,
                ..totals(13.19, 17.0, 0.0)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixture_answers_every_request_of_the_screens() {
        let fixture = Fixture::new(Variant::Normal);
        let days = json!({ "first": "2026-09-11", "last": "2026-10-10" });
        for (method, params) in [
            ("status", Value::Null),
            ("overview", json!({ "days": days, "filters": {} })),
            (
                "sessions",
                json!({ "days": days, "filters": {}, "offset": 0, "limit": 500 }),
            ),
            ("prices", json!({ "models": ["claude-opus-5-5"] })),
        ] {
            assert!(fixture.answer(method, &params).is_ok(), "{method}");
        }
    }
}
