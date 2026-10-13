use claude_dashboard_core::history::History;
use claude_dashboard_core::http::Answer;
use claude_dashboard_core::price_refresh::{
    self, Check, EVERY_MS, Outcome, RETRY_MS, TABLE_SETTING, When,
};
use claude_dashboard_core::pricing::PriceTable;
use serde_json::{Value, json};

const NOW: i64 = 1_791_640_800_000;
const MINUTE: i64 = 60 * 1000;

const BUNDLED: &str = include_str!("../../../pricing/prices.json");

struct Fixture {
    _dir: tempfile::TempDir,
    history: History,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let history = History::open(&dir.path().join("history.sqlite")).unwrap();
        Self { _dir: dir, history }
    }

    fn switched_on() -> Self {
        let mut fixture = Self::new();
        price_refresh::switch(&mut fixture.history, true).unwrap();
        fixture
    }

    /// Refreshes at `now_ms` with `answer` as what the repository says, and returns what the
    /// refresh did and whether it asked, with which entity tag.
    fn refresh(
        &mut self,
        now_ms: i64,
        when: When,
        answer: Result<Answer, String>,
    ) -> (Check, Asked) {
        let mut asked = Asked::No;
        let check = price_refresh::refresh_with(&mut self.history, now_ms, when, |etag| {
            asked = etag.map_or(Asked::WithoutTag, |etag| Asked::WithTag(etag.to_owned()));
            answer
        })
        .unwrap();
        (check, asked)
    }

    fn in_use(&self) -> std::sync::Arc<PriceTable> {
        price_refresh::in_use(&self.history.settings().unwrap())
    }

    fn state(&self) -> price_refresh::State {
        price_refresh::state(&self.history.settings().unwrap())
    }
}

/// Whether a refresh asked the repository, and with which entity tag.
#[derive(Debug, PartialEq, Eq)]
enum Asked {
    No,
    WithoutTag,
    WithTag(String),
}

fn table(change: impl FnOnce(&mut Value)) -> String {
    let mut table: Value = serde_json::from_str(BUNDLED).unwrap();
    change(&mut table);
    serde_json::to_string_pretty(&table).unwrap()
}

/// The bundled table, checked later, with a model added.
fn newer() -> String {
    table(|table| {
        table["checked"] = json!("2099-01-01");
        table["models"].as_array_mut().unwrap().push(json!({
            "name": "Claude Orion 1",
            "ids": ["claude-orion-1"],
            "prices": [{ "input": 3, "cache_write_5m": 3.75, "cache_write_1h": 6,
                         "cache_read": 0.3, "output": 15 }]
        }));
    })
}

fn ok(body: &str, etag: &str) -> Answer {
    answer(200, body, Some(etag))
}

fn not_modified() -> Answer {
    answer(304, "", None)
}

fn answer(status: u16, body: &str, etag: Option<&str>) -> Answer {
    Answer {
        status,
        body: body.to_owned(),
        etag: etag.map(str::to_owned),
        retry_after_s: None,
    }
}

#[test]
fn a_refresh_that_is_off_never_asks() {
    let mut fixture = Fixture::new();

    assert_eq!(
        fixture.refresh(NOW, When::Due, Ok(ok(&newer(), "\"a\""))),
        (Check::Off, Asked::No)
    );
    assert!(PriceTable::is_bundled(&fixture.in_use()));
}

