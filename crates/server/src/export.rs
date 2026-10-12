//! Exports: the dashboard's tables as CSV or JSON, the same from every surface.
//!
//! CSV follows RFC 4180: a header row, then a record per line, with any field that holds a comma,
//! a quote, a carriage return or a line feed quoted and its quotes doubled. Days are `YYYY-MM-DD`
//! in the time zone days are counted in, instants are UTC RFC 3339, numbers have no grouping and
//! dollars no symbol. JSON keeps the reports' shapes: nested `tokens` and `cost`, and models by
//! id. Costs are to the cent, except a request's own, which rounding would turn to $0.00 for most
//! requests.
//!
//! A file saved for the person is meant for a spreadsheet as much as for a script. As CSV it
//! starts with a byte order mark, without which Excel reads UTF-8 as another encoding, and a text
//! field that a spreadsheet would take for a formula starts with an apostrophe, as OWASP advises
//! against CSV injection, since titles and names come from transcripts. Standard output gets
//! neither. An export reads the history over its own connection, so the dashboard keeps
//! answering while it writes.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::io::{self, BufWriter, Write};
use std::ops::ControlFlow;
use std::path::Path;

use claude_dashboard_core::history::{History, HistoryError, SessionKey, StoredRequest};
use claude_dashboard_core::limits;
use claude_dashboard_core::pricing;
use claude_dashboard_core::report::{self, Bucket, DayTotals, Range, Totals};
use jiff::civil::Date;
use jiff::{Span, Timestamp};
use serde::Serialize;

use crate::Fault;
use crate::app::{self, App};
use crate::fmt::grouped;
use crate::protocol::{Dataset, ExportFormat, ExportParams, Exported, Filters, FiveHourWindow};
use crate::requests::{self, Names, matching_sessions};

/// The most an export returned as content holds: the VS Code extension writes that itself, where
/// the server cannot, such as on the person's own computer from a remote window.
pub const LONGEST_CONTENT: usize = 50 * 1024 * 1024;

/// How many requests are written between reports of progress.
const PROGRESS_EVERY: u64 = 1_000;

/// What starts a CSV file saved for a spreadsheet: UTF-8's byte order mark.
const BYTE_ORDER_MARK: &str = "\u{feff}";

/// Why an export did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportError {
    /// It was cancelled.
    Cancelled,
    /// It failed, for this reason.
    Failed(String),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("The export was cancelled."),
            Self::Failed(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<HistoryError> for ExportError {
    fn from(error: HistoryError) -> Self {
        Self::Failed(format!("Cannot read the history: {error}"))
    }
}

impl From<Fault> for ExportError {
    fn from(fault: Fault) -> Self {
        Self::Failed(fault.message)
    }
}

/// Told how many rows an export has written and how many it will write, 0 when that is not
/// known; it breaks to cancel the export.
pub type Progress<'a> = &'a mut dyn FnMut(u64, u64) -> ControlFlow<()>;

/// The file name to suggest for an export, such as
/// `claude-dashboard-sessions-2026-09-13-to-2026-10-12.csv`.
///
/// # Errors
///
/// Why the history cannot be read, or the days cannot be.
pub fn name(app: &App, params: &ExportParams) -> Result<String, String> {
    let span = if params.dataset == Dataset::Requests && params.session.is_some() {
        None
    } else {
        let history = app.open_history()?;
        let app = app.with_shared(&history).at(app::now());
        Some(span(&app, &history, params).map_err(|error| error.to_string())?)
    };
    Ok(file_name(params, span))
}

/// The file name to suggest for an export of the days from `span`'s first to its last, or, for
/// one session's requests, which need no days, of that session.
#[must_use]
pub fn file_name(params: &ExportParams, span: Option<(Date, Date)>) -> String {
    let extension = match params.format {
        ExportFormat::Csv => "csv",
        ExportFormat::Json => "json",
    };
    let stem = match (params.dataset, &params.session, span) {
        (Dataset::Requests, Some(id), _) => {
            let id: String = id
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                .take(8)
                .collect();
            format!("session-{id}")
        }
        (dataset, _, Some((first, last))) => format!("{}-{first}-to-{last}", stem(dataset)),
        (dataset, _, None) => stem(dataset).to_owned(),
    };
    format!("claude-dashboard-{stem}.{extension}")
}

/// What a dataset's rows are called in a file name.
fn stem(dataset: Dataset) -> &'static str {
    match dataset {
        Dataset::Days => "days",
        Dataset::Weeks => "weeks",
        Dataset::Months => "months",
        Dataset::Models => "models",
        Dataset::Projects => "projects",
        Dataset::Sessions => "sessions",
        Dataset::FiveHourWindows => "five-hour-windows",
        Dataset::Requests => "history",
    }
}

