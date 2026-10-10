//! Benchmarks for reading transcripts, ingesting them into the history and reporting from it.
//!
//! They run on made-up transcripts shaped like Claude Code's, so their numbers compare across
//! machines and changes. With `CLAUDE_DASHBOARD_BENCH_ROOT` set to a data root, such as
//! `~/.claude`, they also time ingesting its real transcripts: a first ingest of all of them, a
//! first ingest of the last 7 days alone, which a first ingest reads first, and an ingest that
//! finds nothing new.
//!
//! `cargo test --benches` runs each benchmark once, as CI does, to keep them working.

use std::fs::{self, OpenOptions};
use std::hint::black_box;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use claude_dashboard_core::history::History;
use claude_dashboard_core::ingest;
use claude_dashboard_core::pricing::PriceTable;
use claude_dashboard_core::reconcile;
use claude_dashboard_core::report::{self, Range};
use claude_dashboard_core::transcript::{self, Transcript};
use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan};
use serde_json::{Value, json};

/// The data root whose real transcripts the benchmarks also ingest.
const REAL_ROOT_ENV: &str = "CLAUDE_DASHBOARD_BENCH_ROOT";

const CWD: &str = r"C:\code\app";
const MODELS: [&str; 3] = ["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"];

/// About `chars` characters of text, with the escapes and non-ASCII characters that
/// transcripts are full of.
fn text(chars: usize) -> String {
    const SNIPPET: &str = "fn main() {\n    println!(\"Grüße — \\\"quoted\\\" C:\\\\code\");\n}\n";
    SNIPPET.chars().cycle().take(chars).collect()
}

fn line(out: &mut Vec<u8>, record: &Value) {
    serde_json::to_writer(&mut *out, record).unwrap();
    out.push(b'\n');
}

/// One turn of a made-up session, about 38 KB in ten lines: a prompt, a response streamed in
/// three records, two tool results, two attachments and two small records. Claude Code's
/// transcripts average about 4 KB a line, half of it in tool results.
fn turn(out: &mut Vec<u8>, session: &str, turn: usize, at: Timestamp) {
    let time = |seconds: i64| (at + seconds.seconds()).to_string();
    let user = |out: &mut Vec<u8>, id: String, content: Value| {
        line(
            out,
            &json!({"type": "user", "uuid": id, "timestamp": time(0), "sessionId": session,
                    "cwd": CWD, "message": {"role": "user", "content": content}}),
        );
    };
    user(out, format!("{session}-{turn}-prompt"), json!(text(500)));
    for (step, output) in [1_u64, 120, 480].into_iter().enumerate() {
        line(
            out,
            &json!({
                "type": "assistant",
                "uuid": format!("{session}-{turn}-{step}"),
                "requestId": format!("req_{session}_{turn}"),
                "timestamp": time(5),
                "sessionId": session,
                "cwd": CWD,
                "entrypoint": "cli",
                "version": "2.1.296",
                "message": {
                    "id": format!("msg_{session}_{turn}"),
                    "model": MODELS[turn % MODELS.len()],
                    "role": "assistant",
                    "content": [{"type": "text", "text": text(1_800)}],
                    "usage": {
                        "input_tokens": 3,
                        "output_tokens": output,
                        "cache_read_input_tokens": 42_000,
                        "cache_creation_input_tokens": 1_500,
                        "cache_creation": {
                            "ephemeral_5m_input_tokens": 500,
                            "ephemeral_1h_input_tokens": 1_000
                        },
                        "server_tool_use": {"web_search_requests": 0, "web_fetch_requests": 0},
                        "service_tier": "standard"
                    }
                }
            }),
        );
    }
    for (n, chars) in [3_000, 25_000].into_iter().enumerate() {
        user(
            out,
            format!("{session}-{turn}-result-{n}"),
            json!([{"type": "tool_result", "tool_use_id": format!("toolu_{turn}_{n}"),
                    "content": text(chars)}]),
        );
    }
    for n in 0..2 {
        line(
            out,
            &json!({"type": "attachment", "uuid": format!("{session}-{turn}-attachment-{n}"),
                    "timestamp": time(6), "attachment": {"type": "file", "content": text(600)}}),
        );
    }
    line(
        out,
        &json!({"type": "mode", "mode": "normal", "sessionId": session}),
    );
    line(
        out,
        &json!({"type": "last-prompt", "lastPrompt": text(200), "sessionId": session}),
    );
}

