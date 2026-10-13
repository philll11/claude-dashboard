use claude_dashboard_core::history::{
    Attribution, History, Identity, ReportedCost, Request, SessionKey,
};
use claude_dashboard_core::pricing::PriceTable;
use claude_dashboard_core::reconcile::{self, Reconciliation};
use claude_dashboard_core::report;
use claude_dashboard_core::tokens::Tokens;

/// 2026-10-10T06:16:27.123Z.
const REQUEST_MS: i64 = 1_791_612_987_123;
const HOUR_MS: i64 = 60 * 60 * 1000;

/// The tokens of one Claude Opus 5.5 request, whose API-equivalent cost is $0.031712.
const TOKENS: Tokens = Tokens {
    input: 3,
    output: 640,
    cache_read: 42_000,
    cache_write_5m: 500,
    cache_write_1h: 1_000,
};
const COST: f64 = 0.031_712;

struct Fixture {
    history: History,
    /// Last, so that the history in it closes first: fields drop in the order they are declared,
    /// and Windows cannot remove a folder that holds an open file.
    _dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let history = History::open(&dir.path().join("history.sqlite")).unwrap();
        Self { history, _dir: dir }
    }

    fn session(&mut self, session_id: &str) -> SessionKey {
        self.history
            .write(|w| {
                let project = w.project("C--code-app", None)?;
                w.session(session_id, project)
            })
            .unwrap()
    }

    fn request(&mut self, session: SessionKey, id: &str, model: &str, timestamp_ms: i64) {
        self.history
            .write(|w| {
                w.request(&Request {
                    session,
                    identity: Identity::from_ids(Some(id), Some(id)),
                    record_id: id,
                    subagent_id: None,
                    timestamp_ms,
                    model,
                    tokens: TOKENS,
                    web_search_requests: 0,
                    web_fetch_requests: 0,
                    speed: None,
                    service_tier: None,
                    inference_geo: None,
                    attribution: Attribution::default(),
                    entrypoint: None,
                    claude_code_version: None,
                    imported_from: None,
                })
            })
            .unwrap();
    }

    fn reported(&mut self, session: SessionKey, model: &str, cost_usd: f64, output: u64) {
        self.history
            .write(|w| {
                w.reported_cost(
                    session,
                    REQUEST_MS - HOUR_MS,
                    &ReportedCost {
                        model,
                        cost_usd,
                        input: 9_000,
                        output,
                        cache_read: TOKENS.cache_read,
                        cache_write: TOKENS.cache_write_5m + TOKENS.cache_write_1h,
                    },
                )
            })
            .unwrap();
    }

    fn reconcile(&self) -> Reconciliation {
        reconcile::reconcile(
            &self.history,
            PriceTable::bundled(),
            REQUEST_MS + 2 * HOUR_MS,
        )
        .unwrap()
    }
}

#[test]
fn matching_sessions_agree_when_the_prices_do() {
    let mut fixture = Fixture::new();
    let session = fixture.session("session-1");
    fixture.request(session, "req_1", "claude-opus-5-5", REQUEST_MS);
    fixture.reported(session, "claude-opus-5-5", COST, TOKENS.output);

    let result = fixture.reconcile();

    assert_eq!(result.sessions, 1);
    assert_eq!(result.not_comparable, 0);
    assert_eq!(result.models.len(), 1);
    assert!(result.drift().unwrap().abs() < 1e-9);
    assert_eq!(result.out_of_line().count(), 0);
}

#[test]
fn a_stale_price_shows_as_drift() {
    let mut fixture = Fixture::new();
    for n in 0..40 {
        let session = fixture.session(&format!("session-{n}"));
        fixture.request(session, &format!("req_{n}"), "claude-opus-5-5", REQUEST_MS);
        fixture.reported(session, "claude-opus-5-5", COST * 1.1, TOKENS.output);
    }

    let result = fixture.reconcile();

    let drift = result.drift().unwrap();
    assert!((drift - (1.0 / 1.1 - 1.0)).abs() < 1e-9, "{drift}");
    let out_of_line: Vec<_> = result
        .out_of_line()
        .map(|model| model.model.as_str())
        .collect();
    assert_eq!(out_of_line, ["claude-opus-5-5"]);
}

#[test]
fn a_small_drift_or_a_small_amount_is_no_warning() {
    let mut fixture = Fixture::new();
    for n in 0..40 {
        let session = fixture.session(&format!("session-{n}"));
        fixture.request(session, &format!("req_{n}"), "claude-opus-5-5", REQUEST_MS);
        fixture.reported(session, "claude-opus-5-5", COST * 1.01, TOKENS.output);
    }
    let session = fixture.session("small");
    fixture.request(session, "req_small", "claude-sonnet-5-5", REQUEST_MS);
    fixture.reported(session, "claude-sonnet-5-5", 0.5, TOKENS.output);

    assert_eq!(fixture.reconcile().out_of_line().count(), 0);
}

