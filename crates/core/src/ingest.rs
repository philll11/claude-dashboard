//! Ingest: reading what Claude Code's transcripts added since the last ingest into the history.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::history::{
    Attribution, History, HistoryError, Identity, ReadPosition, ReportedCost, Request, SessionKey,
    SessionTitle, Writer,
};
use crate::stats_cache;
use crate::transcript::{self, ModelUsage, Read, Record, RequestRecord, Transcript};

/// How much of a transcript one read takes in, so that a large transcript is written in parts.
const CHUNK_BYTES: u64 = 8 << 20;

/// How many threads read transcripts at most, while the calling thread writes what they read.
/// More readers than this only wait for the writer.
const MAX_READERS: usize = 4;

/// How many records one transaction takes in before it commits, unless a single read holds more,
/// so that an ingest never keeps other processes from writing for long.
const RECORDS_PER_WRITE: usize = 4096;

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
    /// When the earliest and the latest request records read were written.
    pub requests_between: Option<(i64, i64)>,
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
    ingest_with_progress(history, roots, |_| {})
}

/// How far an ingest has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// Transcripts looked at so far, read or found unchanged.
    pub done: usize,
    pub total: usize,
}

/// [`ingest`], calling `progress` once it knows which transcripts changed, and again as each of
/// those is read.
///
/// # Errors
///
/// As for [`ingest`].
pub fn ingest_with_progress(
    history: &mut History,
    roots: &[PathBuf],
    progress: impl FnMut(Progress),
) -> Result<Option<Report>, HistoryError> {
    let Some(_lock) = history.try_lock_ingest()? else {
        return Ok(None);
    };
    let mut transcripts: Vec<Transcript> = roots
        .iter()
        .flat_map(|root| transcript::find(root))
        .collect();
    transcripts.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| a.path.cmp(&b.path))
    });
    let positions = history.read_positions()?;
    let mut report = read_transcripts(history, &transcripts, &positions, progress)?;
    let gone = gone(roots, &transcripts, &positions);
    if !gone.is_empty() {
        history.write(|w| {
            for path in &gone {
                w.forget_transcript(path)?;
            }
            Ok(())
        })?;
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
            .collect::<BTreeSet<_>>()
            .len() as u64;
    }
    Ok(Some(report))
}

/// Reads what the given transcripts added since the last ingest, such as one session's, without
/// looking for others or reading Claude Code's statistics.
///
/// Returns `None` without reading anything while another process holds the ingest lock.
///
/// # Errors
///
/// As for [`ingest`].
pub fn ingest_transcripts(
    history: &mut History,
    transcripts: &[Transcript],
) -> Result<Option<Report>, HistoryError> {
    let Some(_lock) = history.try_lock_ingest()? else {
        return Ok(None);
    };
    let paths: Vec<String> = transcripts
        .iter()
        .map(|transcript| path_key(&transcript.path))
        .collect();
    let positions = history.read_positions_of(&paths)?;
    read_transcripts(history, transcripts, &positions, |_| {}).map(Some)
}

/// The transcripts that the history has read but that are no longer there, such as those Claude
/// Code deleted after its clean-up period, in the data roots whose projects folder is there.
///
/// Forgetting how far they were read keeps the read positions from growing for ever. A data root
/// that cannot be reached, such as a WSL distribution that is not running, keeps its positions.
fn gone(
    roots: &[PathBuf],
    found: &[Transcript],
    positions: &HashMap<String, ReadPosition>,
) -> Vec<String> {
    let found: HashSet<String> = found
        .iter()
        .map(|transcript| path_key(&transcript.path))
        .collect();
    let folders: Vec<String> = roots
        .iter()
        .map(|root| root.join("projects"))
        .filter(|folder| folder.is_dir())
        .map(|folder| path_key(&folder.join("")))
        .collect();
    positions
        .keys()
        .filter(|path| !found.contains(*path))
        .filter(|path| {
            folders
                .iter()
                .any(|folder| path.starts_with(folder.as_str()))
        })
        .cloned()
        .collect()
}

/// A transcript with content added since the last ingest.
struct Changed<'a> {
    transcript: &'a Transcript,
    key: String,
    modified_ms: i64,
    /// Where reading starts.
    from: u64,
}

/// What a reading thread found in a transcript.
enum Found {
    /// The next part of transcript `index`; `last` once nothing more of it is there to read.
    Part {
        index: usize,
        read: Read,
        last: bool,
    },
    /// Transcript `index` could not be read, or not to the end.
    Failed { index: usize, error: String },
}

impl Found {
    fn records(&self) -> usize {
        match self {
            Self::Part { read, .. } => read.records.len(),
            Self::Failed { .. } => 0,
        }
    }
}