/// The first and last day an export covers.
fn span(app: &App, history: &History, params: &ExportParams) -> Result<(Date, Date), ExportError> {
    let today = app.now_zoned().date();
    if let Some(days) = &params.days {
        let day = |text: &str| {
            text.parse::<Date>()
                .map_err(|_| ExportError::Failed(format!("{text} is not a day such as 2026-10-11")))
        };
        return Ok((day(&days.first)?, day(&days.last)?));
    }
    let first = if params.dataset == Dataset::FiveHourWindows {
        today.checked_sub(Span::new().days(7)).unwrap_or(today)
    } else {
        history
            .first_request_ms()?
            .and_then(|ms| Timestamp::from_millisecond(ms).ok())
            .map_or(today, |at| at.to_zoned(app.zone.clone()).date())
    };
    Ok((first, today))
}

/// What `rows` rows of `dataset` are, such as `214 sessions`.
#[must_use]
pub fn what(dataset: Dataset, rows: u64) -> String {
    let (one, many) = match dataset {
        Dataset::Days => ("day", "days"),
        Dataset::Weeks => ("week", "weeks"),
        Dataset::Months => ("month", "months"),
        Dataset::Models => ("model", "models"),
        Dataset::Projects => ("project", "projects"),
        Dataset::Sessions => ("session", "sessions"),
        Dataset::FiveHourWindows => ("five-hour window", "five-hour windows"),
        Dataset::Requests => ("request", "requests"),
    };
    format!("{} {}", grouped(rows), if rows == 1 { one } else { many })
}

/// Saves an export to `path`, written beside it under a temporary name and put in its place once
/// complete, so that a cancelled or failed export never leaves half a file.
///
/// # Errors
///
/// [`ExportError`] when it is cancelled, or the history cannot be read or the file written.
pub fn save(
    app: &App,
    params: &ExportParams,
    path: &Path,
    progress: Progress<'_>,
) -> Result<Exported, ExportError> {
    let folder = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let cannot_write = |error: io::Error| {
        ExportError::Failed(format!("Cannot write to {}: {error}", folder.display()))
    };
    let mut file = tempfile::Builder::new()
        .prefix(".claude-dashboard-")
        .suffix(".part")
        .tempfile_in(folder)
        .map_err(cannot_write)?;
    let rows = {
        let mut out = BufWriter::new(file.as_file_mut());
        let rows = write(app, params, &mut out, true, progress)?;
        out.flush().map_err(cannot_write)?;
        rows
    };
    file.as_file().sync_all().map_err(cannot_write)?;
    file.persist(path).map_err(|error| {
        ExportError::Failed(format!("Cannot save {}: {}", path.display(), error.error))
    })?;
    Ok(Exported {
        rows,
        what: what(params.dataset, rows),
        path: Some(path.display().to_string()),
        content: None,
    })
}

/// An export as content, as a saved file holds it, for a client to save where the server cannot.
///
/// # Errors
///
/// [`ExportError`] when it is cancelled, the history cannot be read, or the export is larger than
/// [`LONGEST_CONTENT`].
pub fn content(
    app: &App,
    params: &ExportParams,
    progress: Progress<'_>,
) -> Result<Exported, ExportError> {
    let mut out = Capped::default();
    let rows = match write(app, params, &mut out, true, progress) {
        Err(_) if out.overflowed => {
            return Err(ExportError::Failed(format!(
                "The export is larger than {} MB. Save it on the computer where claude-dashboard runs, or run claude-dashboard export there.",
                LONGEST_CONTENT / (1024 * 1024)
            )));
        }
        result => result?,
    };
    let content = String::from_utf8(out.bytes)
        .map_err(|_| ExportError::Failed("The export is not valid UTF-8.".to_owned()))?;
    Ok(Exported {
        rows,
        what: what(params.dataset, rows),
        path: None,
        content: Some(content),
    })
}

/// What an export writes into memory, up to [`LONGEST_CONTENT`].
#[derive(Default)]
struct Capped {
    bytes: Vec<u8>,
    overflowed: bool,
}