#[test]
fn sessions_whose_tokens_disagree_are_not_compared() {
    let mut fixture = Fixture::new();
    let session = fixture.session("session-1");
    fixture.request(session, "req_1", "claude-opus-5-5", REQUEST_MS);
    fixture.reported(session, "claude-opus-5-5", COST * 4.0, TOKENS.output * 4);
    let titles = fixture.session("session-2");
    fixture.reported(titles, "claude-haiku-4-5-20251001", 0.01, 40);

    let result = fixture.reconcile();

    assert_eq!(result.sessions, 0);
    assert_eq!(result.not_comparable, 2);
    assert_eq!(result.drift(), None);
}

#[test]
fn a_context_variant_matches_its_model() {
    let mut fixture = Fixture::new();
    let session = fixture.session("session-1");
    fixture.request(session, "req_1", "claude-opus-5", REQUEST_MS);
    fixture.reported(session, "claude-opus-5[1m]", 0.025, TOKENS.output);

    let result = fixture.reconcile();

    assert_eq!(result.models.len(), 1);
    assert_eq!(result.models[0].model, "claude-opus-5");
}

#[test]
fn a_session_with_a_request_in_the_last_hour_waits() {
    let mut fixture = Fixture::new();
    let session = fixture.session("session-1");
    fixture.request(session, "req_1", "claude-opus-5-5", REQUEST_MS);
    fixture.request(
        session,
        "req_2",
        "claude-opus-5-5",
        REQUEST_MS + 90 * 60 * 1000,
    );
    fixture.reported(session, "claude-opus-5-5", COST * 2.0, TOKENS.output * 2);

    let result = fixture.reconcile();

    assert_eq!((result.sessions, result.not_comparable), (0, 0));
}

#[test]
fn an_unpriced_model_has_no_drift() {
    let mut fixture = Fixture::new();
    let session = fixture.session("session-1");
    fixture.request(session, "req_1", "claude-orion-1", REQUEST_MS);
    fixture.reported(session, "claude-orion-1", 3.0, TOKENS.output);

    let result = fixture.reconcile();

    assert_eq!(result.models[0].api_equivalent_usd, None);
    assert_eq!(result.models[0].drift(), None);
    assert_eq!(result.drift(), None);
}

impl Fixture {
    /// The session's drift from Claude Code's own figure, as the dashboard shows it.
    fn session_drift(&self, session: SessionKey) -> Option<f64> {
        let detail = report::session_detail(&self.history, PriceTable::bundled(), session)
            .unwrap()
            .unwrap();
        let reported = reconcile::reported(&self.history).unwrap();
        reconcile::session_drift(&detail.models, reported.get(&session)?)
    }
}

#[test]
fn a_session_is_compared_with_claude_codes_figure_for_the_same_requests() {
    let mut fixture = Fixture::new();
    let session = fixture.session("session-1");
    fixture.request(session, "req_1", "claude-opus-5-5", REQUEST_MS);
    fixture.reported(session, "claude-opus-5-5", COST * 1.1, TOKENS.output);
    // A call that gave the session its title, which no transcript records.
    fixture.reported(session, "claude-haiku-5-5", 0.000_5, 20);

    let drift = fixture.session_drift(session).unwrap();
    assert!((drift - (1.0 / 1.1 - 1.0)).abs() < 1e-9, "{drift}");
}

#[test]
fn one_sessions_figures_are_those_reported_for_it_among_every_session() {
    let mut fixture = Fixture::new();
    let first = fixture.session("session-1");
    let second = fixture.session("session-2");
    let quiet = fixture.session("session-3");
    fixture.reported(first, "claude-opus-5-5[1m]", 1.5, 300);
    fixture.reported(first, "claude-haiku-5-5", 0.000_5, 20);
    fixture.reported(second, "claude-opus-5-5", 2.0, 400);

    let every = reconcile::reported(&fixture.history).unwrap();
    for session in [first, second] {
        assert_eq!(
            reconcile::reported_for(&fixture.history, session).unwrap(),
            every[&session]
        );
    }
    assert!(
        reconcile::reported_for(&fixture.history, quiet)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_session_still_running_after_a_process_ended_is_not_compared() {
    let mut fixture = Fixture::new();
    let session = fixture.session("session-1");
    fixture.request(session, "req_1", "claude-opus-5-5", REQUEST_MS);
    // The session goes on in a process that has not written its cost yet.
    fixture.request(session, "req_2", "claude-opus-5-5", REQUEST_MS + HOUR_MS);
    fixture.request(session, "req_3", "claude-sonnet-5-5", REQUEST_MS + HOUR_MS);
    fixture.reported(session, "claude-opus-5-5", COST, TOKENS.output);

    assert_eq!(fixture.session_drift(session), None);
}
