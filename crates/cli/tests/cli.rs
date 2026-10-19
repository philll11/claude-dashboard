mod common;

use std::fs;
use std::path::{Path, PathBuf};

use predicates::prelude::*;
use predicates::str::contains;
use serde_json::{Value, json};

use common::{Fixture, NOW, request, today_at};

const TODAY_NOON: &str = "2026-10-10T12:00:00Z";

#[test]
fn doctor_reports_the_data_folder_history_prices_and_limits() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[request("1", "claude-opus-5-5", &today_at("00:00:01"))],
    );

    let output = fixture.cli(&["doctor"]).assert().success();

    let root = fixture.root().display().to_string();
    output
        .stdout(contains(format!("✓ Data folder    {root}   1 transcript")))
        .stdout(contains("(CLAUDE_CONFIG_DIR)"))
        .stdout(contains("✓ History        "))
        .stdout(contains("integrity check passed"))
        .stdout(contains(
            "✓ Prices         table of 10 Oct 2026, all 1 model priced",
        ))
        .stdout(contains("- Limits         status line not connected"))
        .stdout(contains(
            "- Limits         usage endpoint off (experimental, opt-in)",
        ));
}

#[test]
fn doctor_fails_when_no_data_folder_exists() {
    let fixture = Fixture::new();
    fixture
        .cli(&["doctor"])
        .assert()
        .failure()
        .stdout(contains("✗ Data folder"))
        .stdout(contains("not found"));
}

#[test]
fn today_shows_each_model_and_the_total() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[
            request("1", "claude-opus-5-5", &today_at("00:00:01")),
            request("2", "claude-opus-5-5", &today_at("00:00:02")),
            request("3", "claude-sonnet-5-5", &today_at("00:00:03")),
        ],
    );

    fixture
        .cli(&["today"])
        .assert()
        .success()
        .stdout(contains("API-equivalent estimate · prices of 10 Oct 2026"))
        .stdout(contains(
            "Model         Requests    Output   Cache writes   Cache reads     Cost",
        ))
        .stdout(contains(
            "■ Opus 5.5           2        1K             3K           84K    $0.06",
        ))
        .stdout(contains(
            "■ Sonnet 5.5         1       640             2K           42K    $0.02",
        ))
        .stdout(contains(
            "  Total              3        2K             5K          126K    $0.08",
        ))
        .stdout(contains(
            "Plan limits: not recorded yet. To record them, run claude-dashboard statusline connect",
        ));
}

#[test]
fn today_without_requests_says_so() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root().join("projects")).unwrap();
    fixture
        .cli(&["today"])
        .assert()
        .success()
        .stdout(contains("No requests today."));
}

#[test]
fn daily_writes_json_and_csv() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[
            request("1", "claude-opus-5-5", "2026-10-05T10:00:00Z"),
            request("2", "claude-sonnet-5-5", "2026-10-06T10:00:00Z"),
        ],
    );

    let output = fixture
        .cli(&[
            "daily",
            "--since",
            "2026-10-05",
            "--until",
            "2026-10-06",
            "--json",
        ])
        .assert()
        .success();
    let days: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(days.as_array().unwrap().len(), 2);
    assert_eq!(days[0]["date"], "2026-10-05");
    assert_eq!(days[0]["requests"], 1);
    assert_eq!(days[0]["tokens"]["cache_write_1h"], 1000);
    assert_eq!(days[0]["cost"]["usd"], 0.03);
    assert_eq!(days[0]["cost"]["provenance"], "estimate");
    assert_eq!(days[0]["cost"]["price_table"], "2026-10-10");
    assert_eq!(days[0]["models"], json!(["claude-opus-5-5"]));

    fixture
        .cli(&[
            "daily",
            "--since",
            "2026-10-05",
            "--until",
            "2026-10-06",
            "--csv",
        ])
        .assert()
        .success()
        .stdout(contains("date,requests,input_tokens,output_tokens"))
        .stdout(contains("2026-10-06,1,3,640,500,1000,42000,"));
}

