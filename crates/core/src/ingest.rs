//! Ingest: reading what Claude Code's transcripts added since the last ingest into the history.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::history::{
    Attribution, History, HistoryError, Identity, ReadPosition, ReportedCost, Request, SessionKey,
    SessionTitle, Writer,
};
use crate::stats_cache;
use crate::transcript::{self, ModelUsage, Record, RequestRecord, Transcript};

/// How much of a transcript one transaction takes in, so that a large transcript does not keep
/// other processes from writing for long.
const CHUNK_BYTES: u64 = 8 << 20;

/// What one ingest did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Transcripts with content added since the last ingest.
    pub transcripts_read: u64,
    /// Transcripts that had not changed.
    pub transcripts_unchanged: u64,
    /// Request records read; a streamed response leaves several for one request.
    pub request_records: u64,
    /// Lines that this version cannot read.
    pub unreadable_lines: u64,
    /// Days with tokens from Claude Code's statistics, summed over the data roots.
    pub daily_totals: u64,
    /// Files that could not be read, such as a transcript Claude Code deleted meanwhile, with the
    /// reason.
    pub failed: Vec<(PathBuf, String)>,
}

/// Reads what the transcripts in `roots` added since the last ingest into the history, starting
/// with the most recently changed transcripts so that recent days are complete first, and then
/// the daily totals from Claude Code's statistics.
///
/// Returns `None` without reading anything while another process holds the ingest lock.
///
/// # Errors
///
/// [`HistoryError`] when the history cannot be written. A transcript that cannot be read is
/// listed in the report instead.
pub fn ingest(history: &mut History, roots: &[PathBuf]) -> Result<Option<Report>, HistoryError> {
    let Some(_lock) = history.try_lock_ingest()? else {
        return Ok(None);
    };
    let positions = history.read_positions()?;
    let mut transcripts: Vec<Transcript> = roots
        .iter()
        .flat_map(|root| transcript::find(root))
        .collect();
    transcripts.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| a.path.cmp(&b.path))
    });

    let mut report = Report::default();
    for transcript in &transcripts {
        let key = path_key(&transcript.path);
        let modified_ms = milliseconds(transcript.modified);
        let known = positions.get(&key);
        if known
            .is_some_and(|known| known.len == transcript.len && known.modified_ms == modified_ms)
        {
            report.transcripts_unchanged += 1;
            continue;
        }
        report.transcripts_read += 1;
        let mut offset = known.map_or(0, |known| known.read_to);
        loop {
            let read = match transcript::read_from(&transcript.path, offset, CHUNK_BYTES) {
                Ok(read) => read,
                Err(error) => {
                    report
                        .failed
                        .push((transcript.path.clone(), error.to_string()));
                    break;
                }
            };
            report.unreadable_lines += read.unreadable;
            report.request_records += read
                .records
                .iter()
                .filter(|record| matches!(record, Record::Request(_)))
                .count() as u64;
            let position = ReadPosition {
                read_to: read.end,
                len: transcript.len,
                modified_ms,
            };
            history.write(|w| record(w, transcript, &read.records, &key, position))?;
            if read.end == offset || read.end >= transcript.len {
                break;
            }
            offset = read.end;
        }
    }

    let mut totals: BTreeMap<(String, String), u64> = BTreeMap::new();
    for root in roots {
        match stats_cache::daily_totals(root) {
            Ok(days) => {
                for day in days {
                    let total = totals.entry((day.day, day.model)).or_default();
                    *total = total.saturating_add(day.tokens);
                }
            }
            Err(error) => report
                .failed
                .push((root.join("stats-cache.json"), error.to_string())),
        }
    }
    if !totals.is_empty() {
        history.write(|w| {
            for ((day, model), tokens) in &totals {
                w.daily_total(day, model, *tokens)?;
            }
            Ok(())
        })?;
        report.daily_totals = totals
            .keys()
            .map(|(day, _)| day)
            .collect::<std::collections::BTreeSet<_>>()
            .len() as u64;
    }
    Ok(Some(report))
}

/// Records what one read of a transcript found, and where the next read starts.
fn record(
    w: &mut Writer<'_>,
    transcript: &Transcript,
    records: &[Record],
    key: &str,
    position: ReadPosition,
) -> Result<(), HistoryError> {
    if !records.is_empty() {
        let cwd = records.iter().find_map(|record| match record {
            Record::Request(request) => request.cwd.as_deref(),
            _ => None,
        });
        let project = w.project(&transcript.project_folder, cwd)?;
        let session = w.session(&transcript.session_id, project)?;
        for record in records {
            match record {
                Record::Request(request) => {
                    record_request(w, session, transcript.subagent_id.as_deref(), request)?;
                }
                Record::AiTitle(title) => w.session_title(session, SessionTitle::Ai(title))?,
                Record::CustomTitle(title) => {
                    w.session_title(session, SessionTitle::Custom(title))?;
                }
                Record::CostState(state) => {
                    for cost in &state.models {
                        w.reported_cost(
                            session,
                            state.started_ms,
                            &ReportedCost {
                                model: &cost.model,
                                cost_usd: cost.cost_usd,
                                input: cost.input,
                                output: cost.output,
                                cache_read: cost.cache_read,
                                cache_write: cost.cache_write,
                            },
                        )?;
                    }
                }
            }
        }
    }
    w.transcript_read(key, position)
}

/// Records the request a transcript record holds: one request per model it used.
///
/// The record's own model keeps the request's identity. An attempt on another model, such as
/// one Claude Code made before falling back, is a request of its own, told apart by that model's
/// name added to its most specific id.
fn record_request(
    w: &mut Writer<'_>,
    session: SessionKey,
    subagent_id: Option<&str>,
    request: &RequestRecord,
) -> Result<(), HistoryError> {
    for (index, ModelUsage { model, tokens }) in request.usage.iter().enumerate() {
        let tag = |id: &str| {
            if index == 0 {
                id.to_owned()
            } else {
                format!("{id}@{model}")
            }
        };
        let (message_id, request_id) = match (&request.message_id, &request.request_id) {
            (Some(message), Some(request)) => (Some(message.clone()), Some(tag(request))),
            (Some(message), None) => (Some(tag(message)), None),
            (None, request) => (None, request.clone()),
        };
        let record_id = tag(&request.record_id);
        let first = index == 0;
        w.request(&Request {
            session,
            identity: Identity::from_ids(message_id.as_deref(), request_id.as_deref()),
            record_id: &record_id,
            subagent_id,
            timestamp_ms: request.timestamp_ms,
            model,
            tokens: *tokens,
            web_search_requests: if first {
                request.web_search_requests
            } else {
                0
            },
            web_fetch_requests: if first { request.web_fetch_requests } else { 0 },
            speed: request.speed.as_deref(),
            service_tier: request.service_tier.as_deref(),
            inference_geo: request.inference_geo.as_deref(),
            attribution: Attribution {
                skill: request.skill.as_deref(),
                mcp_server: request.mcp_server.as_deref(),
                mcp_tool: request.mcp_tool.as_deref(),
            },
            entrypoint: request.entrypoint.as_deref(),
            claude_code_version: request.claude_code_version.as_deref(),
            imported_from: None,
        })?;
    }
    Ok(())
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn milliseconds(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |since| {
        i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
    })
}
