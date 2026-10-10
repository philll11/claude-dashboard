//! The messages between `claude-dashboard serve` and its clients: the parameters and results of
//! each JSON-RPC request, and the notifications the server sends.
//!
//! ts-rs writes their TypeScript into `web/protocol/src/generated/protocol.ts` when the tests run;
//! CI fails when that file is stale. Times are milliseconds since the Unix epoch, and days are
//! `YYYY-MM-DD` in the time zone the server reports.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The days a request covers, both included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Days {
    pub first: String,
    pub last: String,
}

/// Which requests to count, besides the days.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "protocol.ts")]
pub struct Filters {
    /// A project's id.
    #[ts(optional = nullable)]
    pub project: Option<String>,
    /// A model id, as Claude Code records it.
    #[ts(optional = nullable)]
    pub model: Option<String>,
}

/// Tokens by kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
}

/// API-equivalent cost in US dollars, by what it pays for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Cost {
    pub total: f64,
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
    pub web_search: f64,
}

/// Requests added up.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Totals {
    pub requests: u64,
    pub tokens: Tokens,
    /// The API-equivalent cost of the priced requests.
    pub cost: Cost,
    /// Requests to models the price table does not list, whose cost is unknown.
    pub unpriced_requests: u64,
    pub web_search_requests: u64,
    pub fast_requests: u64,
}

/// A model, by its id and the short name the price table gives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Model {
    pub id: String,
    /// Such as `Opus 5.5`; the id when the price table does not list the model.
    pub name: String,
}

/// A project, by its id and the name reports give it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Project {
    /// The folder that holds its transcripts, which identifies it.
    pub id: String,
    /// The last part of its working folder, or the last two when another project shares it.
    pub name: String,
    /// Its working folder, once a transcript has named it.
    #[ts(optional = nullable)]
    pub path: Option<String>,
}

/// Where a number comes from (ADR 0005).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub enum Provenance {
    /// Claude Code's status line.
    Official,
    /// Anthropic's undocumented usage endpoint.
    Experimental,
}

/// How close a plan window is to its limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub enum Level {
    Normal,
    /// From 70%.
    Warning,
    /// From 90%.
    Critical,
}

/// The latest reading of one plan window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct WindowReading {
    /// `five_hour`, `seven_day`, `seven_day:<model>` for one model's weekly window, or another
    /// window Claude Code reports, such as `spend_limit`.
    pub window: String,
    /// The model whose weekly window this is.
    #[ts(optional = nullable)]
    pub model: Option<String>,
    pub used_percentage: f64,
    #[ts(optional = nullable)]
    pub resets_at_ms: Option<i64>,
    pub observed_ms: i64,
    pub provenance: Provenance,
    pub level: Level,
    /// Older than an hour.
    pub stale: bool,
    /// The window has reset since the reading, so the reading no longer tells its use.
    pub expired: bool,
}

/// How far a read of new transcript content has got. The `ingest/progress` notification carries
/// it while a read goes on, and once more with every transcript done when the read ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Progress {
    pub done: u64,
    pub total: u64,
}

/// What the server can do with the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub enum Access {
    ReadWrite,
    /// A newer claude-dashboard has upgraded the history, so this one only reads it.
    ReadOnly,
}

/// `status`: what every view's header, the status bar and the tree show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Status {
    pub version: String,
    /// The IANA time zone that days are counted in.
    pub time_zone: String,
    pub today: String,
    /// The first day the history holds requests for.
    #[ts(optional = nullable)]
    pub first_day: Option<String>,
    /// When this server last finished reading new transcript content.
    #[ts(optional = nullable)]
    pub read_ms: Option<i64>,
    /// Set while a read is under way.
    #[ts(optional = nullable)]
    pub reading: Option<Progress>,
    pub access: Access,
    pub limits: Vec<WindowReading>,
    pub today_totals: Totals,
    pub week_totals: Totals,
    /// The first day of the week the person counts from.
    pub week_start: String,
}

/// `overview` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct OverviewParams {
    pub days: Days,
    #[serde(default)]
    pub filters: Filters,
}