fn status_line_input(fixture: &Fixture) -> String {
    let transcript = fixture
        .root()
        .join("projects")
        .join("C--code-app")
        .join("s1.jsonl");
    json!({
        "session_id": "s1",
        "transcript_path": transcript,
        "model": {"id": "claude-opus-5-5", "display_name": "Opus"},
        "rate_limits": {
            "five_hour": {"used_percentage": 42.0, "resets_at": 4_102_444_800_i64},
            "seven_day": {"used_percentage": 71.4, "resets_at": 4_103_049_600_i64}
        }
    })
    .to_string()
}

#[test]
fn the_status_line_records_the_limits_and_shows_them() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[request("1", "claude-opus-5-5", &today_at("00:00:01"))],
    );

    fixture
        .cli(&["statusline"])
        .write_stdin(status_line_input(&fixture))
        .assert()
        .success()
        .stdout("Opus 5.5 · 5h 42% · week 71% ▲ · $0.03 today");

    fixture
        .cli(&["today"])
        .assert()
        .success()
        .stdout(contains(
            "Plan limits, official, just now: five-hour 42% resets",
        ))
        .stdout(contains("weekly 71% ▲ resets"));
}

#[test]
fn the_status_line_runs_your_own_after_recording() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root()).unwrap();

    fixture
        .cli(&["statusline", "--then", "echo", "your own line"])
        .write_stdin(status_line_input(&fixture))
        .assert()
        .success()
        .stdout(contains("your own line"));

    fixture
        .cli(&["today"])
        .assert()
        .stdout(contains("five-hour 42%"));
}

#[test]
fn the_status_line_starts_with_the_sessions_title() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[request("1", "claude-opus-5-5", &today_at("00:00:01"))],
    );
    let mut input: Value = serde_json::from_str(&status_line_input(&fixture)).unwrap();

    input["session_name"] = json!("Fix the\nimporter");
    fixture
        .cli(&["statusline"])
        .write_stdin(input.to_string())
        .assert()
        .success()
        .stdout("Fix the importer · Opus 5.5 · 5h 42% · week 71% ▲ · $0.03 today");

    input["session_name"] = json!("Find out why the nightly import drops readings on Sundays");
    fixture
        .cli(&["statusline"])
        .write_stdin(input.to_string())
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            "Find out why the nightly import drops r… · Opus 5.5",
        ));

    // Bold in the terminal's own colour, which reads on a light background as on a dark one.
    input["session_name"] = json!("Fix the importer");
    fixture
        .cli(&["statusline"])
        .env_remove("NO_COLOR")
        .write_stdin(input.to_string())
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            "\u{1b}[1mFix the importer\u{1b}[0m ",
        ));
}

fn program() -> String {
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_claude-dashboard"));
    let path = fs::canonicalize(exe).unwrap().display().to_string();
    path.strip_prefix(r"\\?\")
        .unwrap_or(&path)
        .replace('\\', "/")
}

#[test]
fn connecting_adds_the_status_line_and_disconnecting_removes_it() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root()).unwrap();
    fs::write(
        fixture.root().join("settings.json"),
        "{\n  \"model\": \"opus\",\n  \"theme\": \"dark\"\n}\n",
    )
    .unwrap();

    fixture
        .cli_without_path(&["statusline", "connect", "--yes"])
        .assert()
        .success()
        .stdout(contains("This adds one setting to"))
        .stdout(contains(format!(
            "+ \"statusLine\": {{ \"type\": \"command\", \"command\": \"{} statusline\" }}",
            program()
        )))
        .stdout(contains("Connected."));
    let settings = fixture.settings();
    let keys: Vec<&str> = settings
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["model", "theme", "statusLine"]);
    assert!(
        fixture
            .root()
            .join("settings.claude-dashboard-backup.json")
            .is_file()
    );

    fixture
        .cli_without_path(&["statusline", "connect", "--yes"])
        .assert()
        .success()
        .stdout(contains("already connected"));

    fixture
        .cli_without_path(&["statusline", "disconnect", "--yes"])
        .assert()
        .success()
        .stdout(contains("This removes one setting from"))
        .stdout(contains("Disconnected."));
    assert_eq!(
        fixture.settings(),
        json!({"model": "opus", "theme": "dark"})
    );
}