/// Reads the transcripts that changed since the last ingest and records what they added.
///
/// Several threads read and parse transcripts, taking them in the given order, while this thread
/// writes what they found. The parts of one transcript are written in order; transcripts finish
/// in roughly the order given, and the history does not depend on the order they finish in.
fn read_transcripts(
    history: &mut History,
    transcripts: &[Transcript],
    positions: &HashMap<String, ReadPosition>,
    mut progress: impl FnMut(Progress),
) -> Result<Report, HistoryError> {
    let mut report = Report::default();
    let mut changed = Vec::new();
    for transcript in transcripts {
        let key = path_key(&transcript.path);
        let modified_ms = milliseconds(transcript.modified);
        match positions.get(&key) {
            Some(known) if known.len == transcript.len && known.modified_ms == modified_ms => {
                report.transcripts_unchanged += 1;
            }
            known => changed.push(Changed {
                transcript,
                key,
                modified_ms,
                from: known.map_or(0, |known| known.read_to),
            }),
        }
    }
    report.transcripts_read = changed.len() as u64;
    let total = transcripts.len();
    let mut done = total - changed.len();
    progress(Progress { done, total });

    let readers = thread::available_parallelism()
        .map_or(1, NonZero::get)
        .min(MAX_READERS)
        .min(changed.len());
    let next = AtomicUsize::new(0);
    thread::scope(|scope| {
        let (sender, receiver) = mpsc::sync_channel(2 * readers);
        let (changed, next) = (&changed, &next);
        for _ in 0..readers {
            let sender = sender.clone();
            scope.spawn(move || read_changed(changed, next, &sender));
        }
        drop(sender);
        // Returning drops the receiver before the scope waits for the readers, so a reader
        // blocked on a full channel stops when writing fails.
        write_found(history, changed, &receiver, |found| {
            match found {
                Found::Part { read, last, .. } => {
                    report.unreadable_lines += read.unreadable;
                    for record in &read.records {
                        if let Record::Request(request) = record {
                            report.request_records += 1;
                            let at = request.timestamp_ms;
                            report.requests_between =
                                Some(report.requests_between.map_or((at, at), |(first, last)| {
                                    (first.min(at), last.max(at))
                                }));
                        }
                    }
                    if !last {
                        return;
                    }
                }
                Found::Failed { index, error } => {
                    report
                        .failed
                        .push((changed[index].transcript.path.clone(), error));
                }
            }
            done += 1;
            progress(Progress { done, total });
        })
    })?;
    Ok(report)
}

/// Reads changed transcripts, taking the next one nobody has taken yet, until none is left or
/// nobody is writing any more.
fn read_changed(changed: &[Changed<'_>], next: &AtomicUsize, sender: &SyncSender<Found>) {
    loop {
        let index = next.fetch_add(1, Ordering::Relaxed);
        let Some(Changed {
            transcript, from, ..
        }) = changed.get(index)
        else {
            return;
        };
        let mut from = *from;
        loop {
            let found = match transcript::read_from(&transcript.path, from, CHUNK_BYTES) {
                Ok(read) => {
                    let last = read.end == from || read.end >= transcript.len;
                    from = read.end;
                    Found::Part { index, read, last }
                }
                Err(error) => Found::Failed {
                    index,
                    error: error.to_string(),
                },
            };
            let more = matches!(found, Found::Part { last: false, .. });
            if sender.send(found).is_err() {
                return;
            }
            if !more {
                break;
            }
        }
    }
}

/// Writes what the readers found, a batch of parts per transaction, and hands each part on to
/// `then` once it is written.
fn write_found(
    history: &mut History,
    changed: &[Changed<'_>],
    receiver: &Receiver<Found>,
    mut then: impl FnMut(Found),
) -> Result<(), HistoryError> {
    let mut batch = Vec::new();
    while let Ok(found) = receiver.recv() {
        let mut records = found.records();
        batch.push(found);
        while records < RECORDS_PER_WRITE
            && let Ok(found) = receiver.try_recv()
        {
            records += found.records();
            batch.push(found);
        }
        history.write(|w| {
            for found in &batch {
                if let Found::Part { index, read, .. } = found {
                    let changed = &changed[*index];
                    let position = ReadPosition {
                        read_to: read.end,
                        len: changed.transcript.len,
                        modified_ms: changed.modified_ms,
                    };
                    record(w, changed.transcript, &read.records, &changed.key, position)?;
                }
            }
            Ok(())
        })?;
        batch.drain(..).for_each(&mut then);
    }
    Ok(())
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
        // A subagent may work in another folder, such as a worktree, so only the session's own
        // transcript names the project's working folder.
        let cwd = if transcript.subagent_id.is_none() {
            records.iter().find_map(|record| match record {
                Record::Request(request) => request.cwd.as_deref(),
                _ => None,
            })
        } else {
            None
        };
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