/// One model's part of a day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct ModelDay {
    pub model: String,
    pub totals: Totals,
}

/// One day's totals, with each model's part, highest cost first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Day {
    pub date: String,
    pub totals: Totals,
    pub models: Vec<ModelDay>,
}

/// One model's totals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct ModelRow {
    pub model: Model,
    pub totals: Totals,
}

/// One project's cost on one day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct DayCost {
    pub date: String,
    pub usd: f64,
}

/// One project's totals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct ProjectRow {
    pub project: Project,
    pub sessions: u64,
    pub last_ms: i64,
    pub totals: Totals,
    /// Its API-equivalent cost on each day it has priced requests.
    pub daily_cost: Vec<DayCost>,
}

/// `overview`: the requests of some days added up in every way the dashboard shows them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Overview {
    pub days: Days,
    pub totals: Totals,
    /// Days with requests, oldest first.
    pub by_day: Vec<Day>,
    /// Highest cost first.
    pub models: Vec<ModelRow>,
    /// Highest cost first.
    pub projects: Vec<ProjectRow>,
}

/// `sessions` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct SessionsParams {
    pub days: Days,
    #[serde(default)]
    pub filters: Filters,
    /// Text the session's title, id or project name contains, in any case.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub search: Option<String>,
    #[serde(default)]
    pub offset: u32,
    pub limit: u32,
}

/// One session's totals over the days asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct SessionRow {
    /// Claude Code's session id.
    pub id: String,
    #[ts(optional = nullable)]
    pub title: Option<String>,
    pub project: Project,
    pub first_ms: i64,
    pub last_ms: i64,
    /// Highest cost first.
    pub models: Vec<Model>,
    pub subagents: u64,
    pub totals: Totals,
    /// The cost Claude Code reports for the session, once it has written one.
    #[ts(optional = nullable)]
    pub reported_usd: Option<f64>,
}

/// `sessions`: one page of sessions, the one started last first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct SessionsPage {
    /// Sessions that match, on every page.
    pub total: u64,
    pub rows: Vec<SessionRow>,
}

/// `session` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct SessionParams {
    /// Claude Code's session id.
    pub id: String,
}

/// The session itself, or one of its subagents, on one model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct WhoRow {
    /// The subagent's id; `None` for the session itself.
    #[ts(optional = nullable)]
    pub subagent: Option<String>,
    /// The kind of agent the subagent is, such as `Explore`, where Claude Code recorded it.
    #[ts(optional = nullable)]
    pub agent_type: Option<String>,
    pub model: Model,
    pub first_ms: i64,
    pub last_ms: i64,
    pub totals: Totals,
}

/// Requests credited to a skill, or to an MCP server and tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Credit {
    pub name: String,
    #[ts(optional = nullable)]
    pub tool: Option<String>,
    pub totals: Totals,
}

/// The running API-equivalent cost at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct CostPoint {
    pub ms: i64,
    pub usd: f64,
}

/// `session`: everything about one session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct SessionDetail {
    pub id: String,
    #[ts(optional = nullable)]
    pub title: Option<String>,
    pub project: Project,
    /// The session's transcript, while Claude Code keeps it.
    #[ts(optional = nullable)]
    pub transcript: Option<String>,
    pub first_ms: i64,
    pub last_ms: i64,
    #[ts(optional = nullable)]
    pub claude_code_version: Option<String>,
    /// How Claude Code was started, such as `cli` or `claude-vscode`.
    #[ts(optional = nullable)]
    pub entrypoint: Option<String>,
    pub totals: Totals,
    /// The session's own requests, without its subagents'.
    pub own: Totals,
    #[ts(optional = nullable)]
    pub reported_usd: Option<f64>,
    pub cumulative_cost: Vec<CostPoint>,
    /// The session's own models first, then each subagent's, in the order they started.
    pub who: Vec<WhoRow>,
    pub skills: Vec<Credit>,
    pub mcp_tools: Vec<Credit>,
    pub uncredited: Totals,
}