impl Write for Capped {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.bytes.len() + buf.len() > LONGEST_CONTENT {
            self.overflowed = true;
            return Err(io::Error::other("the export is too large"));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Writes an export to `out`: for a file saved for the person when `saved`, otherwise for
/// standard output. Returns how many rows it wrote.
///
/// # Errors
///
/// [`ExportError`] when it is cancelled, or the history cannot be read or `out` written.
pub fn write(
    app: &App,
    params: &ExportParams,
    out: &mut dyn Write,
    saved: bool,
    progress: Progress<'_>,
) -> Result<u64, ExportError> {
    let history = app.open_history().map_err(ExportError::Failed)?;
    let app = app.with_shared(&history).at(app::now());
    let range = match &params.days {
        Some(days) => requests::range(&app, days)?,
        None => Range {
            from_ms: i64::MIN,
            to_ms: i64::MAX,
        },
    };
    let mut out = Out {
        writer: out,
        saved,
        format: params.format,
    };
    let rows = match params.dataset {
        Dataset::Days => {
            let summary = summary(&app, &history, range, &params.filters)?;
            out.table(&rows(&summary.days, |day| DayRow::of(&app, day)))
        }
        Dataset::Weeks | Dataset::Months => {
            let bucket = if params.dataset == Dataset::Weeks {
                Bucket::Week
            } else {
                Bucket::Month
            };
            let summary = summary(&app, &history, range, &params.filters)?;
            let periods = report::by_bucket(&summary.days, bucket, app.week_start);
            out.table(&rows(&periods, |period| {
                PeriodRow::of(&app, bucket, period)
            }))
        }
        Dataset::Models => {
            let summary = summary(&app, &history, range, &params.filters)?;
            out.table(&rows(&summary.models, |model| ModelRow {
                model: model.model.clone(),
                name: app.prices.model_name(&model.model).map(str::to_owned),
                requests: model.totals.requests,
                tokens: TokensJson::of(&model.totals),
                cost: CostJson::of(&app, &model.totals),
            }))
        }
        Dataset::Projects => {
            let summary = summary(&app, &history, range, &params.filters)?;
            let names = crate::names::project_names(&history);
            out.table(&rows(&summary.projects, |project| {
                let project = &project.totals;
                let (name, path, transcript_folder) =
                    names.get(&project.project).cloned().unwrap_or_default();
                ProjectRow {
                    name,
                    path,
                    transcript_folder,
                    sessions: project.sessions,
                    requests: project.totals.requests,
                    tokens: TokensJson::of(&project.totals),
                    cost: CostJson::of(&app, &project.totals),
                    last_request_at: at(project.last_ms),
                }
            }))
        }
        Dataset::Sessions => {
            let sessions = matching_sessions(
                &app,
                &history,
                range,
                &params.filters,
                params.search.as_deref(),
            )?;
            out.table(&rows(&sessions, |session| SessionRow::of(&app, session)))
        }
        Dataset::FiveHourWindows => {
            let now_ms = app.now.as_millisecond();
            let (since_ms, until_ms) = match &params.days {
                Some(_) => (range.from_ms, range.to_ms.min(now_ms)),
                None => (now_ms - crate::limits::RECENT_WINDOWS_MS, now_ms),
            };
            let windows =
                limits::recent_five_hour_windows(&history, &app.prices, since_ms, until_ms)?;
            out.table(&rows(&windows, |(window, totals)| {
                WindowRow::of(
                    &app,
                    &FiveHourWindow {
                        start_ms: window.start_ms,
                        resets_at_ms: window.resets_at_ms,
                        highest: window.highest,
                        totals: requests::totals(totals),
                    },
                )
            }))
        }
        Dataset::Requests => return out.requests(&app, &history, range, params, progress),
    }
    .map_err(|error| failed_write(&error))?;
    let _ = progress(rows, rows);
    Ok(rows)
}

/// Writes `days` as the days dataset does, for a report that has added them up itself; for
/// standard output.
///
/// # Errors
///
/// Why `out` cannot be written.
pub fn write_days(
    app: &App,
    days: &[DayTotals],
    format: ExportFormat,
    out: &mut dyn Write,
) -> io::Result<()> {
    Out {
        writer: out,
        saved: false,
        format,
    }
    .table(&rows(days, |day| DayRow::of(app, day)))
    .map(drop)
}

/// A field of a CSV record that a report writes itself, quoted when it needs to be.
#[must_use]
pub fn csv_field(text: &str) -> String {
    let mut field = Vec::new();
    let _ = Out {
        writer: &mut field,
        saved: false,
        format: ExportFormat::Csv,
    }
    .text(text);
    String::from_utf8(field).unwrap_or_default()
}

fn failed_write(error: &io::Error) -> ExportError {
    ExportError::Failed(format!("Cannot write the export: {error}"))
}

fn summary(
    app: &App,
    history: &History,
    range: Range,
    filters: &Filters,
) -> Result<report::Summary, ExportError> {
    let names = crate::names::project_names(history);
    let filter = requests::filter(&names, filters)?;
    Ok(report::summary(
        history,
        &app.prices,
        range,
        &filter,
        &app.zone,
    )?)
}

fn rows<T, R>(items: &[T], row: impl FnMut(&T) -> R) -> Vec<R> {
    items.iter().map(row).collect()
}

/// An instant as UTC RFC 3339.
fn at(ms: i64) -> String {
    Timestamp::from_millisecond(ms)
        .map(|at| at.to_string())
        .unwrap_or_default()
}

/// Dollars to the cent.
fn cents(usd: f64) -> f64 {
    (usd * 100.0).round() / 100.0
}

/// A field of a CSV record.
enum Cell<'a> {
    /// Text from the history, quoted when it needs to be, and kept from being read as a formula
    /// in a saved file.
    Text(Cow<'a, str>),
    /// A number, day or instant, written as it is.
    Plain(String),
}

fn text(value: Option<&str>) -> Cell<'_> {
    Cell::Text(Cow::Borrowed(value.unwrap_or_default()))
}

fn plain(value: &dyn fmt::Display) -> Cell<'static> {
    Cell::Plain(value.to_string())
}