/// A made-up session of `turns` turns from `start`, with its title and the cost Claude Code
/// reports for it.
fn session(id: &str, turns: usize, start: Timestamp) -> Vec<u8> {
    let mut out = Vec::new();
    for n in 0..turns {
        turn(
            &mut out,
            id,
            n,
            start + (2 * i64::try_from(n).unwrap()).minutes(),
        );
    }
    line(
        &mut out,
        &json!({"type": "ai-title", "aiTitle": "Made-up title", "sessionId": id}),
    );
    line(
        &mut out,
        &json!({"type": "cost-state", "sessionId": id, "startTime": start.as_millisecond(),
                "modelUsage": {"claude-opus-5-5": {"costUSD": 1.5, "inputTokens": 3,
                "outputTokens": 480, "cacheReadInputTokens": 42_000,
                "cacheCreationInputTokens": 1_500}}}),
    );
    out
}

/// Made-up transcripts in a data root: 48 sessions of 5 to 64 turns over the last four weeks,
/// every fourth with a subagent, about 64 MB in all.
struct Corpus {
    root: PathBuf,
    bytes: u64,
    largest: PathBuf,
    /// A session that grows by a turn whenever a benchmark asks.
    growing: PathBuf,
    _dir: tempfile::TempDir,
}

impl Corpus {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".claude");
        let project = root.join("projects").join("C--code-app");
        let now = Timestamp::now();
        let mut written = Vec::new();
        for k in 0..48_usize {
            let id = format!("{k:08x}-0000-4000-8000-000000000000");
            let start = now - (13 * i64::try_from(k).unwrap()).hours();
            written.push((
                project.join(format!("{id}.jsonl")),
                session(&id, 5 + (k * 37) % 60, start),
            ));
            if k % 4 == 0 {
                written.push((
                    project.join(&id).join("subagents").join("agent-a1.jsonl"),
                    session(&format!("{id}-a1"), 5, start),
                ));
            }
        }
        for (path, bytes) in &written {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        let (largest, _) = written.iter().max_by_key(|(_, bytes)| bytes.len()).unwrap();
        Self {
            bytes: written.iter().map(|(_, bytes)| bytes.len() as u64).sum(),
            largest: largest.clone(),
            growing: written[0].0.clone(),
            root,
            _dir: dir,
        }
    }

    fn roots(&self) -> &[PathBuf] {
        std::slice::from_ref(&self.root)
    }

    /// Adds turn `n` to the growing session.
    fn grow(&self, n: usize) {
        let mut bytes = Vec::new();
        turn(&mut bytes, "growing", n, Timestamp::now());
        OpenOptions::new()
            .append(true)
            .open(&self.growing)
            .unwrap()
            .write_all(&bytes)
            .unwrap();
    }
}

/// A history in a folder of its own, removed when it is dropped.
struct Scratch {
    history: History,
    _dir: tempfile::TempDir,
}

impl Scratch {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        Self {
            history: History::open(&dir.path().join("history.sqlite")).unwrap(),
            _dir: dir,
        }
    }

    fn ingested(roots: &[PathBuf]) -> Self {
        let mut scratch = Self::new();
        ingest::ingest(&mut scratch.history, roots).unwrap();
        scratch
    }
}

fn parse(c: &mut Criterion, corpus: &Corpus) {
    let mut lines = Vec::new();
    turn(&mut lines, "parse", 0, Timestamp::now());
    let lines: Vec<&[u8]> = lines.split_inclusive(|&byte| byte == b'\n').collect();
    let (assistant, tool_result) = (lines[3], lines[5]);

    let mut group = c.benchmark_group("parse");
    group.throughput(Throughput::Bytes(assistant.len() as u64));
    group.bench_function("assistant record", |b| {
        b.iter(|| transcript::parse_line(black_box(assistant)));
    });
    group.throughput(Throughput::Bytes(tool_result.len() as u64));
    group.bench_function("tool result", |b| {
        b.iter(|| transcript::parse_line(black_box(tool_result)));
    });
    group.throughput(Throughput::Bytes(
        fs::metadata(&corpus.largest).unwrap().len(),
    ));
    group.bench_function("transcript", |b| {
        b.iter(|| transcript::read_from(&corpus.largest, 0, u64::MAX).unwrap());
    });
    group.finish();
}

