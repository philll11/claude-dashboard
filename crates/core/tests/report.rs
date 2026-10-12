use claude_dashboard_core::history::{Attribution, History, Identity, Request, SessionKey};
use claude_dashboard_core::pricing::PriceTable;
use claude_dashboard_core::report::{self, Bucket, Filter, Range};
use claude_dashboard_core::tokens::Tokens;
use jiff::civil::{Date, Weekday, date};
use jiff::tz::TimeZone;
use serde_json::Value;

/// 2026-10-09T23:30:00Z, half an hour before a day ends in UTC.
const EVENING: i64 = 1_791_588_600_000;
const MINUTE: i64 = 60 * 1000;

struct Fixture {
    _dir: tempfile::TempDir,
    history: History,
    app: SessionKey,
    site: SessionKey,
}

/// What one request carries, beyond its session, model and time.
#[derive(Clone, Copy, Default)]
struct Extra<'a> {
    subagent: Option<&'a str>,
    skill: Option<&'a str>,
    mcp: Option<(&'a str, &'a str)>,
    fast: bool,
}

impl Fixture {
    /// Two projects: `app` with one session, `site` with another.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut history = History::open(&dir.path().join("history.sqlite")).unwrap();
        let (app, site) = history
            .write(|w| {
                let app = w.project("C--code-app", Some(r"C:\code\app"))?;
                let site = w.project("C--code-site", Some(r"C:\code\site"))?;
                Ok((
                    w.session("session-app", app)?,
                    w.session("session-site", site)?,
                ))
            })
            .unwrap();
        Self {
            _dir: dir,
            history,
            app,
            site,
        }
    }

    fn request(
        &mut self,
        session: SessionKey,
        id: &str,
        model: &str,
        at_ms: i64,
        extra: Extra<'_>,
    ) {
        let message = format!("msg_{id}");
        let request = format!("req_{id}");
        let record = format!("rec-{id}");
        self.history
            .write(|w| {
                w.request(&Request {
                    session,
                    identity: Identity::from_ids(Some(&message), Some(&request)),
                    record_id: &record,
                    subagent_id: extra.subagent,
                    timestamp_ms: at_ms,
                    model,
                    tokens: Tokens {
                        input: 10,
                        output: 1_000,
                        cache_read: 100_000,
                        cache_write_5m: 2_000,
                        cache_write_1h: 0,
                    },
                    web_search_requests: 0,
                    web_fetch_requests: 0,
                    speed: extra.fast.then_some("fast"),
                    service_tier: Some("standard"),
                    inference_geo: None,
                    attribution: Attribution {
                        skill: extra.skill,
                        mcp_server: extra.mcp.map(|(server, _)| server),
                        mcp_tool: extra.mcp.map(|(_, tool)| tool),
                    },
                    entrypoint: Some("cli"),
                    claude_code_version: Some("2.1.296"),
                    imported_from: None,
                })
            })
            .unwrap();
    }

    fn table() -> &'static PriceTable {
        PriceTable::bundled()
    }

    fn everything() -> Range {
        Range {
            from_ms: 0,
            to_ms: i64::MAX,
        }
    }
}