/// Dollars to the cent, without a symbol.
fn usd(value: f64) -> Cell<'static> {
    Cell::Plain(format!("{value:.2}"))
}

/// A row of a dataset: its fields as CSV, and as JSON by its serialisation.
trait Row: Serialize {
    /// The CSV header: the columns of the reports first, in their order, then any added since.
    const COLUMNS: &'static [&'static str];

    fn cells(&self) -> Vec<Cell<'_>>;
}

/// Where an export goes, and how.
struct Out<'a> {
    writer: &'a mut dyn Write,
    saved: bool,
    format: ExportFormat,
}

impl Out<'_> {
    /// Writes a dataset held in memory and returns how many rows it has.
    fn table<R: Row>(&mut self, rows: &[R]) -> io::Result<u64> {
        match self.format {
            ExportFormat::Csv => {
                self.header(R::COLUMNS)?;
                for row in rows {
                    self.record(&row.cells())?;
                }
            }
            ExportFormat::Json => {
                serde_json::to_writer_pretty(&mut *self.writer, rows)?;
                self.writer.write_all(b"\n")?;
            }
        }
        Ok(rows.len() as u64)
    }

    fn header(&mut self, columns: &[&str]) -> io::Result<()> {
        if self.saved {
            self.writer.write_all(BYTE_ORDER_MARK.as_bytes())?;
        }
        self.writer.write_all(columns.join(",").as_bytes())?;
        self.writer.write_all(b"\n")
    }

    fn record(&mut self, cells: &[Cell<'_>]) -> io::Result<()> {
        for (index, cell) in cells.iter().enumerate() {
            if index > 0 {
                self.writer.write_all(b",")?;
            }
            match cell {
                Cell::Plain(value) => self.writer.write_all(value.as_bytes())?,
                Cell::Text(value) => self.text(value)?,
            }
        }
        self.writer.write_all(b"\n")
    }

    fn text(&mut self, value: &str) -> io::Result<()> {
        let guarded = self.saved && value.starts_with(['=', '+', '-', '@', '\t', '\r']);
        let quoted = value.contains([',', '"', '\r', '\n']);
        if quoted {
            self.writer.write_all(b"\"")?;
        }
        if guarded {
            self.writer.write_all(b"'")?;
        }
        if quoted {
            self.writer
                .write_all(value.replace('"', "\"\"").as_bytes())?;
            self.writer.write_all(b"\"")
        } else {
            self.writer.write_all(value.as_bytes())
        }
    }

    /// Writes requests one at a time as the history reads them, so that however many there are,
    /// they never sit in memory together.
    fn requests(
        &mut self,
        app: &App,
        history: &History,
        range: Range,
        params: &ExportParams,
        progress: Progress<'_>,
    ) -> Result<u64, ExportError> {
        let names = crate::names::project_names(history);
        let filter = requests::filter(&names, &params.filters)?;
        let sessions: HashMap<SessionKey, (String, Option<String>)> = history
            .sessions()?
            .into_iter()
            .map(|session| (session.key, (session.session_id, session.title)))
            .collect();
        let agent_types = history.all_subagent_types()?;
        let session = match &params.session {
            Some(id) => Some(
                sessions
                    .iter()
                    .find(|(_, (session_id, _))| session_id == id)
                    .map(|(key, _)| *key)
                    .ok_or_else(|| {
                        ExportError::Failed(format!("The history holds no session {id}"))
                    })?,
            ),
            None => None,
        };
        let total = match session {
            Some(_) => 0,
            None => history.count_requests(range.from_ms, range.to_ms)?,
        };
        let context = RequestContext {
            app,
            names: &names,
            sessions: &sessions,
            agent_types: &agent_types,
            price_table: app.prices.checked().to_string(),
        };
        if self.format == ExportFormat::Csv {
            self.header(RequestRow::COLUMNS)
                .map_err(|error| failed_write(&error))?;
        }
        let mut written = 0_u64;
        let mut each = |request: &StoredRequest<'_>| -> ControlFlow<ExportError> {
            if !filter.admits(request) {
                return ControlFlow::Continue(());
            }
            let row = context.row(request);
            let result = match self.format {
                ExportFormat::Csv => self.record(&row.cells()),
                ExportFormat::Json => {
                    let start: &[u8] = if written == 0 { b"[\n" } else { b",\n" };
                    self.writer.write_all(start).and_then(|()| {
                        serde_json::to_writer(&mut *self.writer, &row).map_err(io::Error::from)
                    })
                }
            };
            if let Err(error) = result {
                return ControlFlow::Break(failed_write(&error));
            }
            written += 1;
            if written.is_multiple_of(PROGRESS_EVERY) && progress(written, total).is_break() {
                return ControlFlow::Break(ExportError::Cancelled);
            }
            ControlFlow::Continue(())
        };
        let flow = match session {
            Some(key) => history.try_for_each_request_in_session(key, &mut each)?,
            None => history.try_for_each_request(range.from_ms, range.to_ms, &mut each)?,
        };
        if let ControlFlow::Break(error) = flow {
            return Err(error);
        }
        if self.format == ExportFormat::Json {
            let end: &[u8] = if written == 0 { b"[]\n" } else { b"\n]\n" };
            self.writer
                .write_all(end)
                .map_err(|error| failed_write(&error))?;
        }
        let _ = progress(written, total.max(written));
        Ok(written)
    }
}