#[test]
fn connecting_wraps_your_own_status_line_and_disconnecting_restores_it() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root()).unwrap();
    let own = r#"jq -r '"[\(.model.display_name)]"'"#;
    fs::write(
        fixture.root().join("settings.json"),
        json!({"statusLine": {"type": "command", "command": own, "padding": 2}}).to_string(),
    )
    .unwrap();

    fixture
        .cli_without_path(&["statusline", "connect", "--yes"])
        .assert()
        .success()
        .stdout(contains("This changes one setting in"))
        .stdout(contains("Your status line keeps showing"));
    assert_eq!(
        fixture.settings()["statusLine"],
        json!({"type": "command", "command": format!("{} statusline --then {own}", program()), "padding": 2})
    );

    fixture
        .cli_without_path(&["statusline", "disconnect", "--yes"])
        .assert()
        .success();
    assert_eq!(
        fixture.settings()["statusLine"],
        json!({"type": "command", "command": own, "padding": 2})
    );
}

#[test]
fn connecting_refuses_a_command_with_shell_operators() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root()).unwrap();
    let settings = json!({"statusLine": {"type": "command", "command": "cat | jq ."}}).to_string();
    fs::write(fixture.root().join("settings.json"), &settings).unwrap();

    fixture
        .cli_without_path(&["statusline", "connect", "--yes"])
        .assert()
        .failure()
        .stderr(contains("cannot wrap safely"));
    assert_eq!(
        fs::read_to_string(fixture.root().join("settings.json")).unwrap(),
        settings
    );
}

#[test]
fn connecting_asks_before_changing_anything() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root()).unwrap();

    fixture
        .cli(&["statusline", "connect"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(contains("Nothing was changed"));
    assert!(!fixture.root().join("settings.json").exists());
}

fn claude_usage(path: &Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE sessions (session_id TEXT PRIMARY KEY, project_name TEXT, topic TEXT);
         CREATE TABLE turns (
             id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT, timestamp TEXT, model TEXT,
             input_tokens INTEGER, output_tokens INTEGER, cache_read_tokens INTEGER,
             cache_creation_tokens INTEGER, cwd TEXT, message_id TEXT, is_subagent INTEGER,
             agent_id TEXT
         );
         INSERT INTO turns (session_id, timestamp, model, input_tokens, output_tokens,
             cache_read_tokens, cache_creation_tokens, cwd, message_id, is_subagent)
         VALUES ('old', '2026-08-01T10:00:00.000Z', 'claude-opus-5', 4, 60, 3000, 90,
             'C:\\code\\app', 'msg_old', 0);",
    )
    .unwrap();
}

#[test]
fn importing_claude_usage_adds_what_the_transcripts_no_longer_hold() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root().join("projects")).unwrap();
    claude_usage(&fixture.root().join("usage.db"));

    fixture
        .cli(&["doctor"])
        .assert()
        .stdout(contains("! Old history    claude-usage's database at"));

    fixture
        .cli(&["import", "claude-usage"])
        .assert()
        .success()
        .stdout(contains("Imported 1 request from"))
        .stdout(contains("the history already held 0."));

    fixture.cli(&["doctor"]).assert().stdout(contains(
        "✓ Old history    1 requests imported from claude-usage on",
    ));
}

#[test]
fn json_is_written_without_colour_when_piped() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root().join("projects")).unwrap();
    fixture
        .cli(&["today", "--json"])
        .env_remove("NO_COLOR")
        .assert()
        .success()
        .stdout(predicate::str::contains('\u{1b}').not());
}