#[test]
fn a_newer_table_is_used_and_asked_after_once_a_day_with_its_tag() {
    let mut fixture = Fixture::switched_on();

    assert_eq!(
        fixture.refresh(NOW, When::Due, Ok(ok(&newer(), "\"a\""))),
        (Check::Checked(Outcome::Updated), Asked::WithoutTag)
    );
    assert_eq!(
        fixture.in_use().model_name("claude-orion-1"),
        Some("Claude Orion 1")
    );
    assert_eq!(
        fixture.state(),
        price_refresh::State {
            enabled: true,
            checked_ms: Some(NOW),
            outcome: Some(Outcome::Updated),
            detail: None,
            downloaded_ms: Some(NOW),
        }
    );

    assert_eq!(
        fixture.refresh(NOW + 60 * MINUTE, When::Due, Ok(not_modified())),
        (
            Check::Waiting {
                until_ms: NOW + EVERY_MS
            },
            Asked::No
        )
    );
    assert_eq!(
        fixture.refresh(NOW + EVERY_MS, When::Due, Ok(not_modified())),
        (
            Check::Checked(Outcome::UpToDate),
            Asked::WithTag("\"a\"".to_owned())
        )
    );
    assert_eq!(
        fixture.in_use().model_name("claude-orion-1"),
        Some("Claude Orion 1")
    );
}

#[test]
fn the_same_table_as_this_versions_is_up_to_date() {
    let mut fixture = Fixture::switched_on();

    assert_eq!(
        fixture.refresh(NOW, When::Due, Ok(ok(BUNDLED, "\"a\""))).0,
        Check::Checked(Outcome::UpToDate)
    );
    assert_eq!(*fixture.in_use(), *PriceTable::bundled());
}

#[test]
fn a_table_checked_on_the_same_day_as_this_versions_is_used() {
    let mut fixture = Fixture::switched_on();
    let same_day = table(|table| {
        table["models"].as_array_mut().unwrap().push(json!({
            "name": "Claude Orion 1",
            "ids": ["claude-orion-1"],
            "prices": [{ "input": 3, "cache_write_5m": 3.75, "cache_write_1h": 6,
                         "cache_read": 0.3, "output": 15 }]
        }));
    });

    assert_eq!(
        fixture
            .refresh(NOW, When::Due, Ok(ok(&same_day, "\"a\"")))
            .0,
        Check::Checked(Outcome::Updated)
    );
    assert!(fixture.in_use().model_name("claude-orion-1").is_some());
}

#[test]
fn a_table_that_cannot_replace_this_versions_is_refused_and_the_last_good_one_stays() {
    let older = table(|table| table["checked"] = json!("2026-01-01"));
    let changed = table(|table| {
        table["checked"] = json!("2099-01-01");
        table["models"][0]["prices"][0]["input"] = json!(1);
    });
    let dropped = table(|table| {
        table["checked"] = json!("2099-01-01");
        table["models"].as_array_mut().unwrap().remove(0);
    });
    let searches = table(|table| {
        table["checked"] = json!("2099-01-01");
        table["web_search_per_thousand"] = json!(12);
    });
    let truncated = newer()[..200].to_owned();
    let invalid = table(|table| {
        table["checked"] = json!("2099-01-01");
        table["models"][0]["prices"][0]["output"] = json!(-1);
    });
    for (body, reason) in [
        (older, "before this version's table"),
        (changed, "changes the prices Claude Fable 5.1 had"),
        (dropped, "leaves out Claude Fable 5.1"),
        (searches, "web search"),
        (truncated, "cannot be read"),
        (invalid, "invalid"),
    ] {
        let mut fixture = Fixture::switched_on();
        fixture.refresh(NOW, When::Due, Ok(ok(&newer(), "\"a\"")));

        assert_eq!(
            fixture
                .refresh(NOW + EVERY_MS, When::Due, Ok(ok(&body, "\"b\"")))
                .0,
            Check::Checked(Outcome::Refused),
            "{reason}"
        );
        let state = fixture.state();
        assert_eq!(state.outcome, Some(Outcome::Refused));
        assert!(
            state
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains(reason)),
            "{state:?} does not say {reason}"
        );
        assert!(
            fixture.in_use().model_name("claude-orion-1").is_some(),
            "{reason}"
        );
        // The refused table's tag is not kept, so the next check reads the table again.
        assert_eq!(
            fixture
                .refresh(NOW + 2 * EVERY_MS, When::Due, Ok(not_modified()))
                .1,
            Asked::WithTag("\"a\"".to_owned()),
            "{reason}"
        );
    }
}