/// What every request's row looks up.
struct RequestContext<'a> {
    app: &'a App,
    names: &'a Names,
    sessions: &'a HashMap<SessionKey, (String, Option<String>)>,
    agent_types: &'a HashMap<(SessionKey, String), String>,
    price_table: String,
}

impl RequestContext<'_> {
    fn row<'r>(&'r self, request: &'r StoredRequest<'r>) -> RequestRow<'r> {
        let (session_id, title) = self
            .sessions
            .get(&request.session)
            .map(|(id, title)| (id.as_str(), title.as_deref()))
            .unwrap_or_default();
        let project = self
            .names
            .get(&request.project)
            .map(|(name, _, _)| name.as_str())
            .unwrap_or_default();
        let agent_type = request.subagent_id.and_then(|subagent| {
            self.agent_types
                .get(&(request.session, subagent.to_owned()))
                .map(String::as_str)
        });
        let cost = self
            .app
            .prices
            .cost(&pricing::Request {
                model: request.model,
                timestamp_ms: request.timestamp_ms,
                tokens: request.tokens,
                speed: request.speed,
                inference_geo: request.inference_geo,
                web_search_requests: request.web_search_requests,
            })
            .map(|cost| cost.total());
        let tokens = request.tokens;
        RequestRow {
            at: at(request.timestamp_ms),
            session_id,
            title,
            project,
            subagent_id: request.subagent_id,
            agent_type,
            model: request.model,
            tokens: TokensJson {
                input: tokens.input,
                output: tokens.output,
                cache_write_5m: tokens.cache_write_5m,
                cache_write_1h: tokens.cache_write_1h,
                cache_read: tokens.cache_read,
            },
            web_search_requests: request.web_search_requests,
            web_fetch_requests: request.web_fetch_requests,
            speed: request.speed,
            inference_geo: request.inference_geo,
            skill: request.skill,
            mcp_server: request.mcp_server,
            mcp_tool: request.mcp_tool,
            entrypoint: request.entrypoint,
            claude_code_version: request.claude_code_version,
            cost: RequestCostJson {
                usd: cost,
                provenance: "estimate",
                price_table: &self.price_table,
            },
        }
    }
}

/// Tokens by kind, as JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TokensJson {
    pub input: u64,
    pub output: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub cache_read: u64,
}

impl TokensJson {
    #[must_use]
    pub fn of(totals: &Totals) -> Self {
        let tokens = totals.tokens;
        Self {
            input: tokens.input,
            output: tokens.output,
            cache_write_5m: tokens.cache_write_5m,
            cache_write_1h: tokens.cache_write_1h,
            cache_read: tokens.cache_read,
        }
    }
}

/// An API-equivalent cost in US dollars to the cent, as JSON, leaving out `unpriced_requests`
/// whose cost is unknown.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CostJson {
    pub usd: f64,
    pub provenance: &'static str,
    /// The day the price table was checked against Anthropic's pricing page.
    pub price_table: String,
    pub unpriced_requests: u64,
}

impl CostJson {
    #[must_use]
    pub fn of(app: &App, totals: &Totals) -> Self {
        Self::new(app, totals.cost.total(), totals.unpriced_requests)
    }