#[test]
fn models_sessions_and_projects_break_the_days_down() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[
            json!({"type": "ai-title", "aiTitle": "Fix the importer"}),
            request("1", "claude-opus-5-5", "2026-10-05T10:00:00Z"),
            request("2", "claude-sonnet-5-5", "2026-10-05T11:00:00Z"),
        ],
    );
    let range = ["--since", "2026-10-01", "--until", "2026-10-07"];
    let json = |command: &str| -> Value {
        let mut args = vec![command];
        args.extend(range);
        args.push("--json");
        let output = fixture.cli(&args).assert().success();
        serde_json::from_slice(&output.get_output().stdout).unwrap()
    };

    let models = json("models");
    assert_eq!(models[0]["model"], "claude-opus-5-5");
    assert_eq!(models[0]["name"], "Claude Opus 5.5");
    assert_eq!(models[1]["model"], "claude-sonnet-5-5");

    let sessions = json("sessions");
    assert_eq!(sessions[0]["session_id"], "s1");
    assert_eq!(sessions[0]["title"], "Fix the importer");
    assert_eq!(sessions[0]["project"], "app");
    assert_eq!(sessions[0]["requests"], 2);

    let projects = json("projects");
    assert_eq!(projects[0]["name"], "app");
    assert_eq!(projects[0]["transcript_folder"], "C--code-app");
    assert_eq!(projects[0]["sessions"], 1);

    let mut args = vec!["sessions"];
    args.extend(range);
    fixture
        .cli(&args)
        .assert()
        .success()
        .stdout(contains("Fix the importer · app"));
}

/// A claude.ai login in the fixture's Claude Code folder, expiring at `expires_ms`.
fn sign_in(fixture: &Fixture, expires_ms: i64) {
    fs::create_dir_all(fixture.root()).unwrap();
    fs::write(
        fixture.root().join(".credentials.json"),
        format!(
            r#"{{"claudeAiOauth": {{"accessToken": "made-up-token", "expiresAt": {expires_ms}}}}}"#
        ),
    )
    .unwrap();
}

const LOGIN_EXPIRED: &str = "Claude Code's login on this computer has expired, so the usage endpoint can't be asked. Open Claude Code once to sign in again; claude-dashboard never refreshes the login itself.";

#[test]
fn turning_the_usage_endpoint_on_asks_first() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[request("1", "claude-opus-5-5", &today_at("00:00:01"))],
    );

    fixture
        .cli(&["limits", "endpoint", "on"])
        .assert()
        .failure()
        .stdout(contains("Use Anthropic's usage endpoint?"))
        .stdout(contains(
            "It reads the login Claude Code keeps on this computer and sends it only to api.anthropic.com.",
        ))
        .stdout(contains(
            "Anthropic's terms say Claude Code's login is for Claude Code and Anthropic's own apps, and Anthropic may act on other uses without notice.",
        ))
        .stderr(contains("Nothing was changed."));
    fixture.cli(&["doctor"]).assert().success().stdout(contains(
        "- Limits         usage endpoint off (experimental, opt-in)",
    ));
}

#[test]
fn an_expired_login_is_never_sent_and_the_reports_say_to_sign_in_again() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[request("1", "claude-opus-5-5", &today_at("00:00:01"))],
    );
    sign_in(&fixture, 1_000);

    fixture
        .cli(&["limits", "endpoint", "on", "--yes"])
        .assert()
        .success()
        .stdout(contains(
            "The usage endpoint is on. Turn it off with claude-dashboard limits endpoint off.",
        ))
        .stdout(contains(LOGIN_EXPIRED));
    fixture
        .cli(&["limits", "endpoint", "on", "--yes"])
        .assert()
        .success()
        .stdout(contains("The usage endpoint is already on."));
    fixture.cli(&["doctor"]).assert().success().stdout(contains(
        "! Limits         usage endpoint on (experimental): Claude Code's login on this computer has expired; open Claude Code once to sign in again",
    ));
    fixture
        .cli(&["today"])
        .assert()
        .success()
        .stdout(contains(LOGIN_EXPIRED));

    fixture
        .cli(&["limits", "endpoint", "off"])
        .assert()
        .success()
        .stdout(contains("The usage endpoint is off."));
    fixture
        .cli(&["today"])
        .assert()
        .success()
        .stdout(contains(LOGIN_EXPIRED).not());
}

#[test]
fn without_a_claude_ai_login_the_usage_endpoint_says_so() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[request("1", "claude-opus-5-5", &today_at("00:00:01"))],
    );

    fixture
        .cli(&["limits", "endpoint", "on", "--yes"])
        .assert()
        .success()
        .stdout(contains(
            "Claude Code has no claude.ai login on this computer.",
        ));
}