/// Where a window is heading at its current pace.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Pace {
    /// The share of the limit used when the window resets.
    pub at_reset: f64,
    /// When the limit is reached, if before the window resets.
    #[ts(optional = nullable)]
    pub limit_reached_ms: Option<i64>,
    /// How far back the pace is measured from.
    pub since_ms: i64,
}

/// One reading in a window's history.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct UsePoint {
    pub ms: i64,
    pub used: f64,
}

/// A plan window, with its readings since it started and its pace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct WindowView {
    pub reading: WindowReading,
    #[ts(optional = nullable)]
    pub start_ms: Option<i64>,
    pub series: Vec<UsePoint>,
    #[ts(optional = nullable)]
    pub pace: Option<Pace>,
}

/// One past five-hour window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct FiveHourWindow {
    pub start_ms: i64,
    pub resets_at_ms: i64,
    /// The largest share of the limit a reading saw.
    pub highest: f64,
    pub totals: Totals,
}

/// The status line's connection to Claude Code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "state")]
#[ts(export, export_to = "protocol.ts")]
pub enum StatusLine {
    Connected {
        /// The person's own status line command, which claude-dashboard's runs.
        #[ts(optional = nullable)]
        wrapping: Option<String>,
    },
    NotConnected,
    /// The settings could not be read.
    Unknown {
        reason: String,
    },
}

/// The usage endpoint's state, and how asking it last went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Endpoint {
    pub enabled: bool,
    #[ts(optional = nullable)]
    pub asked_ms: Option<i64>,
    /// `answered`, `no-login`, `login-expired`, `login-refused`, `rate-limited`, `unreadable` or
    /// `failed`.
    #[ts(optional = nullable)]
    pub outcome: Option<String>,
    #[ts(optional = nullable)]
    pub detail: Option<String>,
    pub next_ask_ms: i64,
    /// What went wrong the last time it was asked, as a sentence, while it is on.
    #[ts(optional = nullable)]
    pub problem: Option<String>,
    /// What turning it on means, which the person agrees to first.
    pub consent: Vec<String>,
}

/// `limits`: every plan window, where the readings come from, and recent five-hour windows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Limits {
    pub windows: Vec<WindowView>,
    /// Newest first.
    pub recent: Vec<FiveHourWindow>,
    pub status_line: StatusLine,
    pub endpoint: Endpoint,
}

/// A model's prices, in US dollars per million tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct PriceRow {
    pub model: Model,
    pub input: f64,
    pub output: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
    pub cache_read: f64,
    /// The prices of prompts longer than a number of tokens, for a model that charges more for
    /// them.
    #[ts(optional = nullable)]
    pub long_prompt: Option<LongPrompt>,
}

/// A model's prices for long prompts, in US dollars per million tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct LongPrompt {
    /// Prompts of more tokens than this pay these prices.
    pub above: u64,
    pub input: f64,
    pub output: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
    pub cache_read: f64,
}

/// A model whose API-equivalent cost drifts from Claude Code's own figure by more than rounding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct PriceDrift {
    pub model: Model,
    /// The estimate against Claude Code's figure, as a share: -0.06 is 6% below it.
    pub drift: f64,
    pub sessions: u64,
}

/// `prices`: the price table and how well it agrees with Claude Code's own costs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Prices {
    /// The day the table was last compared with the pricing page.
    pub checked: String,
    pub source: String,
    pub models: Vec<PriceRow>,
    /// The estimate against Claude Code's own figure, over the sessions that can be compared, as a
    /// share: 0.004 is 0.4% above it.
    #[ts(optional = nullable)]
    pub drift: Option<f64>,
    pub sessions_compared: u64,
    /// The models whose prices may be out of date.
    pub out_of_line: Vec<PriceDrift>,
}

/// Something the person needs to know about the data folders or the history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "state")]
#[ts(export, export_to = "protocol.ts")]
pub enum FolderState {
    Found { transcripts: u64, bytes: u64 },
    Missing,
    Unreachable { reason: String },
    Unreadable { reason: String },
}