    /// An API-equivalent cost of `usd`, leaving out `unpriced_requests` whose cost is unknown.
    #[must_use]
    pub fn new(app: &App, usd: f64, unpriced_requests: u64) -> Self {
        Self {
            usd: cents(usd),
            provenance: "estimate",
            price_table: app.prices.checked().to_string(),
            unpriced_requests,
        }
    }
}

/// The CSV fields every total ends its columns with in the reports: the tokens by kind, then the
/// cost, the unpriced requests and the price table.
fn total_cells(totals: &TokensJson, cost: &CostJson) -> [Cell<'static>; 8] {
    [
        plain(&totals.input),
        plain(&totals.output),
        plain(&totals.cache_write_5m),
        plain(&totals.cache_write_1h),
        plain(&totals.cache_read),
        usd(cost.usd),
        plain(&cost.unpriced_requests),
        plain(&cost.price_table),
    ]
}

/// One day.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayRow {
    pub date: String,
    pub requests: u64,
    pub tokens: TokensJson,
    pub cost: CostJson,
    pub models: Vec<String>,
}

impl DayRow {
    #[must_use]
    pub fn of(app: &App, day: &DayTotals) -> Self {
        Self {
            date: day.day.to_string(),
            requests: day.totals.requests,
            tokens: TokensJson::of(&day.totals),
            cost: CostJson::of(app, &day.totals),
            models: day.models.iter().map(|model| model.model.clone()).collect(),
        }
    }
}

impl Row for DayRow {
    const COLUMNS: &'static [&'static str] = &[
        "date",
        "requests",
        "input_tokens",
        "output_tokens",
        "cache_write_5m_tokens",
        "cache_write_1h_tokens",
        "cache_read_tokens",
        "cost_usd",
        "unpriced_requests",
        "price_table",
        "models",
    ];

    fn cells(&self) -> Vec<Cell<'_>> {
        let mut cells = vec![plain(&self.date), plain(&self.requests)];
        cells.extend(total_cells(&self.tokens, &self.cost));
        cells.push(Cell::Text(Cow::Owned(self.models.join(";"))));
        cells
    }
}

/// One week or calendar month.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PeriodRow {
    pub first_day: String,
    pub last_day: String,
    pub requests: u64,
    pub tokens: TokensJson,
    pub cost: CostJson,
    pub models: Vec<String>,
}

impl PeriodRow {
    fn of(app: &App, bucket: Bucket, period: &DayTotals) -> Self {
        Self {
            first_day: period.day.to_string(),
            last_day: bucket.end(period.day).to_string(),
            requests: period.totals.requests,
            tokens: TokensJson::of(&period.totals),
            cost: CostJson::of(app, &period.totals),
            models: period
                .models
                .iter()
                .map(|model| model.model.clone())
                .collect(),
        }
    }
}

impl Row for PeriodRow {
    const COLUMNS: &'static [&'static str] = &[
        "first_day",
        "last_day",
        "requests",
        "input_tokens",
        "output_tokens",
        "cache_write_5m_tokens",
        "cache_write_1h_tokens",
        "cache_read_tokens",
        "cost_usd",
        "unpriced_requests",
        "price_table",
        "models",
    ];

    fn cells(&self) -> Vec<Cell<'_>> {
        let mut cells = vec![
            plain(&self.first_day),
            plain(&self.last_day),
            plain(&self.requests),
        ];
        cells.extend(total_cells(&self.tokens, &self.cost));
        cells.push(Cell::Text(Cow::Owned(self.models.join(";"))));
        cells
    }
}

/// One model.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct ModelRow {
    model: String,
    name: Option<String>,
    requests: u64,
    tokens: TokensJson,
    cost: CostJson,
}

impl Row for ModelRow {
    const COLUMNS: &'static [&'static str] = &[
        "model",
        "requests",
        "input_tokens",
        "output_tokens",
        "cache_write_5m_tokens",
        "cache_write_1h_tokens",
        "cache_read_tokens",
        "cost_usd",
        "unpriced_requests",
        "price_table",
        "name",
    ];

    fn cells(&self) -> Vec<Cell<'_>> {
        let mut cells = vec![
            Cell::Text(Cow::Borrowed(&self.model)),
            plain(&self.requests),
        ];
        cells.extend(total_cells(&self.tokens, &self.cost));
        cells.push(text(self.name.as_deref()));
        cells
    }
}

/// One project.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct ProjectRow {
    name: String,
    path: Option<String>,
    transcript_folder: String,
    sessions: u64,
    requests: u64,
    tokens: TokensJson,
    cost: CostJson,
    last_request_at: String,
}