#[test]
fn app_says_where_to_get_the_desktop_app_until_it_has_run() {
    let fixture = Fixture::new();
    fixture
        .cli(&["app", "limits"])
        .assert()
        .failure()
        .stderr(contains(
            "The desktop app has not run on this computer yet. Install it from https://github.com/philll11/claude-dashboard/releases/latest",
        ));
    assert!(!fixture.history().exists(), "asking creates no history");
}

#[test]
fn without_a_command_or_a_terminal_it_prints_its_help() {
    let fixture = Fixture::new();
    fixture
        .cli(&[])
        .assert()
        .code(2)
        .stderr(contains("Usage: claude-dashboard"))
        .stderr(contains("weekly"));
}

#[test]
fn weekly_and_monthly_add_whole_weeks_and_months_up() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[
            request("1", "claude-opus-5-5", "2026-09-30T10:00:00Z"),
            request("2", "claude-opus-5-5", "2026-10-05T10:00:00Z"),
            request("3", "claude-sonnet-5-5", "2026-10-07T10:00:00Z"),
            request("4", "claude-opus-5-5", TODAY_NOON),
        ],
    );

    let weeks = fixture.cli(&["weekly", "--json"]).assert().success();
    let weeks: Value = serde_json::from_slice(&weeks.get_output().stdout).unwrap();
    let firsts: Vec<&str> = weeks
        .as_array()
        .unwrap()
        .iter()
        .map(|week| week["first_day"].as_str().unwrap())
        .collect();
    assert_eq!(firsts, ["2026-09-28", "2026-10-05"]);
    assert_eq!(weeks[1]["last_day"], "2026-10-11");
    assert_eq!(weeks[1]["requests"], 3);
    assert_eq!(
        weeks[1]["models"],
        json!(["claude-opus-5-5", "claude-sonnet-5-5"])
    );

    // A day in a week brings in the whole week.
    fixture
        .cli(&[
            "weekly",
            "--since",
            "2026-10-07",
            "--until",
            "2026-10-07",
            "--csv",
        ])
        .assert()
        .success()
        .stdout(contains("first_day,last_day,requests"))
        .stdout(contains("2026-10-05,2026-10-11,3,"));

    fixture
        .cli(&["weekly"])
        .assert()
        .success()
        .stdout(contains("Weeks from Monday · API-equivalent estimate"))
        .stdout(contains("Week of"))
        .stdout(contains("5 Oct 2026"));

    let months = fixture.cli(&["monthly", "--json"]).assert().success();
    let months: Value = serde_json::from_slice(&months.get_output().stdout).unwrap();
    assert_eq!(months.as_array().unwrap().len(), 2);
    assert_eq!(months[0]["first_day"], "2026-09-01");
    assert_eq!(months[0]["last_day"], "2026-09-30");
    assert_eq!(months[1]["requests"], 3);
    fixture
        .cli(&["monthly"])
        .assert()
        .success()
        .stdout(contains("September 2026"))
        .stdout(contains("October 2026"));
}

