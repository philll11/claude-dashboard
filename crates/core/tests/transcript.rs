use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, SystemTime};

use claude_dashboard_core::tokens::Tokens;
use claude_dashboard_core::transcript::{
    self, ModelUsage, Record, RequestRecord, parse_line, read_from,
};
use proptest::prelude::*;
use serde_json::{Value, json};

/// An assistant record shaped like the ones Claude Code writes, with made-up content.
fn assistant() -> Value {
    json!({
        "parentUuid": "0b6f4d1c-5f7e-4a51-9d55-6c2f1e0d9a10",
        "isSidechain": false,
        "message": {
            "model": "claude-opus-5-5",
            "id": "msg_01AbCdEf",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "Made-up reply."}],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 3,
                "cache_creation_input_tokens": 1_500,
                "cache_read_input_tokens": 42_000,
                "cache_creation": {"ephemeral_5m_input_tokens": 500, "ephemeral_1h_input_tokens": 1_000},
                "output_tokens": 640,
                "server_tool_use": {"web_search_requests": 2, "web_fetch_requests": 1},
                "service_tier": "standard",
                "inference_geo": "global",
                "speed": "standard",
                "output_tokens_details": {"thinking_tokens": 120},
                "iterations": [{
                    "type": "message",
                    "input_tokens": 3,
                    "output_tokens": 640,
                    "cache_read_input_tokens": 42_000,
                    "cache_creation_input_tokens": 1_500,
                    "cache_creation": {"ephemeral_5m_input_tokens": 500, "ephemeral_1h_input_tokens": 1_000}
                }]
            }
        },
        "requestId": "req_011Xyz",
        "type": "assistant",
        "uuid": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
        "timestamp": "2026-10-10T06:16:27.123Z",
        "userType": "external",
        "entrypoint": "cli",
        "cwd": "C:\\code\\app",
        "sessionId": "4f1c2a3b-0000-4000-8000-000000000001",
        "version": "2.1.296",
        "gitBranch": "main",
        "attributionSkill": "dataviz",
        "attributionMcpServer": "github",
        "attributionMcpTool": "create_pull_request"
    })
}

fn line(record: &Value) -> Vec<u8> {
    let mut line = serde_json::to_vec(record).unwrap();
    line.push(b'\n');
    line
}

fn request(record: &Value) -> RequestRecord {
    match parse_line(&line(record)) {
        Ok(Some(Record::Request(request))) => *request,
        other => panic!("expected a request, got {other:?}"),
    }
}

fn tokens(input: u64, output: u64, cache_read: u64, five: u64, hour: u64) -> Tokens {
    Tokens {
        input,
        output,
        cache_read,
        cache_write_5m: five,
        cache_write_1h: hour,
    }
}

#[test]
fn reads_a_request_from_an_assistant_record() {
    assert_eq!(
        request(&assistant()),
        RequestRecord {
            record_id: "7c9e6679-7425-40de-944b-e07fc1f90ae7".into(),
            message_id: Some("msg_01AbCdEf".into()),
            request_id: Some("req_011Xyz".into()),
            timestamp_ms: 1_791_612_987_123,
            cwd: Some(r"C:\code\app".into()),
            entrypoint: Some("cli".into()),
            claude_code_version: Some("2.1.296".into()),
            speed: Some("standard".into()),
            service_tier: Some("standard".into()),
            inference_geo: Some("global".into()),
            skill: Some("dataviz".into()),
            mcp_server: Some("github".into()),
            mcp_tool: Some("create_pull_request".into()),
            web_search_requests: 2,
            web_fetch_requests: 1,
            usage: vec![ModelUsage {
                model: "claude-opus-5-5".into(),
                tokens: tokens(3, 640, 42_000, 500, 1_000),
            }],
        }
    );
}

#[test]
fn without_the_cache_split_every_cache_write_is_kept_five_minutes() {
    let mut record = assistant();
    let usage_json = &mut record["message"]["usage"];
    usage_json.as_object_mut().unwrap().remove("cache_creation");
    usage_json.as_object_mut().unwrap().remove("iterations");

    assert_eq!(
        request(&record).usage[0].tokens,
        tokens(3, 640, 42_000, 1_500, 0)
    );
}

