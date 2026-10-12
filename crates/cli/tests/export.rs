//! The exports as a person or a script receives them: what every report writes with `--json` or
//! `--csv`, and what `export` writes to standard output or to a file saved for a spreadsheet.
//!
//! The files in `tests/goldens` hold the exact bytes. After a change to the output that is meant,
//! run the tests with `UPDATE_GOLDENS=1` and read the goldens' diff.

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::mem;
use std::path::Path;

use predicates::str::contains;
use serde_json::{Value, json};

use common::{Fixture, request};

const QUOTES: &str = "Fix \"quotes\", commas";
const FORMULA: &str = "=HYPERLINK(\"http://example.com\")";
const LINES: &str = "Line one\r\nline two";
const CAFE: &str = "Café ☕ tests";

/// The formula title as a saved CSV holds it: the apostrophe that keeps it from being read as a
/// formula comes first inside the quotes that its own quotes call for.
const FORMULA_SAVED: &str = r#""'=HYPERLINK(""http://example.com"")""#;

const BYTE_ORDER_MARK: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// The days the dated reports are asked for, and the wider range the breakdowns are.
const TWO_DAYS: [&str; 4] = ["--since", "2026-10-05", "--until", "2026-10-06"];
const FIRST_WEEK: [&str; 4] = ["--since", "2026-10-01", "--until", "2026-10-07"];

const REPORTS: [&str; 6] = [
    "daily", "weekly", "monthly", "models", "sessions", "projects",
];

/// Four sessions of the project `C:\code\app` on 5 and 6 October, one for each way that a title
/// can need care in CSV, with seven requests between them: one on a model the price table does
/// not know, and one from a subagent.
fn fixture() -> Fixture {
    let fixture = Fixture::new();
    let title = |text: &str| json!({"type": "ai-title", "aiTitle": text});
    fixture.transcript(
        "session-quotes",
        &[
            title(QUOTES),
            detailed(
                "1",
                "claude-opus-5-5",
                "2026-10-05T09:00:00Z",
                json!({"entrypoint": "cli", "version": "2.1.296", "attributionSkill": "research"}),
                json!({"speed": "fast"}),
            ),
            detailed(
                "2",
                "claude-sonnet-5-5",
                "2026-10-05T09:30:00Z",
                json!({"entrypoint": "cli", "version": "2.1.296"}),
                json!({
                    "inference_geo": "us",
                    "server_tool_use": {"web_search_requests": 2, "web_fetch_requests": 1}
                }),
            ),
            json!({
                "type": "cost-state",
                "startTime": 1_791_190_800_000_i64,
                "modelUsage": {"claude-opus-5-5": {
                    "costUSD": 0.0987, "inputTokens": 3, "outputTokens": 640,
                    "cacheReadInputTokens": 42_000, "cacheCreationInputTokens": 1_500
                }}
            }),
        ],
    );
    fixture.transcript(
        "session-formula",
        &[
            title(FORMULA),
            detailed(
                "3",
                "claude-opus-5-5",
                "2026-10-05T14:00:00Z",
                json!({
                    "entrypoint": "claude-vscode", "version": "2.1.300",
                    "attributionMcpServer": "github", "attributionMcpTool": "search_code"
                }),
                json!({}),
            ),
        ],
    );
    fixture.transcript(
        "session-lines",
        &[
            title(LINES),
            request("4", "claude-sonnet-5-5", "2026-10-06T08:00:00Z"),
            request("5", "claude-orion-1", "2026-10-06T08:15:00Z"),
        ],
    );
    fixture.transcript(
        "session-cafe",
        &[
            title(CAFE),
            detailed(
                "6",
                "claude-opus-5-5",
                "2026-10-06T16:00:00Z",
                json!({"entrypoint": "cli", "version": "2.1.296"}),
                json!({"inference_geo": "us"}),
            ),
        ],
    );
    let subagents = fixture
        .root()
        .join("projects")
        .join("C--code-app")
        .join("session-cafe")
        .join("subagents");
    fs::create_dir_all(&subagents).unwrap();
    fs::write(
        subagents.join("agent-a1.jsonl"),
        request("7", "claude-sonnet-5-5", "2026-10-06T16:05:00Z").to_string() + "\n",
    )
    .unwrap();
    fs::write(
        subagents.join("agent-a1.meta.json"),
        json!({"agentType": "Explore"}).to_string(),
    )
    .unwrap();
    fixture
}

