//! `claude-dashboard limits`: each plan window and the recent five-hour windows; turning
//! Anthropic's usage endpoint on or off; and what `doctor` says about it.

use std::fmt::Write as _;
use std::process::ExitCode;

use claude_dashboard_core::history::History;
use claude_dashboard_core::usage_endpoint::{self, Outcome, Refresh, State};
use claude_dashboard_server::app::App;
use claude_dashboard_server::fmt::{ago, clock, day_label, money, percent, resets};
use claude_dashboard_server::limits::{CONSENT, login_note, sentence, trouble, view};
use claude_dashboard_server::protocol::{
    FiveHourWindow, Level, Limits, Provenance, WindowReading, WindowView,
};
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use serde::Serialize;

use crate::prompt;
use crate::report::{CostJson, Output, limits_line, print_json, ready_history};
use crate::style::Paint;

/// Each plan window with its use, reset, source and age, and the recent five-hour windows, after
/// giving the usage endpoint its turn.
pub fn report(app: &App, output: Output) -> ExitCode {
    let mut history = match ready_history(app) {
        Ok(history) => history,
        Err(code) => return code,
    };
    refresh(app, &mut history);
    let limits = match view(app, &history) {
        Ok(limits) => limits,
        Err(error) => {
            eprintln!("Cannot read the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    let windows = in_order(&limits.windows);
    match output {
        Output::Table => {
            let paint = Paint::for_stdout();
            let mut out = limits_table(app, paint, &limits, &windows);
            let endpoint = history.usage_endpoint().unwrap_or_default();
            if let Some(note) = login_note(&endpoint) {
                let _ = writeln!(
                    out,
                    "\n{}",
                    paint.paint(note, paint.palette().warning, false)
                );
            }
            print!("{out}");
        }
        Output::Json => print_json(&LimitsJson {
            windows: windows.iter().map(|window| window_json(window)).collect(),
            recent_five_hour_windows: limits
                .recent
                .iter()
                .map(|window| recent_json(app, window))
                .collect(),
        }),
        Output::Csv => {
            let mut out = String::from(
                "window,model,used_percentage,resets_at,observed_at,provenance,stale,expired\n",
            );
            for window in &windows {
                let reading = &window.reading;
                let _ = writeln!(
                    out,
                    "{},{},{},{},{},{},{},{}",
                    reading.window,
                    reading.model.as_deref().unwrap_or_default(),
                    reading.used_percentage,
                    reading.resets_at_ms.and_then(at).unwrap_or_default(),
                    at(reading.observed_ms).unwrap_or_default(),
                    provenance(reading.provenance),
                    reading.stale,
                    reading.expired
                );
            }
            print!("{out}");
        }
    }
    ExitCode::SUCCESS
}

/// The windows in the order every surface lists them: five-hour first, then weekly, then each
/// model's weekly window, then any other.
fn in_order(windows: &[WindowView]) -> Vec<&WindowView> {
    let rank = |reading: &WindowReading| match reading.window.as_str() {
        "five_hour" => 0,
        "seven_day" => 1,
        _ if reading.model.is_some() => 2,
        _ => 3,
    };
    let mut ordered: Vec<&WindowView> = windows.iter().collect();
    ordered.sort_by(|a, b| {
        rank(&a.reading).cmp(&rank(&b.reading)).then_with(|| {
            let key = |reading: &WindowReading| {
                reading
                    .model
                    .clone()
                    .unwrap_or_else(|| reading.window.clone())
            };
            key(&a.reading).cmp(&key(&b.reading))
        })
    });
    ordered
}

/// What a window is called: `Five-hour window`, `Weekly, all models`, `Weekly, Fable`.
pub fn window_name(reading: &WindowReading) -> String {
    match (reading.window.as_str(), &reading.model) {
        ("five_hour", _) => "Five-hour window".to_owned(),
        ("seven_day", _) => "Weekly, all models".to_owned(),
        (_, Some(model)) => format!("Weekly, {model}"),
        ("spend_limit", None) => "Spend limit".to_owned(),
        (window, None) => window.to_owned(),
    }
}

fn provenance(provenance: Provenance) -> &'static str {
    match provenance {
        Provenance::Official => "official",
        Provenance::Experimental => "experimental",
    }
}

fn at(ms: i64) -> Option<String> {
    Timestamp::from_millisecond(ms)
        .ok()
        .map(|at| at.to_string())
}

fn zoned(app: &App, ms: i64) -> Option<Zoned> {
    Timestamp::from_millisecond(ms)
        .ok()
        .map(|at| at.to_zoned(app.zone.clone()))
}

/// A five-hour window's span: `Sat 10 Oct 09:20–14:20`, or across midnight
/// `Fri 9 Oct 22:00–Sat 10 Oct 03:00`.
pub fn five_hour_span(zone: &TimeZone, window: &FiveHourWindow) -> String {
    let at = |ms: i64| {
        Timestamp::from_millisecond(ms)
            .ok()
            .map(|at| at.to_zoned(zone.clone()))
    };
    let (Some(start), Some(end)) = (at(window.start_ms), at(window.resets_at_ms)) else {
        return String::new();
    };
    let end = if end.date() == start.date() {
        clock(&end)
    } else {
        format!("{} {}", day_label(end.date()), clock(&end))
    };
    format!("{} {}–{end}", day_label(start.date()), clock(&start))
}

fn limits_table(app: &App, paint: Paint, limits: &Limits, windows: &[&WindowView]) -> String {
    if windows.is_empty() {
        return format!("{}\n", limits_line(app, paint, &[]));
    }
    let mut out = format!(
        "{}\n\n",
        paint.muted("Plan limits · shared between Claude Code and claude.ai")
    );
    out.push_str(&windows_table(app, paint, windows));
    if !limits.recent.is_empty() {
        out.push('\n');
        out.push_str(&recent_table(app, paint, &limits.recent));
    }
    out
}

/// Each window with its use, reset, source and the age of its reading.
fn windows_table(app: &App, paint: Paint, windows: &[&WindowView]) -> String {
    let mut out = String::new();
    let palette = paint.palette();
    let now = app.now_zoned();
    let rows: Vec<[String; 4]> = windows
        .iter()
        .map(|window| {
            let reading = &window.reading;
            let resets = if reading.expired {
                "no reading since it reset".to_owned()
            } else {
                reading
                    .resets_at_ms
                    .and_then(|ms| zoned(app, ms))
                    .map_or_else(
                        || "no reset time".to_owned(),
                        |at| {
                            let text = resets(&at, &now);
                            text.strip_prefix("Resets ").unwrap_or(&text).to_owned()
                        },
                    )
            };
            let age = Timestamp::from_millisecond(reading.observed_ms)
                .map(|observed| ago(observed, app.now))
                .unwrap_or_default();
            [
                window_name(reading),
                resets,
                provenance(reading.provenance).to_owned(),
                age,
            ]
        })
        .collect();
    let width = |column: usize, heading: &str| {
        rows.iter()
            .map(|row| row[column].chars().count())
            .chain([heading.chars().count()])
            .max()
            .unwrap_or(0)
    };
    let (name_width, resets_width, source_width) =
        (width(0, "Window"), width(1, "Resets"), width(2, "Source"));
    let _ = writeln!(
        out,
        "{}",
        paint.muted(&format!(
            "{:<name_width$}   {:>4}     {:<resets_width$}   {:<source_width$}   Reading",
            "Window", "Used", "Resets", "Source"
        ))
    );
    for (window, [name, resets, source, age]) in windows.iter().zip(&rows) {
        let reading = &window.reading;
        let (used, marker) = if reading.expired {
            (paint.muted(&format!("{:>4}", "–")), "  ".to_owned())
        } else {
            let figure = format!("{:>4}", percent(reading.used_percentage));
            match reading.level {
                Level::Normal => (paint.strong(&figure), "  ".to_owned()),
                Level::Warning => (
                    paint.paint(&figure, palette.warning, true),
                    format!(" {}", paint.paint("▲", palette.warning, true)),
                ),
                // Red passes WCAG AA only for marks, so the figure stays in strong ink.
                Level::Critical => (
                    paint.strong(&figure),
                    format!(" {}", paint.paint("▲", palette.critical, true)),
                ),
            }
        };
        let age = if reading.stale {
            paint.paint(age, palette.warning, false)
        } else {
            paint.muted(age)
        };
        let _ = writeln!(
            out,
            "{name:<name_width$}   {used}{marker}   {resets:<resets_width$}   {}   {age}",
            paint.muted(&format!("{source:<source_width$}")),
        );
    }
    out
}

/// The recent five-hour windows, with the highest use a reading saw and their cost.
fn recent_table(app: &App, paint: Paint, recent: &[FiveHourWindow]) -> String {
    let mut out = String::new();
    let palette = paint.palette();
    let spans: Vec<String> = recent
        .iter()
        .map(|window| {
            let running = window.start_ms <= app.now.as_millisecond()
                && app.now.as_millisecond() < window.resets_at_ms;
            format!(
                "{}{}",
                five_hour_span(&app.zone, window),
                if running { ", in progress" } else { "" }
            )
        })
        .collect();
    let span_width = spans
        .iter()
        .map(|span| span.chars().count())
        .chain(["Recent five-hour windows".chars().count()])
        .max()
        .unwrap_or(0);
    let costs: Vec<String> = recent
        .iter()
        .map(|window| {
            let totals = &window.totals;
            if totals.requests > 0 && totals.unpriced_requests == totals.requests {
                "unpriced".to_owned()
            } else {
                money(totals.cost.total)
            }
        })
        .collect();
    let cost_width = costs
        .iter()
        .map(|cost| cost.chars().count())
        .chain(["API-equivalent cost".chars().count()])
        .max()
        .unwrap_or(0);
    let _ = writeln!(
        out,
        "{}",
        paint.muted(&format!(
            "{:<span_width$}   {:>7}   {:>cost_width$}",
            "Recent five-hour windows", "Highest", "API-equivalent cost"
        ))
    );
    for ((window, span), cost) in recent.iter().zip(&spans).zip(&costs) {
        let highest = format!("{:>7}", percent(window.highest));
        let marker = if window.highest >= 90.0 {
            format!(" {}", paint.paint("▲", palette.critical, true))
        } else if window.highest >= 70.0 {
            format!(" {}", paint.paint("▲", palette.warning, true))
        } else {
            "  ".to_owned()
        };
        let _ = writeln!(
            out,
            "{span:<span_width$}   {highest}{marker} {}",
            paint.strong(&format!("{cost:>cost_width$}"))
        );
    }
    out
}

#[derive(Serialize)]
struct LimitsJson {
    windows: Vec<WindowJson>,
    recent_five_hour_windows: Vec<RecentJson>,
}

#[derive(Serialize)]
struct WindowJson {
    window: String,
    model: Option<String>,
    used_percentage: f64,
    resets_at: Option<String>,
    observed_at: Option<String>,
    provenance: &'static str,
    stale: bool,
    expired: bool,
    pace: Option<PaceJson>,
}

#[derive(Serialize)]
struct PaceJson {
    /// The share of the limit used when the window resets, at the pace since `since`.
    at_reset_percentage: f64,
    limit_reached_at: Option<String>,
    since: Option<String>,
}

#[derive(Serialize)]
struct RecentJson {
    start: Option<String>,
    resets_at: Option<String>,
    highest_percentage: f64,
    requests: u64,
    cost: CostJson,
}

fn window_json(window: &WindowView) -> WindowJson {
    let reading = &window.reading;
    WindowJson {
        window: reading.window.clone(),
        model: reading.model.clone(),
        used_percentage: reading.used_percentage,
        resets_at: reading.resets_at_ms.and_then(at),
        observed_at: at(reading.observed_ms),
        provenance: provenance(reading.provenance),
        stale: reading.stale,
        expired: reading.expired,
        pace: window.pace.map(|pace| PaceJson {
            at_reset_percentage: pace.at_reset,
            limit_reached_at: pace.limit_reached_ms.and_then(at),
            since: at(pace.since_ms),
        }),
    }
}

fn recent_json(app: &App, window: &FiveHourWindow) -> RecentJson {
    let totals = &window.totals;
    RecentJson {
        start: at(window.start_ms),
        resets_at: at(window.resets_at_ms),
        highest_percentage: window.highest,
        requests: totals.requests,
        cost: CostJson::of(app, totals.cost.total, totals.unpriced_requests),
    }
}

pub fn endpoint(app: &App, on: bool, yes: bool) -> ExitCode {
    let mut history = match app.open_history() {
        Ok(history) => history,
        Err(error) => {
            eprintln!("Cannot open the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    let enabled = match history.usage_endpoint() {
        Ok(state) => state.enabled,
        Err(error) => {
            eprintln!("Cannot read the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    let paint = Paint::for_stdout();
    if on == enabled {
        println!(
            "The usage endpoint is already {}.",
            if on { "on" } else { "off" }
        );
        return ExitCode::SUCCESS;
    }
    if on {
        println!("{}\n", paint.strong("Use Anthropic's usage endpoint?"));
        for sentence in CONSENT {
            println!("{sentence}");
        }
        if !prompt::confirmed(paint, yes, "Turn it on?") {
            return ExitCode::FAILURE;
        }
    }
    if let Err(error) = history.write(|w| w.set_usage_endpoint(on)) {
        eprintln!("Cannot record the change: {error}");
        return ExitCode::FAILURE;
    }
    if !on {
        println!("The usage endpoint is off.");
        return ExitCode::SUCCESS;
    }
    println!("\nThe usage endpoint is on. Turn it off with claude-dashboard limits endpoint off.");
    // Asking once now shows whether the endpoint works with this computer's login.
    let asked = matches!(refresh(app, &mut history), Some(Refresh::Asked(_)));
    let state = history.usage_endpoint().unwrap_or_default();
    if !asked {
        println!(
            "{}",
            paint.muted("It is asked whenever no plan-limit reading is newer than 5 minutes.")
        );
    } else if state.outcome == Some(Outcome::Answered) {
        let limits = history.latest_limits().unwrap_or_default();
        println!("{}", limits_line(app, paint, &limits));
    } else if let Some(note) = login_note(&state) {
        println!("{}", paint.paint(note, paint.palette().warning, false));
    } else if let Some(trouble) = trouble(app, &state) {
        println!(
            "{}",
            paint.paint(&sentence(&trouble), paint.palette().warning, false)
        );
    }
    ExitCode::SUCCESS
}

/// Asks the usage endpoint for the plan limits when it is on and its turn has come. What comes
/// of it is recorded in the history, so a report never fails over it.
pub fn refresh(app: &App, history: &mut History) -> Option<Refresh> {
    let login = app.login_source()?;
    usage_endpoint::refresh(history, &login, app.now.as_millisecond()).ok()
}

/// The `doctor` line for the endpoint while it is on: whether it works, and what to say.
pub fn doctor_detail(app: &App, state: &State) -> (bool, String) {
    match (state.outcome, state.asked_ms) {
        (Some(Outcome::Answered), Some(asked_ms)) => {
            let asked = Timestamp::from_millisecond(asked_ms)
                .map(|at| ago(at, app.now))
                .unwrap_or_default();
            (
                true,
                format!("usage endpoint on (experimental), last answer {asked}"),
            )
        }
        (Some(_), Some(_)) => (
            false,
            format!(
                "usage endpoint on (experimental): {}",
                trouble(app, state).unwrap_or_default()
            ),
        ),
        _ => (
            true,
            "usage endpoint on (experimental), no answer yet".to_owned(),
        ),
    }
}