#[test]
fn a_fallback_counts_each_model_with_the_records_own_model_first() {
    let mut record = assistant();
    record["message"]["model"] = json!("claude-opus-4-8");
    record["message"]["usage"]["iterations"] = json!([
        {"type": "message", "model": "claude-fable-5-1", "input_tokens": 32, "output_tokens": 484,
         "cache_read_input_tokens": 424_503, "cache_creation_input_tokens": 2_448,
         "cache_creation": {"ephemeral_5m_input_tokens": 0, "ephemeral_1h_input_tokens": 2_448}},
        {"type": "fallback_message", "model": "claude-opus-4-8", "input_tokens": 32, "output_tokens": 5_978,
         "cache_read_input_tokens": 336_961, "cache_creation_input_tokens": 0}
    ]);

    assert_eq!(
        request(&record).usage,
        [
            ModelUsage {
                model: "claude-opus-4-8".into(),
                tokens: tokens(32, 5_978, 336_961, 0, 0),
            },
            ModelUsage {
                model: "claude-fable-5-1".into(),
                tokens: tokens(32, 484, 424_503, 0, 2_448),
            },
        ]
    );
}

#[test]
fn a_response_cut_off_mid_stream_counts_what_its_iterations_used() {
    let mut record = assistant();
    let usage_json = &mut record["message"]["usage"];
    for field in [
        "input_tokens",
        "output_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
    ] {
        usage_json[field] = json!(0);
    }
    usage_json["cache_creation"] =
        json!({"ephemeral_5m_input_tokens": 0, "ephemeral_1h_input_tokens": 0});
    usage_json["iterations"] = json!([{
        "type": "message", "input_tokens": 2, "output_tokens": 671,
        "cache_read_input_tokens": 966_536, "cache_creation_input_tokens": 363,
        "cache_creation": {"ephemeral_5m_input_tokens": 0, "ephemeral_1h_input_tokens": 363}
    }]);

    assert_eq!(
        request(&record).usage,
        [ModelUsage {
            model: "claude-opus-5-5".into(),
            tokens: tokens(2, 671, 966_536, 0, 363),
        }]
    );
}

#[test]
fn messages_claude_code_writes_itself_are_not_requests() {
    let mut record = assistant();
    record["message"]["model"] = json!("<synthetic>");
    assert_eq!(parse_line(&line(&record)), Ok(None));
}

#[test]
fn other_records_are_ignored() {
    let mut without_usage = assistant();
    without_usage["message"]
        .as_object_mut()
        .unwrap()
        .remove("usage");
    let others = [
        without_usage,
        json!({"type": "user", "message": {"role": "user", "content": "Made-up prompt."}, "uuid": "u1"}),
        json!({"type": "user", "message": "a message that is not an object"}),
        json!({"type": "attachment", "attachment": {"type": "file"}}),
        json!({"type": "a-record-type-from-a-later-version", "anything": [1, 2, 3]}),
        json!({"no-type": true}),
    ];
    for record in others {
        assert_eq!(parse_line(&line(&record)), Ok(None), "{record}");
    }
}