/// A request with more of what Claude Code records: `record` sets fields of the transcript record
/// and `usage` fields of the message's usage.
fn detailed(id: &str, model: &str, timestamp: &str, record: Value, usage: Value) -> Value {
    fn merge(into: &mut Value, from: Value) {
        match from {
            Value::Object(fields) => into.as_object_mut().unwrap().extend(fields),
            other => panic!("{other} is not an object"),
        }
    }
    let mut request = request(id, model, timestamp);
    merge(&mut request, record);
    merge(&mut request["message"]["usage"], usage);
    request
}

/// Compares `actual` byte for byte with the golden file `name`; with `UPDATE_GOLDENS` set, writes
/// the golden instead.
fn golden(name: &str, actual: &[u8]) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("goldens")
        .join(name);
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "cannot read the golden {}: {error}\nrun the tests with UPDATE_GOLDENS=1 to write it",
            path.display()
        )
    });
    if expected != actual {
        let at = expected
            .iter()
            .zip(actual)
            .position(|(golden, actual)| golden != actual)
            .unwrap_or_else(|| expected.len().min(actual.len()));
        panic!(
            "{name} differs from its golden, first at byte {at}\n\
             golden: {}\nactual: {}\n\n--- golden ---\n{}\n--- actual ---\n{}",
            around(&expected, at),
            around(actual, at),
            String::from_utf8_lossy(&expected),
            String::from_utf8_lossy(actual),
        );
    }
}

/// The bytes around `at`, with the ones a terminal does not show written out.
fn around(bytes: &[u8], at: usize) -> String {
    let from = at.saturating_sub(24);
    let to = (at + 24).min(bytes.len());
    format!("{:?}", String::from_utf8_lossy(&bytes[from..to]))
}

/// What `args` write to standard output, when the command succeeds.
fn stdout(fixture: &Fixture, args: &[&str]) -> Vec<u8> {
    fixture
        .cli(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone()
}

/// [`stdout`] over a fixture of its own.
fn stdout_of(args: &[&str]) -> Vec<u8> {
    stdout(&fixture(), args)
}

fn text(fixture: &Fixture, args: &[&str]) -> String {
    String::from_utf8(stdout(fixture, args)).unwrap()
}

/// The arguments of a report: `command` for the days of `range`, as `format` (`--csv` or
/// `--json`).
fn report<'a>(command: &'a str, range: [&'a str; 4], format: &'a str) -> Vec<&'a str> {
    let mut args = vec![command];
    args.extend(range);
    args.push(format);
    args
}

/// The arguments of `export` as `format` for the `days` given.
fn export<'a>(format: &'a str, days: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec!["export", format];
    args.extend(days);
    args
}

fn json_of(fixture: &Fixture, command: &str, range: [&str; 4]) -> Value {
    serde_json::from_slice(&stdout(fixture, &report(command, range, "--json"))).unwrap()
}

/// What `export --output` writes to a file, over a fixture of its own.
fn saved(format: &str) -> Vec<u8> {
    let fixture = fixture();
    let path = fixture.dir.path().join("requests");
    fixture
        .cli(&["export", format, "--output", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("Saved 7 requests to"));
    fs::read(path).unwrap()
}

/// CSV as RFC 4180 has it, read strictly: a quote only opens a field or closes a quoted one, and
/// every record ends with a line feed.
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    #[derive(Debug, Clone, Copy)]
    enum State {
        Start,
        Plain,
        Quoted,
        Closing,
    }
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut state = State::Start;
    for c in text.chars() {
        state = match (state, c) {
            (State::Start, '"') => State::Quoted,
            (State::Start | State::Plain | State::Closing, ',') => {
                record.push(mem::take(&mut field));
                State::Start
            }
            (State::Start | State::Plain | State::Closing, '\n') => {
                record.push(mem::take(&mut field));
                records.push(mem::take(&mut record));
                State::Start
            }
            (State::Start | State::Plain, c) if c != '"' => {
                field.push(c);
                State::Plain
            }
            (State::Quoted, '"') => State::Closing,
            (State::Quoted, c) => {
                field.push(c);
                State::Quoted
            }
            (State::Closing, '"') => {
                field.push('"');
                State::Quoted
            }
            (state, c) => panic!("{c:?} where the CSV is {state:?}: {field:?}"),
        };
    }
    assert!(
        matches!(state, State::Start) && field.is_empty() && record.is_empty(),
        "the CSV ends in the middle of a record: {field:?}"
    );
    records
}