#[test]
fn a_table_in_a_newer_format_asks_for_a_newer_claude_dashboard() {
    let mut fixture = Fixture::switched_on();
    let body = r#"{ "format": 2, "anything": ["a later version may hold"] }"#;

    assert_eq!(
        fixture.refresh(NOW, When::Due, Ok(ok(body, "\"a\""))).0,
        Check::Checked(Outcome::NewerFormat)
    );
    assert_eq!(fixture.state().detail.as_deref(), Some("format 2"));
    assert!(PriceTable::is_bundled(&fixture.in_use()));
}

#[test]
fn a_repository_out_of_reach_is_asked_again_after_an_hour() {
    let mut fixture = Fixture::switched_on();

    assert_eq!(
        fixture
            .refresh(NOW, When::Due, Err("timed out".to_owned()))
            .0,
        Check::Checked(Outcome::Unreachable)
    );
    assert_eq!(fixture.state().detail.as_deref(), Some("timed out"));
    assert_eq!(
        fixture
            .refresh(NOW + 30 * MINUTE, When::Due, Ok(ok(&newer(), "\"a\"")))
            .0,
        Check::Waiting {
            until_ms: NOW + RETRY_MS
        }
    );
    let status = Ok(answer(503, "", None));
    assert_eq!(
        fixture.refresh(NOW + RETRY_MS, When::Due, status).0,
        Check::Checked(Outcome::Unreachable)
    );
    assert_eq!(
        fixture.state().detail.as_deref(),
        Some("it answered with HTTP status 503")
    );
    assert_eq!(
        fixture
            .refresh(NOW + 2 * RETRY_MS, When::Due, Ok(ok(&newer(), "\"a\"")))
            .0,
        Check::Checked(Outcome::Updated)
    );
}

#[test]
fn an_update_asks_at_once_whether_the_refresh_is_on_or_not() {
    let mut fixture = Fixture::new();

    assert_eq!(
        fixture.refresh(NOW, When::Now, Ok(ok(&newer(), "\"a\""))).0,
        Check::Checked(Outcome::Updated)
    );
    assert_eq!(
        fixture
            .refresh(NOW + MINUTE, When::Now, Ok(not_modified()))
            .0,
        Check::Checked(Outcome::UpToDate)
    );
    assert!(!fixture.state().enabled);
}

#[test]
fn switching_on_makes_the_next_check_due_at_once() {
    let mut fixture = Fixture::switched_on();
    fixture.refresh(NOW, When::Due, Ok(ok(&newer(), "\"a\"")));
    price_refresh::switch(&mut fixture.history, false).unwrap();
    assert_eq!(
        fixture
            .refresh(NOW + MINUTE, When::Due, Ok(not_modified()))
            .0,
        Check::Off
    );

    price_refresh::switch(&mut fixture.history, true).unwrap();

    assert_eq!(
        fixture
            .refresh(NOW + 2 * MINUTE, When::Due, Ok(not_modified()))
            .0,
        Check::Checked(Outcome::UpToDate)
    );
}

#[test]
fn a_stored_table_older_than_this_versions_gives_way_to_it() {
    let mut fixture = Fixture::new();
    let older = table(|table| table["checked"] = json!("2026-01-01"));
    fixture
        .history
        .write(|w| w.set_setting(TABLE_SETTING, Some(&older)))
        .unwrap();

    assert!(PriceTable::is_bundled(&fixture.in_use()));
}

#[test]
fn only_one_process_takes_the_days_check() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite");
    let mut first = History::open(&path).unwrap();
    let mut second = History::open(&path).unwrap();
    price_refresh::switch(&mut first, true).unwrap();

    let mut asked = 0;
    for history in [&mut first, &mut second] {
        price_refresh::refresh_with(history, NOW, When::Due, |_| {
            asked += 1;
            Ok(ok(&newer(), "\"a\""))
        })
        .unwrap();
    }

    assert_eq!(asked, 1);
}