#[test]
fn reads_session_titles() {
    assert_eq!(
        parse_line(br#"{"type":"ai-title","aiTitle":"Fix the importer","sessionId":"s"}"#),
        Ok(Some(Record::AiTitle("Fix the importer".into())))
    );
    assert_eq!(
        parse_line(br#"{"type":"custom-title","customTitle":"Release prep","sessionId":"s"}"#),
        Ok(Some(Record::CustomTitle("Release prep".into())))
    );
}

#[test]
fn a_field_of_an_unexpected_type_costs_only_that_field() {
    let mut record = assistant();
    record["version"] = json!(2_128);
    record["entrypoint"] = json!({"name": "cli"});
    record["message"]["usage"]["speed"] = json!(["standard"]);
    record["message"]["usage"]["server_tool_use"] = json!("none");
    record["message"]["usage"]["output_tokens"] = json!("640");
    record["message"]["usage"]
        .as_object_mut()
        .unwrap()
        .remove("iterations");

    let request = request(&record);

    assert_eq!(request.claude_code_version, None);
    assert_eq!(request.entrypoint, None);
    assert_eq!(request.speed, None);
    assert_eq!(request.web_search_requests, 0);
    assert_eq!(request.usage[0].tokens, tokens(3, 0, 42_000, 500, 1_000));
    assert_eq!(request.message_id.as_deref(), Some("msg_01AbCdEf"));
}

#[test]
fn lines_that_cannot_be_read_are_reported() {
    use claude_dashboard_core::transcript::Unreadable;

    let mut without_time = assistant();
    without_time.as_object_mut().unwrap().remove("timestamp");
    let mut without_id = assistant();
    without_id.as_object_mut().unwrap().remove("uuid");
    let mut without_model = assistant();
    without_model["message"]
        .as_object_mut()
        .unwrap()
        .remove("model");

    assert_eq!(parse_line(b"not json\n"), Err(Unreadable::NotJson));
    assert_eq!(parse_line(b"[1, 2]\n"), Err(Unreadable::NotJson));
    for record in [without_time, without_id, without_model] {
        assert_eq!(parse_line(&line(&record)), Err(Unreadable::Incomplete));
    }
}

#[test]
fn line_endings_and_blank_lines_are_accepted() {
    let mut crlf = serde_json::to_vec(&assistant()).unwrap();
    crlf.extend_from_slice(b"\r\n");
    assert!(matches!(parse_line(&crlf), Ok(Some(Record::Request(_)))));
    assert_eq!(parse_line(b"\r\n"), Ok(None));
    assert_eq!(parse_line(b"   \n"), Ok(None));
}

#[test]
fn odd_text_in_the_content_does_not_cost_the_request() {
    let text = serde_json::to_string(&assistant()).unwrap();
    let marker = "Made-up reply.";
    let mut invalid_utf8 = text.replace(marker, "Made-up \u{1}").into_bytes();
    let at = invalid_utf8.iter().position(|&byte| byte == 1).unwrap();
    invalid_utf8[at] = 0xFF;
    let lone_surrogate = text.replace(marker, r"half of a pair: \ud83d");

    for line in [invalid_utf8, lone_surrogate.into_bytes()] {
        assert!(
            matches!(parse_line(&line), Ok(Some(Record::Request(_)))),
            "{}",
            String::from_utf8_lossy(&line)
        );
    }
}

fn write(path: &Path, contents: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn age(path: &Path, seconds: u64) {
    let file = OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(seconds))
        .unwrap();
}

#[test]
fn finds_sessions_and_subagents_most_recently_changed_first() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("projects").join("C--code-app");
    let session = project.join("4f1c.jsonl");
    let subagent = project
        .join("4f1c")
        .join("subagents")
        .join("agent-a1.jsonl");
    let workflow = project
        .join("4f1c")
        .join("subagents")
        .join("workflows")
        .join("wf_9")
        .join("agent-b2.jsonl");
    for (path, seconds) in [(&session, 300), (&subagent, 100), (&workflow, 200)] {
        write(path, b"{}\n");
        age(path, seconds);
    }
    write(
        &project
            .join("4f1c")
            .join("subagents")
            .join("workflows")
            .join("wf_9")
            .join("journal.jsonl"),
        b"{}\n",
    );

    let found: Vec<_> = transcript::find(dir.path())
        .into_iter()
        .map(|t| (t.path, t.session_id, t.subagent_id, t.project_folder))
        .collect();

    assert_eq!(
        found,
        [
            (
                subagent,
                "4f1c".into(),
                Some("a1".into()),
                "C--code-app".into()
            ),
            (
                workflow,
                "4f1c".into(),
                Some("b2".into()),
                "C--code-app".into()
            ),
            (session, "4f1c".into(), None, "C--code-app".into()),
        ]
    );
}

#[test]
fn a_root_without_projects_has_no_transcripts() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(transcript::find(dir.path()), []);
}

/// A transcript with three requests and a title among records the history ignores.
fn transcript_bytes() -> Vec<u8> {
    let mut bytes = Vec::new();
    for n in 0..3 {
        let mut record = assistant();
        record["uuid"] = json!(format!("rec-{n}"));
        record["message"]["id"] = json!(format!("msg_{n}"));
        bytes.extend(line(
            &json!({"type": "user", "message": {"role": "user", "content": "Made-up prompt."}}),
        ));
        bytes.extend(line(&record));
    }
    bytes.extend(line(
        &json!({"type": "ai-title", "aiTitle": "Made-up title"}),
    ));
    bytes
}

fn record_ids(records: &[Record]) -> Vec<String> {
    records
        .iter()
        .map(|record| match record {
            Record::Request(request) => request.record_id.clone(),
            Record::AiTitle(title) | Record::CustomTitle(title) => title.clone(),
            Record::CostState(state) => format!("cost-state {}", state.started_ms),
        })
        .collect()
}