/// The values of the column `name`, one for each record below the header.
fn column<'a>(records: &'a [Vec<String>], name: &str) -> Vec<&'a str> {
    let index = records[0]
        .iter()
        .position(|header| header == name)
        .unwrap_or_else(|| panic!("no column {name} in {:?}", records[0]));
    records[1..]
        .iter()
        .map(|record| record[index].as_str())
        .collect()
}

#[test]
fn daily_csv_matches_its_golden() {
    golden("daily.csv", &stdout_of(&report("daily", TWO_DAYS, "--csv")));
}

#[test]
fn daily_json_matches_its_golden() {
    golden(
        "daily.json",
        &stdout_of(&report("daily", TWO_DAYS, "--json")),
    );
}

#[test]
fn weekly_csv_matches_its_golden() {
    golden(
        "weekly.csv",
        &stdout_of(&report("weekly", TWO_DAYS, "--csv")),
    );
}

#[test]
fn weekly_json_matches_its_golden() {
    golden(
        "weekly.json",
        &stdout_of(&report("weekly", TWO_DAYS, "--json")),
    );
}

#[test]
fn monthly_csv_matches_its_golden() {
    golden(
        "monthly.csv",
        &stdout_of(&report("monthly", TWO_DAYS, "--csv")),
    );
}

#[test]
fn monthly_json_matches_its_golden() {
    golden(
        "monthly.json",
        &stdout_of(&report("monthly", TWO_DAYS, "--json")),
    );
}

#[test]
fn models_csv_matches_its_golden() {
    golden(
        "models.csv",
        &stdout_of(&report("models", FIRST_WEEK, "--csv")),
    );
}

#[test]
fn models_json_matches_its_golden() {
    golden(
        "models.json",
        &stdout_of(&report("models", FIRST_WEEK, "--json")),
    );
}

#[test]
fn sessions_csv_matches_its_golden() {
    golden(
        "sessions.csv",
        &stdout_of(&report("sessions", FIRST_WEEK, "--csv")),
    );
}

#[test]
fn sessions_json_matches_its_golden() {
    golden(
        "sessions.json",
        &stdout_of(&report("sessions", FIRST_WEEK, "--json")),
    );
}

#[test]
fn projects_csv_matches_its_golden() {
    golden(
        "projects.csv",
        &stdout_of(&report("projects", FIRST_WEEK, "--csv")),
    );
}

#[test]
fn projects_json_matches_its_golden() {
    golden(
        "projects.json",
        &stdout_of(&report("projects", FIRST_WEEK, "--json")),
    );
}

#[test]
fn export_csv_matches_its_golden() {
    golden("export.csv", &stdout_of(&export("--csv", &[])));
}

#[test]
fn export_json_matches_its_golden() {
    golden("export.json", &stdout_of(&export("--json", &[])));
}

#[test]
fn a_saved_csv_matches_its_golden() {
    golden("export-saved.csv", &saved("--csv"));
}

#[test]
fn a_saved_json_matches_its_golden() {
    golden("export-saved.json", &saved("--json"));
}

#[test]
fn export_without_a_format_writes_csv() {
    let fixture = fixture();
    assert_eq!(
        stdout(&fixture, &["export"]),
        stdout(&fixture, &["export", "--csv"])
    );
}

#[test]
fn a_saved_csv_starts_with_a_byte_order_mark_and_a_saved_json_does_not() {
    let csv = saved("--csv");
    assert!(csv.starts_with(&BYTE_ORDER_MARK));
    assert!(
        csv[BYTE_ORDER_MARK.len()..].starts_with(b"at,session_id,title,"),
        "the header follows the mark"
    );

    let json = saved("--json");
    assert!(!json.starts_with(&BYTE_ORDER_MARK));
    assert!(json.starts_with(b"[\n{"));
    assert_eq!(json, stdout_of(&export("--json", &[])));
}