/// One request of the fixture's shape on `model`, in dollars.
fn cost_of(model: &str, fast: bool) -> f64 {
    Fixture::table()
        .cost(&claude_dashboard_core::pricing::Request {
            model,
            timestamp_ms: EVENING,
            tokens: Tokens {
                input: 10,
                output: 1_000,
                cache_read: 100_000,
                cache_write_5m: 2_000,
                cache_write_1h: 0,
            },
            speed: fast.then_some("fast"),
            inference_geo: None,
            web_search_requests: 0,
        })
        .unwrap()
        .total()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn a_summary_adds_up_days_models_and_projects_in_one_pass() {
    let mut fixture = Fixture::new();
    let (app, site) = (fixture.app, fixture.site);
    fixture.request(app, "1", "claude-opus-5-5", EVENING, Extra::default());
    fixture.request(
        app,
        "2",
        "claude-sonnet-5-5",
        EVENING + 40 * MINUTE,
        Extra::default(),
    );
    fixture.request(
        site,
        "3",
        "claude-opus-5-5",
        EVENING + 50 * MINUTE,
        Extra {
            fast: true,
            ..Extra::default()
        },
    );

    let summary = report::summary(
        &fixture.history,
        Fixture::table(),
        Fixture::everything(),
        &Filter::default(),
        &TimeZone::UTC,
    )
    .unwrap();

    assert_eq!(summary.totals.requests, 3);
    assert_eq!(summary.totals.fast_requests, 1);
    let days: Vec<_> = summary
        .days
        .iter()
        .map(|day| (day.day, day.totals.requests))
        .collect();
    assert_eq!(days, [(date(2026, 10, 9), 1), (date(2026, 10, 10), 2)]);
    let models: Vec<_> = summary
        .models
        .iter()
        .map(|model| (model.model.as_str(), model.totals.requests))
        .collect();
    assert_eq!(models, [("claude-opus-5-5", 2), ("claude-sonnet-5-5", 1)]);
    let opus = cost_of("claude-opus-5-5", false);
    let opus_fast = cost_of("claude-opus-5-5", true);
    let sonnet = cost_of("claude-sonnet-5-5", false);
    assert!(close(
        summary.totals.cost.total(),
        opus + opus_fast + sonnet
    ));
    let projects: Vec<_> = summary
        .projects
        .iter()
        .map(|project| {
            (
                project.totals.project,
                project.totals.sessions,
                project.daily_cost.len(),
            )
        })
        .collect();
    // The fast request makes the site the more costly project.
    assert!(opus_fast > opus + sonnet);
    assert_eq!(
        projects,
        [
            (site_project(&fixture), 1, 1),
            (app_project(&fixture), 1, 2)
        ]
    );
}

fn project_of(fixture: &Fixture, session_id: &str) -> claude_dashboard_core::history::ProjectKey {
    fixture
        .history
        .sessions()
        .unwrap()
        .into_iter()
        .find(|session| session.session_id == session_id)
        .unwrap()
        .project
}

fn app_project(fixture: &Fixture) -> claude_dashboard_core::history::ProjectKey {
    project_of(fixture, "session-app")
}

fn site_project(fixture: &Fixture) -> claude_dashboard_core::history::ProjectKey {
    project_of(fixture, "session-site")
}

#[test]
fn a_filter_keeps_one_project_or_one_model() {
    let mut fixture = Fixture::new();
    let (app, site) = (fixture.app, fixture.site);
    fixture.request(app, "1", "claude-opus-5-5", EVENING, Extra::default());
    fixture.request(
        app,
        "2",
        "claude-sonnet-5-5",
        EVENING + MINUTE,
        Extra::default(),
    );
    fixture.request(
        site,
        "3",
        "claude-opus-5-5",
        EVENING + 2 * MINUTE,
        Extra::default(),
    );
    let requests = |filter: &Filter| {
        report::summary(
            &fixture.history,
            Fixture::table(),
            Fixture::everything(),
            filter,
            &TimeZone::UTC,
        )
        .unwrap()
        .totals
        .requests
    };

    let app_only = Filter {
        project: Some(app_project(&fixture)),
        model: None,
    };
    let opus_only = Filter {
        project: None,
        model: Some("claude-opus-5-5".to_owned()),
    };
    assert_eq!(requests(&app_only), 2);
    assert_eq!(requests(&opus_only), 2);
    assert_eq!(
        requests(&Filter {
            project: app_only.project,
            model: opus_only.model.clone(),
        }),
        1
    );
    let sessions = report::sessions_matching(
        &fixture.history,
        Fixture::table(),
        Fixture::everything(),
        &Filter {
            project: Some(site_project(&fixture)),
            model: None,
        },
    )
    .unwrap();
    assert_eq!(
        sessions
            .iter()
            .map(|session| session.session)
            .collect::<Vec<_>>(),
        [site]
    );
}

#[test]
fn a_sessions_detail_separates_subagents_and_credits() {
    let mut fixture = Fixture::new();
    let app = fixture.app;
    fixture.request(app, "1", "claude-opus-5-5", EVENING, Extra::default());
    fixture.request(
        app,
        "2",
        "claude-haiku-5-5",
        EVENING + 5 * MINUTE,
        Extra {
            subagent: Some("a1"),
            skill: Some("research"),
            ..Extra::default()
        },
    );
    fixture.request(
        app,
        "3",
        "claude-opus-5-5",
        EVENING + 9 * MINUTE,
        Extra {
            mcp: Some(("github", "search_code")),
            ..Extra::default()
        },
    );

    let detail = report::session_detail(&fixture.history, Fixture::table(), app)
        .unwrap()
        .unwrap();

    assert_eq!(
        (detail.first_ms, detail.last_ms),
        (EVENING, EVENING + 9 * MINUTE)
    );
    assert_eq!((detail.totals.requests, detail.own.requests), (3, 2));
    assert_eq!(
        detail
            .subagents
            .iter()
            .map(|subagent| (
                subagent.subagent_id.as_str(),
                subagent.first_ms,
                subagent.totals.requests
            ))
            .collect::<Vec<_>>(),
        [("a1", EVENING + 5 * MINUTE, 1)]
    );
    assert_eq!(
        detail
            .own_models
            .iter()
            .map(|model| model.model.as_str())
            .collect::<Vec<_>>(),
        ["claude-opus-5-5"]
    );
    let times: Vec<i64> = detail.cumulative_cost.iter().map(|(at, _)| *at).collect();
    assert_eq!(times, [EVENING, EVENING + 5 * MINUTE, EVENING + 9 * MINUTE]);
    assert!(
        detail
            .cumulative_cost
            .windows(2)
            .all(|pair| pair[0].1 < pair[1].1)
    );
    assert!(close(
        detail.cumulative_cost.last().unwrap().1,
        detail.totals.cost.total()
    ));
    assert_eq!(
        detail
            .skills
            .iter()
            .map(|credit| (credit.name.as_str(), credit.totals.requests))
            .collect::<Vec<_>>(),
        [("research", 1)]
    );
    assert_eq!(
        detail
            .mcp_tools
            .iter()
            .map(|credit| (
                credit.name.as_str(),
                credit.tool.as_deref(),
                credit.totals.requests
            ))
            .collect::<Vec<_>>(),
        [("github", Some("search_code"), 1)]
    );
    assert_eq!(detail.uncredited.requests, 1);
    assert_eq!(detail.claude_code_version.as_deref(), Some("2.1.296"));
    assert_eq!(detail.entrypoint.as_deref(), Some("cli"));
}

#[test]
fn a_session_without_requests_has_no_detail() {
    let fixture = Fixture::new();
    assert_eq!(
        report::session_detail(&fixture.history, Fixture::table(), fixture.site).unwrap(),
        None
    );
}

fn weekday(name: &str) -> Weekday {
    match name {
        "monday" => Weekday::Monday,
        "tuesday" => Weekday::Tuesday,
        "wednesday" => Weekday::Wednesday,
        "thursday" => Weekday::Thursday,
        "friday" => Weekday::Friday,
        "saturday" => Weekday::Saturday,
        "sunday" => Weekday::Sunday,
        other => panic!("{other} is not a day of the week"),
    }
}

fn day(value: &Value) -> Date {
    value.as_str().unwrap().parse().unwrap()
}

#[test]
fn days_fall_into_weeks_and_months_as_on_every_surface() {
    let examples: Value =
        serde_json::from_str(include_str!("../../../design/buckets.json")).unwrap();
    for pair in examples["bucketOf"].as_array().unwrap() {
        let bucket = match pair[1].as_str().unwrap() {
            "day" => Bucket::Day,
            "week" => Bucket::Week,
            _ => Bucket::Month,
        };
        assert_eq!(Bucket::of(pair[0].as_i64().unwrap()), bucket, "{pair}");
    }
    for example in examples["weekStarts"].as_array().unwrap() {
        let start = Bucket::Week.start(day(&example[0]), weekday(example[1].as_str().unwrap()));
        assert_eq!(start, day(&example[2]), "{example}");
    }
    for example in examples["monthStarts"].as_array().unwrap() {
        let start = Bucket::Month.start(day(&example[0]), Weekday::Monday);
        assert_eq!(start, day(&example[1]), "{example}");
    }
    assert_eq!(Bucket::Week.end(date(2026, 10, 5)), date(2026, 10, 11));
    assert_eq!(Bucket::Month.end(date(2026, 2, 1)), date(2026, 2, 28));
    assert_eq!(Bucket::Day.end(date(2026, 2, 1)), date(2026, 2, 1));
}

#[test]
fn days_add_up_by_week_and_by_month() {
    let mut fixture = Fixture::new();
    let app = fixture.app;
    let at = |day: Date| {
        day.to_zoned(TimeZone::UTC)
            .unwrap()
            .timestamp()
            .as_millisecond()
            + 12 * 60 * MINUTE
    };
    for (id, model, day) in [
        ("1", "claude-opus-5-5", date(2026, 9, 30)),
        ("2", "claude-opus-5-5", date(2026, 10, 9)),
        ("3", "claude-sonnet-5-5", date(2026, 10, 10)),
        ("4", "claude-opus-5-5", date(2026, 10, 12)),
    ] {
        fixture.request(app, id, model, at(day), Extra::default());
    }
    let days = report::by_day(
        &fixture.history,
        Fixture::table(),
        Fixture::everything(),
        &TimeZone::UTC,
    )
    .unwrap();
    let summed = |bucket| {
        report::by_bucket(&days, bucket, Weekday::Monday)
            .iter()
            .map(|total| {
                (
                    total.day,
                    total.totals.requests,
                    total
                        .models
                        .iter()
                        .map(|model| model.model.clone())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    };

    let opus = || "claude-opus-5-5".to_owned();
    let sonnet = || "claude-sonnet-5-5".to_owned();
    assert_eq!(
        summed(Bucket::Week),
        [
            (date(2026, 9, 28), 1, vec![opus()]),
            (date(2026, 10, 5), 2, vec![opus(), sonnet()]),
            (date(2026, 10, 12), 1, vec![opus()]),
        ]
    );
    assert_eq!(
        summed(Bucket::Month),
        [
            (date(2026, 9, 1), 1, vec![opus()]),
            (date(2026, 10, 1), 3, vec![opus(), sonnet()]),
        ]
    );
    assert_eq!(summed(Bucket::Day).len(), 4);
    let weeks = report::by_bucket(&days, Bucket::Week, Weekday::Monday);
    assert!(close(
        weeks[1].totals.cost.total(),
        cost_of("claude-opus-5-5", false) + cost_of("claude-sonnet-5-5", false)
    ));
}