/// A data folder and what it holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Folder {
    pub path: String,
    /// `default`, `environment` or `added`.
    pub source: String,
    pub state: FolderState,
}

/// `health`: what the error and empty states show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Health {
    pub folders: Vec<Folder>,
    #[ts(optional = nullable)]
    pub history_path: Option<String>,
    /// What the history holds, once it can be read.
    #[ts(optional = nullable)]
    pub history: Option<HistoryFacts>,
    /// How many days Claude Code keeps transcripts for.
    pub cleanup_days: u32,
    pub access: Access,
    /// The schema that made the history read-only.
    #[ts(optional = nullable)]
    pub newer_schema: Option<u32>,
    /// What went wrong reading files during the last ingest.
    pub problems: Vec<String>,
}

/// What the history holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct HistoryFacts {
    pub bytes: u64,
    pub requests: u64,
    pub sessions: u64,
    /// The import from the claude-usage dashboard, once one ran.
    #[ts(optional = nullable)]
    pub imported: Option<Imported>,
}

/// An import that ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Imported {
    pub requests: u64,
    pub imported_ms: i64,
}

/// `settings`: the settings every surface shares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Settings {
    /// The IANA time zone days are counted in, when one is chosen; `None` for this computer's.
    #[ts(optional = nullable)]
    pub time_zone: Option<String>,
    /// This computer's IANA time zone.
    pub system_time_zone: String,
    /// The first day of the week, such as `monday`.
    pub week_start: String,
}

/// `settings/set` parameters: what to change; whatever is left out stays as it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct SettingsChange {
    /// An IANA time zone, or `""` for this computer's.
    #[serde(default)]
    #[ts(optional)]
    pub time_zone: Option<String>,
    /// A day of the week, such as `sunday`.
    #[serde(default)]
    #[ts(optional)]
    pub week_start: Option<String>,
}

/// `folders/add` and `folders/remove` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct FolderParams {
    pub path: String,
}

/// `import/offer`: what the claude-usage dashboard's database would add to the history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct ImportOffer {
    pub path: String,
    /// Requests the history does not hold yet.
    pub requests: u64,
    #[ts(optional = nullable)]
    pub first_day: Option<String>,
    #[ts(optional = nullable)]
    pub last_day: Option<String>,
    /// Whether the person chose not to import it.
    pub dismissed: bool,
}

/// `import/run`: what the import did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct ImportDone {
    pub imported: u64,
    pub already_held: u64,
    pub unreadable: u64,
}

/// The `history/changed` notification: new requests arrived for these days.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct HistoryChanged {
    #[ts(optional = nullable)]
    pub days: Option<Days>,
}

/// `choices`: every project and model the history holds, for the filters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct Choices {
    /// By name.
    pub projects: Vec<Project>,
    /// By name.
    pub models: Vec<Model>,
}

/// `endpoint/set` parameters: turning Anthropic's usage endpoint on or off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct EndpointParams {
    pub enabled: bool,
}

/// `statusLine/plan` parameters: connecting claude-dashboard's status line, or disconnecting it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct StatusLineParams {
    pub connect: bool,
}

/// `statusLine/plan`: what connecting or disconnecting would do to Claude Code's settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(export, export_to = "protocol.ts")]
pub enum StatusLinePlan {
    /// A change to the `statusLine` setting, shown before it is made.
    Change {
        /// Claude Code's settings.json.
        path: String,
        /// The setting as it is, as one line of JSON members; `None` when there is none.
        before: Option<String>,
        /// The setting the change leaves; `None` when the change removes it.
        after: Option<String>,
        /// Whether settings.json exists, so that a backup of it is kept.
        exists: bool,
        /// What changes for the person once the change is made.
        effect: String,
    },
    /// Nothing needs changing.
    Nothing { reason: String },
    /// The change cannot be made.
    Refused { reason: String },
}

/// `statusLine/apply` parameters: the change as the person saw it, which is made only while it
/// still stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct StatusLineApply {
    pub connect: bool,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// `prices` parameters: the models whose prices to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "protocol.ts")]
pub struct PricesParams {
    pub models: Vec<String>,
}