#[test]
fn a_saved_csv_guards_a_formula_title_inside_its_quotes() {
    let csv = String::from_utf8(saved("--csv")).unwrap();
    let csv = csv.strip_prefix('\u{feff}').unwrap();

    assert!(csv.contains(FORMULA_SAVED), "{csv}");
    assert_eq!(
        csv.matches('\'').count(),
        1,
        "only the formula title is guarded"
    );
    let records = parse_csv(csv);
    let titles: BTreeSet<&str> = column(&records, "title").into_iter().collect();
    assert_eq!(
        titles,
        BTreeSet::from([QUOTES, "'=HYPERLINK(\"http://example.com\")", LINES, CAFE])
    );
}

#[test]
fn standard_output_csv_has_no_byte_order_mark_and_no_apostrophe() {
    let fixture = fixture();
    let csv = stdout(&fixture, &export("--csv", &[]));
    assert!(csv.starts_with(b"at,session_id,title,"));
    assert!(!csv.contains(&b'\''));
    let csv = String::from_utf8(csv).unwrap();
    assert!(
        csv.contains(r#""=HYPERLINK(""http://example.com"")""#),
        "{csv}"
    );

    for command in REPORTS {
        let csv = stdout(&fixture, &report(command, FIRST_WEEK, "--csv"));
        assert!(!csv.starts_with(&BYTE_ORDER_MARK), "{command}");
        assert!(!csv.contains(&b'\''), "{command}");
    }
}

#[test]
fn csv_fields_with_commas_quotes_and_line_breaks_are_read_back_whole() {
    let fixture = fixture();
    let sessions = parse_csv(&text(&fixture, &report("sessions", FIRST_WEEK, "--csv")));
    assert_eq!(
        column(&sessions, "title"),
        [CAFE, LINES, FORMULA, QUOTES],
        "the session started last comes first"
    );
    assert_eq!(
        column(&sessions, "session_id"),
        [
            "session-cafe",
            "session-lines",
            "session-formula",
            "session-quotes"
        ]
    );

    let requests = parse_csv(&text(&fixture, &export("--csv", &[])));
    let titles: BTreeSet<&str> = column(&requests, "title").into_iter().collect();
    assert_eq!(titles, BTreeSet::from([QUOTES, FORMULA, LINES, CAFE]));
}

#[test]
fn every_csv_record_has_as_many_fields_as_its_header() {
    let fixture = fixture();
    let mut outputs = vec![text(&fixture, &export("--csv", &[]))];
    for command in REPORTS {
        outputs.push(text(&fixture, &report(command, FIRST_WEEK, "--csv")));
    }
    for output in outputs {
        let records = parse_csv(&output);
        let header = records[0].len();
        assert!(records.len() > 1, "{}", records[0].join(","));
        for record in &records {
            assert_eq!(record.len(), header, "{record:?}\nunder {:?}", records[0]);
        }
    }
}

#[test]
fn an_unpriced_request_has_no_cost_in_csv_and_null_in_json() {
    let fixture = fixture();
    let csv = parse_csv(&text(&fixture, &export("--csv", &[])));
    let costs: Vec<(&str, &str)> = column(&csv, "model")
        .into_iter()
        .zip(column(&csv, "cost_usd"))
        .collect();
    assert_eq!(costs.len(), 7);
    for (model, cost) in costs {
        if model == "claude-orion-1" {
            assert_eq!(cost, "", "an unpriced model has no cost");
        } else {
            assert!(cost.parse::<f64>().unwrap() > 0.0, "{model}: {cost}");
        }
    }

    let requests: Value = serde_json::from_str(&text(&fixture, &export("--json", &[]))).unwrap();
    for request in requests.as_array().unwrap() {
        let usd = &request["cost"]["usd"];
        if request["model"] == "claude-orion-1" {
            assert!(usd.is_null(), "{request}");
        } else {
            assert!(usd.as_f64().unwrap() > 0.0, "{request}");
        }
        assert_eq!(request["cost"]["provenance"], "estimate");
    }
}

#[test]
fn a_requests_cost_is_not_rounded_to_the_cent() {
    let fixture = fixture();
    let requests: Value = serde_json::from_str(&text(&fixture, &export("--json", &[]))).unwrap();
    let opus = requests
        .as_array()
        .unwrap()
        .iter()
        .find(|request| request["session_id"] == "session-formula")
        .unwrap();
    let usd = opus["cost"]["usd"].as_f64().unwrap();
    assert!((usd - 0.031_712).abs() < 1e-9, "{usd}");

    let csv = parse_csv(&text(&fixture, &export("--csv", &[])));
    let costs = column(&csv, "cost_usd");
    assert!(
        costs.iter().any(|cost| cost.starts_with("0.031712")),
        "{costs:?}"
    );
}

#[test]
fn export_json_is_one_object_per_line() {
    let json = String::from_utf8(stdout_of(&export("--json", &[]))).unwrap();
    let lines: Vec<&str> = json.lines().collect();
    assert_eq!(
        lines.len(),
        7 + 2,
        "a line for each request, and the brackets"
    );
    assert_eq!(lines[0], "[");
    assert_eq!(lines[lines.len() - 1], "]");
    assert!(json.ends_with("}\n]\n"));
    let objects = &lines[1..lines.len() - 1];
    for (index, line) in objects.iter().enumerate() {
        let separator = if index + 1 < objects.len() { "," } else { "" };
        let object = line.strip_suffix(separator).unwrap();
        let request: Value = serde_json::from_str(object).unwrap();
        assert!(request.is_object(), "{line}");
    }

    let requests: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(requests.as_array().unwrap().len(), 7);
    let first = &requests[0];
    assert_eq!(first["at"], "2026-10-05T09:00:00Z");
    assert_eq!(first["session_id"], "session-quotes");
    assert_eq!(first["title"], QUOTES);
    assert_eq!(first["project"], "app");
    assert_eq!(first["model"], "claude-opus-5-5");
    assert_eq!(first["speed"], "fast");
    assert_eq!(first["skill"], "research");
    assert_eq!(first["tokens"]["cache_write_1h"], 1000);
    assert!(first["subagent_id"].is_null());
    assert!(first["mcp_server"].is_null());
    let subagent = &requests[6];
    assert_eq!(subagent["subagent_id"], "a1");
    assert_eq!(subagent["agent_type"], "Explore");
}

#[test]
fn the_report_json_nests_tokens_and_cost_and_adds_up_to_the_same_requests() {
    let fixture = fixture();
    for command in REPORTS {
        let range = if matches!(command, "daily" | "weekly" | "monthly") {
            TWO_DAYS
        } else {
            FIRST_WEEK
        };
        let rows = json_of(&fixture, command, range);
        let rows = rows.as_array().unwrap();
        let requests: u64 = rows
            .iter()
            .map(|row| row["requests"].as_u64().unwrap())
            .sum();
        assert_eq!(requests, 7, "{command}");
        for row in rows {
            let tokens = row["tokens"].as_object().unwrap();
            let kinds: Vec<&str> = tokens.keys().map(String::as_str).collect();
            assert_eq!(
                kinds,
                [
                    "input",
                    "output",
                    "cache_write_5m",
                    "cache_write_1h",
                    "cache_read"
                ],
                "{command}"
            );
            assert!(tokens.values().all(Value::is_u64), "{command}: {row}");
            let cost = row["cost"].as_object().unwrap();
            let fields: Vec<&str> = cost.keys().map(String::as_str).collect();
            assert_eq!(
                fields,
                ["usd", "provenance", "price_table", "unpriced_requests"],
                "{command}"
            );
            assert!(cost["usd"].is_number(), "{command}: {row}");
            assert_eq!(cost["provenance"], "estimate");
            assert_eq!(cost["price_table"], "2026-10-10");
        }
    }
}

#[test]
fn models_are_listed_by_id_and_the_unpriced_one_has_no_name() {
    let fixture = fixture();
    let models = json_of(&fixture, "models", FIRST_WEEK);
    let models = models.as_array().unwrap();
    let ids: Vec<&str> = models
        .iter()
        .map(|model| model["model"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["claude-opus-5-5", "claude-sonnet-5-5", "claude-orion-1"],
        "highest cost first"
    );
    assert_eq!(models[0]["name"], "Claude Opus 5.5");
    assert_eq!(models[1]["name"], "Claude Sonnet 5.5");
    assert!(models[2]["name"].is_null(), "{}", models[2]);
    assert_eq!(models[2]["cost"]["unpriced_requests"], 1);
    assert_eq!(models[2]["cost"]["usd"], 0.0);
}

#[test]
fn export_limits_the_requests_to_the_days_given() {
    let fixture = fixture();
    let days = |given: &[&str]| -> Vec<String> {
        let requests: Value =
            serde_json::from_slice(&stdout(&fixture, &export("--json", given))).unwrap();
        requests
            .as_array()
            .unwrap()
            .iter()
            .map(|request| request["at"].as_str().unwrap()[..10].to_owned())
            .collect()
    };

    assert_eq!(days(&["--since", "2026-10-06"]), ["2026-10-06"; 4]);
    assert_eq!(days(&["--until", "2026-10-05"]), ["2026-10-05"; 3]);
    assert_eq!(
        days(&["--since", "2026-10-06", "--until", "2026-10-06"]),
        ["2026-10-06"; 4]
    );
    assert_eq!(days(&TWO_DAYS).len(), 7);
}

#[test]
fn export_of_days_without_requests_is_an_empty_array_or_a_header_alone() {
    let fixture = fixture();
    let quiet_days = ["--since", "2026-10-07", "--until", "2026-10-09"];

    assert_eq!(stdout(&fixture, &export("--json", &quiet_days)), b"[]\n");
    let csv = text(&fixture, &export("--csv", &quiet_days));
    assert!(csv.starts_with("at,session_id,title,"), "{csv}");
    assert_eq!(csv.lines().count(), 1, "{csv}");
}

#[test]
fn a_saved_csv_guards_every_character_a_spreadsheet_reads_as_a_formula() {
    let fixture = Fixture::new();
    let skills = [
        "=SUM(A1)",
        "+1",
        "-1",
        "@SUM(A1)",
        "\tcmd",
        "\rcmd",
        "a=b",
        "plain, \"quoted\"",
    ];
    let records: Vec<Value> = skills
        .iter()
        .enumerate()
        .map(|(index, skill)| {
            detailed(
                &index.to_string(),
                "claude-opus-5-5",
                &format!("2026-10-05T09:0{index}:00Z"),
                json!({"attributionSkill": skill}),
                json!({}),
            )
        })
        .collect();
    fixture.transcript("guarded", &records);
    let path = fixture.dir.path().join("guarded.csv");

    let on_screen = text(&fixture, &export("--csv", &[]));
    assert_eq!(column(&parse_csv(&on_screen), "skill"), skills);

    fixture
        .cli(&export("--csv", &["--output", path.to_str().unwrap()]))
        .assert()
        .success();
    let file = fs::read_to_string(path).unwrap();
    let file = file.strip_prefix('\u{feff}').unwrap();
    assert_eq!(
        column(&parse_csv(file), "skill"),
        [
            "'=SUM(A1)",
            "'+1",
            "'-1",
            "'@SUM(A1)",
            "'\tcmd",
            "'\rcmd",
            "a=b",
            "plain, \"quoted\"",
        ]
    );
}

#[test]
fn exporting_to_a_folder_that_is_not_there_fails_and_says_so() {
    let fixture = fixture();
    let folder = fixture.dir.path().join("missing");
    let path = folder.join("requests.csv");

    fixture
        .cli(&export("--csv", &["--output", path.to_str().unwrap()]))
        .assert()
        .failure()
        .stderr(contains("Cannot write to"))
        .stdout("");
    assert!(!folder.exists());
}

/// The names of the temporary files that an export writes in `folder` before it puts the file in
/// place.
fn part_files(folder: &Path) -> Vec<String> {
    fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| {
            name.starts_with(".claude-dashboard-")
                && Path::new(name)
                    .extension()
                    .is_some_and(|extension| extension == "part")
        })
        .collect()
}

#[test]
fn a_bare_file_name_is_saved_in_the_current_folder() {
    let fixture = fixture();

    fixture
        .cli(&export("--csv", &["--output", "requests.csv"]))
        .current_dir(fixture.dir.path())
        .assert()
        .success()
        .stdout("Saved 7 requests to requests.csv\n");

    let file = fs::read(fixture.dir.path().join("requests.csv")).unwrap();
    assert!(file.starts_with(&BYTE_ORDER_MARK));
    assert_eq!(part_files(fixture.dir.path()), Vec::<String>::new());
}

#[test]
fn saving_over_an_existing_file_replaces_it() {
    let fixture = fixture();
    let path = fixture.dir.path().join("requests.json");
    fs::write(&path, "an earlier export").unwrap();

    fixture
        .cli(&export("--json", &["--output", path.to_str().unwrap()]))
        .assert()
        .success();

    assert_eq!(
        fs::read(&path).unwrap(),
        stdout(&fixture, &export("--json", &[]))
    );
    assert_eq!(part_files(fixture.dir.path()), Vec::<String>::new());
}