impl Row for ProjectRow {
    const COLUMNS: &'static [&'static str] = &[
        "name",
        "path",
        "sessions",
        "requests",
        "output_tokens",
        "cache_read_tokens",
        "cost_usd",
        "unpriced_requests",
        "input_tokens",
        "cache_write_5m_tokens",
        "cache_write_1h_tokens",
        "price_table",
        "last_request_at",
        "transcript_folder",
    ];

    fn cells(&self) -> Vec<Cell<'_>> {
        vec![
            Cell::Text(Cow::Borrowed(&self.name)),
            text(self.path.as_deref()),
            plain(&self.sessions),
            plain(&self.requests),
            plain(&self.tokens.output),
            plain(&self.tokens.cache_read),
            usd(self.cost.usd),
            plain(&self.cost.unpriced_requests),
            plain(&self.tokens.input),
            plain(&self.tokens.cache_write_5m),
            plain(&self.tokens.cache_write_1h),
            plain(&self.cost.price_table),
            plain(&self.last_request_at),
            Cell::Text(Cow::Borrowed(&self.transcript_folder)),
        ]
    }
}

/// One session.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct SessionRow {
    session_id: String,
    title: Option<String>,
    project: String,
    first_request_at: String,
    last_request_at: String,
    requests: u64,
    subagent_requests: u64,
    tokens: TokensJson,
    cost: CostJson,
    models: Vec<String>,
    subagents: u64,
    /// The cost Claude Code reports for the session, once it has written one.
    claude_code_cost_usd: Option<f64>,
}

impl SessionRow {
    fn of(app: &App, session: &requests::Matching) -> Self {
        let row = &session.row;
        let totals = &row.totals;
        let tokens = TokensJson {
            input: totals.tokens.input,
            output: totals.tokens.output,
            cache_write_5m: totals.tokens.cache_write_5m,
            cache_write_1h: totals.tokens.cache_write_1h,
            cache_read: totals.tokens.cache_read,
        };
        Self {
            session_id: row.id.clone(),
            title: row.title.clone(),
            project: row.project.name.clone(),
            first_request_at: at(row.first_ms),
            last_request_at: at(row.last_ms),
            requests: totals.requests,
            subagent_requests: session.subagent_requests,
            tokens,
            cost: CostJson::new(app, totals.cost.total, totals.unpriced_requests),
            models: row.models.iter().map(|model| model.id.clone()).collect(),
            subagents: row.subagents,
            claude_code_cost_usd: row.reported_usd.map(cents),
        }
    }
}

impl Row for SessionRow {
    const COLUMNS: &'static [&'static str] = &[
        "session_id",
        "title",
        "project",
        "first_request_at",
        "last_request_at",
        "requests",
        "subagent_requests",
        "output_tokens",
        "cache_read_tokens",
        "cost_usd",
        "unpriced_requests",
        "input_tokens",
        "cache_write_5m_tokens",
        "cache_write_1h_tokens",
        "price_table",
        "models",
        "subagents",
        "claude_code_cost_usd",
    ];

    fn cells(&self) -> Vec<Cell<'_>> {
        vec![
            Cell::Text(Cow::Borrowed(&self.session_id)),
            text(self.title.as_deref()),
            Cell::Text(Cow::Borrowed(&self.project)),
            plain(&self.first_request_at),
            plain(&self.last_request_at),
            plain(&self.requests),
            plain(&self.subagent_requests),
            plain(&self.tokens.output),
            plain(&self.tokens.cache_read),
            usd(self.cost.usd),
            plain(&self.cost.unpriced_requests),
            plain(&self.tokens.input),
            plain(&self.tokens.cache_write_5m),
            plain(&self.tokens.cache_write_1h),
            plain(&self.cost.price_table),
            Cell::Text(Cow::Owned(self.models.join(";"))),
            plain(&self.subagents),
            self.claude_code_cost_usd
                .map_or(Cell::Plain(String::new()), usd),
        ]
    }
}

/// One five-hour window.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WindowRow {
    pub start: String,
    pub resets_at: String,
    /// The largest share of the limit a reading saw, in percent.
    pub highest_percentage: f64,
    pub requests: u64,
    pub cost: CostJson,
    pub tokens: TokensJson,
}

impl WindowRow {
    #[must_use]
    pub fn of(app: &App, window: &FiveHourWindow) -> Self {
        let totals = &window.totals;
        Self {
            start: at(window.start_ms),
            resets_at: at(window.resets_at_ms),
            highest_percentage: window.highest,
            requests: totals.requests,
            cost: CostJson::new(app, totals.cost.total, totals.unpriced_requests),
            tokens: TokensJson {
                input: totals.tokens.input,
                output: totals.tokens.output,
                cache_write_5m: totals.tokens.cache_write_5m,
                cache_write_1h: totals.tokens.cache_write_1h,
                cache_read: totals.tokens.cache_read,
            },
        }
    }
}