#[test]
fn reads_claude_codes_running_cost() {
    use claude_dashboard_core::transcript::{CostState, ReportedModelCost};

    let line = br#"{"type":"cost-state","sessionId":"s","totalCostUSD":1.75,"startTime":1791600000000,
        "totalDuration":5000,"hasUnknownModelCost":false,"modelUsage":{
        "claude-opus-5-5":{"inputTokens":12,"outputTokens":640,"thinkingTokens":100,
            "cacheReadInputTokens":42000,"cacheCreationInputTokens":1500,"webSearchRequests":1,"costUSD":1.5},
        "claude-haiku-4-5-20251001":{"inputTokens":900,"outputTokens":40,"thinkingTokens":0,
            "cacheReadInputTokens":0,"cacheCreationInputTokens":0,"webSearchRequests":0,"costUSD":0.25}}}"#;
    let line: Vec<u8> = line.iter().copied().filter(|&byte| byte != b'\n').collect();

    assert_eq!(
        parse_line(&line),
        Ok(Some(Record::CostState(CostState {
            started_ms: 1_791_600_000_000,
            models: vec![
                ReportedModelCost {
                    model: "claude-haiku-4-5-20251001".into(),
                    cost_usd: 0.25,
                    input: 900,
                    output: 40,
                    cache_read: 0,
                    cache_write: 0,
                },
                ReportedModelCost {
                    model: "claude-opus-5-5".into(),
                    cost_usd: 1.5,
                    input: 12,
                    output: 640,
                    cache_read: 42_000,
                    cache_write: 1_500,
                },
            ],
        })))
    );
    assert_eq!(
        parse_line(br#"{"type":"cost-state","totalCostUSD":1.75}"#),
        Err(claude_dashboard_core::transcript::Unreadable::Incomplete)
    );
}

#[test]
fn a_half_written_last_line_waits_for_the_next_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("4f1c.jsonl");
    let bytes = transcript_bytes();
    let cut = bytes.len() - 10;
    write(&path, &bytes[..cut]);

    let first = read_from(&path, 0, u64::MAX).unwrap();
    assert_eq!(record_ids(&first.records), ["rec-0", "rec-1", "rec-2"]);
    assert!(first.end < cut as u64);

    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(&bytes[cut..])
        .unwrap();
    let second = read_from(&path, first.end, u64::MAX).unwrap();
    assert_eq!(record_ids(&second.records), ["Made-up title"]);
    assert_eq!(second.end, bytes.len() as u64);
    assert!(!second.restarted);
}

#[test]
fn a_read_stops_after_the_line_that_reaches_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("4f1c.jsonl");
    write(&path, &transcript_bytes());

    let mut offset = 0;
    let mut ids = Vec::new();
    let mut reads = 0;
    loop {
        let read = read_from(&path, offset, 1).unwrap();
        if read.end == offset {
            break;
        }
        ids.extend(record_ids(&read.records));
        offset = read.end;
        reads += 1;
    }

    assert_eq!(reads, 7);
    assert_eq!(ids, ["rec-0", "rec-1", "rec-2", "Made-up title"]);
}

#[test]
fn a_shorter_transcript_is_read_again_from_the_start() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("4f1c.jsonl");
    write(&path, &transcript_bytes());
    let end = read_from(&path, 0, u64::MAX).unwrap().end;
    write(&path, &line(&assistant()));

    let read = read_from(&path, end, u64::MAX).unwrap();

    assert!(read.restarted);
    assert_eq!(
        record_ids(&read.records),
        ["7c9e6679-7425-40de-944b-e07fc1f90ae7"]
    );
}

#[test]
fn unreadable_lines_are_counted_and_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("4f1c.jsonl");
    let mut bytes = b"not json\n".to_vec();
    bytes.extend(line(&assistant()));
    write(&path, &bytes);

    let read = read_from(&path, 0, u64::MAX).unwrap();

    assert_eq!(read.unreadable, 1);
    assert_eq!(read.records.len(), 1);
}

#[test]
fn a_deleted_transcript_cannot_be_read() {
    let dir = tempfile::tempdir().unwrap();
    let error = read_from(&dir.path().join("gone.jsonl"), 0, u64::MAX).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

proptest! {
    // Each case opens the file many times, which is slow on Windows.
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn reading_as_the_file_grows_finds_what_one_read_finds(
        mut cuts in proptest::collection::vec(0..=transcript_bytes().len(), 0..12),
        limit in 1_u64..4_096,
    ) {
        let bytes = transcript_bytes();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("4f1c.jsonl");
        cuts.push(bytes.len());
        cuts.sort_unstable();

        let mut file = File::create(&path).unwrap();
        let mut written = 0;
        let mut offset = 0;
        let mut records = Vec::new();
        for cut in cuts {
            file.write_all(&bytes[written..cut]).unwrap();
            file.flush().unwrap();
            written = cut;
            loop {
                let read = read_from(&path, offset, limit).unwrap();
                prop_assert!(!read.restarted);
                records.extend(read.records);
                if read.end == offset {
                    break;
                }
                offset = read.end;
            }
        }

        let whole = read_from(&path, 0, u64::MAX).unwrap();
        prop_assert_eq!(offset, bytes.len() as u64);
        prop_assert_eq!(records, whole.records);
    }
}