fn reports(c: &mut Criterion, corpus: &Corpus) {
    let scratch = Scratch::ingested(corpus.roots());
    let history = &scratch.history;
    let table = PriceTable::bundled();
    let now = Timestamp::now();
    let days = |days: i64| Range {
        from_ms: (now - (24 * days).hours()).as_millisecond(),
        to_ms: now.as_millisecond(),
    };

    let mut group = c.benchmark_group("report");
    group.bench_function("by day, 30 days", |b| {
        b.iter(|| report::by_day(history, table, days(30), &TimeZone::UTC).unwrap());
    });
    group.bench_function("by model, 30 days", |b| {
        b.iter(|| report::by_model(history, table, days(30)).unwrap());
    });
    group.bench_function("by session, 7 days", |b| {
        b.iter(|| report::by_session(history, table, days(7)).unwrap());
    });
    group.bench_function("by project, 30 days", |b| {
        b.iter(|| report::by_project(history, table, days(30)).unwrap());
    });
    group.bench_function("reconcile", |b| {
        b.iter(|| reconcile::reconcile(history, table, now.as_millisecond()).unwrap());
    });
    group.finish();
}

fn ingests(c: &mut Criterion, corpus: &Corpus) {
    let mut group = c.benchmark_group("ingest");
    group.sample_size(20);
    group.throughput(Throughput::Bytes(corpus.bytes));
    group.bench_function("first", |b| {
        b.iter_batched(
            Scratch::new,
            |mut scratch| {
                ingest::ingest(&mut scratch.history, corpus.roots()).unwrap();
                scratch
            },
            BatchSize::PerIteration,
        );
    });
    group.throughput(Throughput::Elements(1));
    let mut scratch = Scratch::ingested(corpus.roots());
    group.bench_function("nothing new", |b| {
        b.iter(|| ingest::ingest(&mut scratch.history, corpus.roots()).unwrap());
    });
    let mut n = 0;
    group.bench_function("a session grew by a turn", |b| {
        b.iter_batched(
            || {
                n += 1;
                corpus.grow(n);
            },
            |()| ingest::ingest(&mut scratch.history, corpus.roots()).unwrap(),
            BatchSize::PerIteration,
        );
    });
    group.finish();
}

fn real_data(c: &mut Criterion) {
    let Some(root) = std::env::var_os(REAL_ROOT_ENV).map(PathBuf::from) else {
        return;
    };
    let roots = std::slice::from_ref(&root);
    let all = transcript::find(&root);
    let week_ago = SystemTime::now() - Duration::from_hours(7 * 24);
    let recent: Vec<Transcript> = all
        .iter()
        .filter(|transcript| transcript.modified >= week_ago)
        .cloned()
        .collect();
    let bytes = |transcripts: &[Transcript]| transcripts.iter().map(|t| t.len).sum();

    let mut group = c.benchmark_group("real data");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(60));
    group.throughput(Throughput::Bytes(bytes(&all)));
    group.bench_function("first ingest", |b| {
        b.iter_batched(
            Scratch::new,
            |mut scratch| {
                ingest::ingest(&mut scratch.history, roots).unwrap();
                scratch
            },
            BatchSize::PerIteration,
        );
    });
    group.throughput(Throughput::Bytes(bytes(&recent)));
    group.bench_function("first ingest of the last 7 days", |b| {
        b.iter_batched(
            Scratch::new,
            |mut scratch| {
                ingest::ingest_transcripts(&mut scratch.history, &recent).unwrap();
                scratch
            },
            BatchSize::PerIteration,
        );
    });
    group.throughput(Throughput::Elements(1));
    let mut scratch = Scratch::ingested(roots);
    group.measurement_time(Duration::from_secs(10));
    group.bench_function("nothing new", |b| {
        b.iter(|| ingest::ingest(&mut scratch.history, roots).unwrap());
    });
    group.finish();
}

fn benchmarks(c: &mut Criterion) {
    let corpus = Corpus::new();
    parse(c, &corpus);
    reports(c, &corpus);
    ingests(c, &corpus);
    real_data(c);
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