#[test]
fn limits_shows_each_window_and_the_recent_five_hour_windows() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[request("1", "claude-opus-5-5", &today_at("10:30:00"))],
    );
    fixture.cli(&["limits"]).assert().success().stdout(contains(
        "Plan limits: not recorded yet. To record them, run claude-dashboard statusline connect",
    ));

    let resets = NOW.parse::<jiff::Timestamp>().unwrap().as_second() + 2 * 3600;
    fixture
        .cli(&["statusline"])
        .write_stdin(
            json!({"model": {"id": "claude-opus-5-5", "display_name": "Opus"},
                   "rate_limits": {"five_hour": {"used_percentage": 42.0, "resets_at": resets},
                                   "seven_day": {"used_percentage": 92.0, "resets_at": resets + 86_400}}})
            .to_string(),
        )
        .assert()
        .success();

    fixture
        .cli(&["limits"])
        .assert()
        .success()
        .stdout(contains(
            "Window               Used     Resets                 Source     Reading",
        ))
        .stdout(contains(
            "Five-hour window      42%     14:00, in 2 h 00 min   official   just now",
        ))
        .stdout(contains(
            "Weekly, all models    92% ▲   Sun 11 Oct, 14:00      official   just now",
        ))
        .stdout(contains(
            "Sat 10 Oct 09:00–14:00, in progress       42%                 $0.03",
        ));

    let limits = fixture.cli(&["limits", "--json"]).assert().success();
    let limits: Value = serde_json::from_slice(&limits.get_output().stdout).unwrap();
    assert_eq!(limits["windows"][0]["window"], "five_hour");
    assert_eq!(limits["windows"][0]["used_percentage"], 42.0);
    assert_eq!(limits["windows"][0]["provenance"], "official");
    assert_eq!(limits["windows"][1]["window"], "seven_day");
    assert_eq!(limits["recent_five_hour_windows"][0]["requests"], 1);
    assert_eq!(limits["recent_five_hour_windows"][0]["cost"]["usd"], 0.03);

    fixture
        .cli(&["limits", "--csv"])
        .assert()
        .success()
        .stdout(contains("window,model,used_percentage,resets_at"))
        .stdout(contains("five_hour,,42,"));

    // The usage endpoint's switch still works beside the report.
    fixture
        .cli(&["limits", "endpoint", "off"])
        .assert()
        .success()
        .stdout(contains("The usage endpoint is already off."));
}

/// Makes the fixture's history one that a newer claude-dashboard has upgraded to schema 999,
/// which this version can still read when `readable` is set, and cannot otherwise.
fn upgraded_by_a_newer_version(fixture: &Fixture, readable: bool) {
    fixture.cli(&["today"]).assert().success();
    let conn = rusqlite::Connection::open(fixture.history()).unwrap();
    conn.execute(
        "UPDATE compatibility SET readable_from = ?1",
        [if readable { 1 } else { 999 }],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 999).unwrap();
}

#[test]
fn reports_read_a_history_a_newer_version_upgraded_and_say_to_update() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[request("1", "claude-opus-5-5", &today_at("00:00:01"))],
    );
    upgraded_by_a_newer_version(&fixture, true);

    fixture
        .cli(&["today"])
        .assert()
        .success()
        .stdout(contains("Opus 5.5"))
        .stderr(contains(
            "a newer claude-dashboard has upgraded the history (schema 999); update claude-dashboard to keep recording",
        ));
}

#[test]
fn reports_on_a_history_too_new_to_read_say_to_update_and_fail() {
    let fixture = Fixture::new();
    upgraded_by_a_newer_version(&fixture, false);

    fixture
        .cli(&["daily", "--json"])
        .assert()
        .failure()
        .stdout("")
        .stderr(contains(
            "a newer claude-dashboard has upgraded the history (schema 999); update claude-dashboard to read it",
        ));
}

#[test]
fn doctor_names_a_history_a_newer_version_upgraded_once() {
    let fixture = Fixture::new();
    fixture.transcript(
        "s1",
        &[request("1", "claude-opus-5-5", &today_at("00:00:01"))],
    );
    upgraded_by_a_newer_version(&fixture, true);

    let output = fixture.cli(&["doctor"]).assert().failure();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout).into_owned();
    let history: Vec<&str> = stdout
        .lines()
        .filter(|line| line.contains("History"))
        .collect();
    assert_eq!(history.len(), 1, "{stdout}");
    assert!(
        history[0].starts_with('✗')
            && history[0].contains(
                "upgraded by a newer claude-dashboard (schema 999): update claude-dashboard to keep recording"
            ),
        "{stdout}"
    );
}

#[test]
fn the_status_line_prints_its_line_whatever_the_history() {
    for readable in [true, false] {
        let fixture = Fixture::new();
        upgraded_by_a_newer_version(&fixture, readable);
        fixture
            .cli(&["statusline"])
            .write_stdin(status_line_input(&fixture))
            .assert()
            .success()
            .stdout(contains("5h 42%"));
    }
}