impl Row for WindowRow {
    const COLUMNS: &'static [&'static str] = &[
        "start",
        "resets_at",
        "highest_percentage",
        "requests",
        "input_tokens",
        "output_tokens",
        "cache_write_5m_tokens",
        "cache_write_1h_tokens",
        "cache_read_tokens",
        "cost_usd",
        "unpriced_requests",
        "price_table",
    ];

    fn cells(&self) -> Vec<Cell<'_>> {
        let mut cells = vec![
            plain(&self.start),
            plain(&self.resets_at),
            plain(&self.highest_percentage),
            plain(&self.requests),
        ];
        cells.extend(total_cells(&self.tokens, &self.cost));
        cells
    }
}

/// One request's API-equivalent cost, unrounded; `None` for an unpriced model.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct RequestCostJson<'a> {
    usd: Option<f64>,
    provenance: &'static str,
    price_table: &'a str,
}

/// One request.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct RequestRow<'a> {
    at: String,
    session_id: &'a str,
    title: Option<&'a str>,
    project: &'a str,
    /// The subagent that made it; `None` for the session itself.
    subagent_id: Option<&'a str>,
    /// The kind of agent that subagent is, such as `Explore`.
    agent_type: Option<&'a str>,
    model: &'a str,
    tokens: TokensJson,
    web_search_requests: u64,
    web_fetch_requests: u64,
    speed: Option<&'a str>,
    inference_geo: Option<&'a str>,
    skill: Option<&'a str>,
    mcp_server: Option<&'a str>,
    mcp_tool: Option<&'a str>,
    entrypoint: Option<&'a str>,
    claude_code_version: Option<&'a str>,
    cost: RequestCostJson<'a>,
}

impl Row for RequestRow<'_> {
    const COLUMNS: &'static [&'static str] = &[
        "at",
        "session_id",
        "title",
        "project",
        "subagent_id",
        "agent_type",
        "model",
        "input_tokens",
        "output_tokens",
        "cache_write_5m_tokens",
        "cache_write_1h_tokens",
        "cache_read_tokens",
        "web_search_requests",
        "web_fetch_requests",
        "speed",
        "inference_geo",
        "skill",
        "mcp_server",
        "mcp_tool",
        "entrypoint",
        "claude_code_version",
        "cost_usd",
        "price_table",
    ];

    fn cells(&self) -> Vec<Cell<'_>> {
        vec![
            plain(&self.at),
            Cell::Text(Cow::Borrowed(self.session_id)),
            text(self.title),
            Cell::Text(Cow::Borrowed(self.project)),
            text(self.subagent_id),
            text(self.agent_type),
            Cell::Text(Cow::Borrowed(self.model)),
            plain(&self.tokens.input),
            plain(&self.tokens.output),
            plain(&self.tokens.cache_write_5m),
            plain(&self.tokens.cache_write_1h),
            plain(&self.tokens.cache_read),
            plain(&self.web_search_requests),
            plain(&self.web_fetch_requests),
            text(self.speed),
            text(self.inference_geo),
            text(self.skill),
            text(self.mcp_server),
            text(self.mcp_tool),
            text(self.entrypoint),
            text(self.claude_code_version),
            self.cost
                .usd
                .map_or(Cell::Plain(String::new()), |usd| plain(&usd)),
            plain(&self.cost.price_table),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(cells: &[Cell<'_>], saved: bool) -> String {
        let mut written = Vec::new();
        Out {
            writer: &mut written,
            saved,
            format: ExportFormat::Csv,
        }
        .record(cells)
        .unwrap();
        String::from_utf8(written).unwrap()
    }

    #[test]
    fn a_list_of_model_ids_is_text_like_any_other() {
        let models = Cell::Text(Cow::Owned(
            ["=cmd|' /C calc'!A0,\"x\"", "claude-opus-5-5"].join(";"),
        ));
        assert_eq!(
            record(&[plain(&2), models], true),
            "2,\"'=cmd|' /C calc'!A0,\"\"x\"\";claude-opus-5-5\"\n"
        );
    }

    #[test]
    fn text_is_quoted_only_when_it_needs_to_be() {
        let cells = [
            Cell::Text(Cow::Borrowed("plain")),
            Cell::Text(Cow::Borrowed("a, b")),
            Cell::Text(Cow::Borrowed("say \"hi\"")),
            Cell::Text(Cow::Borrowed("one\r\ntwo")),
        ];
        assert_eq!(
            record(&cells, false),
            "plain,\"a, b\",\"say \"\"hi\"\"\",\"one\r\ntwo\"\n"
        );
    }
}
