//! Plan limits: the Overview's box of meters, and the Limits section with each window's use and
//! pace, the recent five-hour windows, and where the readings come from.

use claude_dashboard_server::fmt::{ago, clock, day_label, money, percent, resets, when};
use claude_dashboard_server::protocol::{
    Endpoint, FiveHourWindow, Level, Limits, Provenance, StatusLine, WindowReading, WindowView,
};
use jiff::{Timestamp, Zoned};
use ratatui::style::Style;

use super::{KEYS, footer, header, notice};
use crate::limits::five_hour_span;
use crate::tui::canvas::{Canvas, clean, fit, spark_to, width};
use crate::tui::state::{Section, State};
use crate::tui::theme::Theme;

/// The windows in the order every surface lists them: five-hour first, then weekly, then each
/// model's weekly window, then any other.
pub fn ordered(readings: &[WindowReading]) -> Vec<&WindowReading> {
    let rank = |reading: &WindowReading| match reading.window.as_str() {
        "five_hour" => 0,
        "seven_day" => 1,
        _ if reading.model.is_some() => 2,
        _ => 3,
    };
    let mut ordered: Vec<&WindowReading> = readings.iter().collect();
    ordered.sort_by(|a, b| {
        rank(a).cmp(&rank(b)).then_with(|| {
            let key = |reading: &WindowReading| {
                reading
                    .model
                    .clone()
                    .unwrap_or_else(|| reading.window.clone())
            };
            key(a).cmp(&key(b))
        })
    });
    ordered
}

/// What a window is called in the Limits section: `Weekly, all models`.
fn name(reading: &WindowReading) -> String {
    claude_dashboard_server::glance::window_name(reading)
}

/// What a window is called in the Overview's narrower box: `Weekly, all`.
fn short_name(reading: &WindowReading) -> String {
    if reading.window == "seven_day" {
        "Weekly, all".to_owned()
    } else {
        clean(&name(reading))
    }
}

/// A meter's colour: the level's, or muted for an old reading.
pub fn meter_style(theme: Theme, reading: &WindowReading) -> Style {
    if reading.stale || reading.expired {
        theme.muted()
    } else {
        match reading.level {
            Level::Normal => theme.accent(),
            Level::Warning => theme.warn_text(),
            Level::Critical => theme.critical(),
        }
    }
}

/// The mark and words of a window over 70% or 90%, for a current reading.
fn level(theme: Theme, reading: &WindowReading) -> Option<(Style, &'static str)> {
    if reading.stale || reading.expired {
        return None;
    }
    match reading.level {
        Level::Normal => None,
        Level::Warning => Some((theme.warn(), "over 70%")),
        Level::Critical => Some((theme.critical(), "over 90%")),
    }
}

fn zoned(state: &State, ms: i64) -> Option<Zoned> {
    Timestamp::from_millisecond(ms)
        .ok()
        .map(|at| at.to_zoned(state.zone()))
}

/// Where the newest reading comes from, and how old it is: `official · status line · 2 min ago`;
/// with `▲` and in the warning colour when every reading is old.
fn provenance(state: &State, readings: &[&WindowReading], source: bool) -> (String, Style) {
    let theme = state.theme;
    let Some(newest) = readings.iter().max_by_key(|reading| reading.observed_ms) else {
        return (String::new(), theme.muted());
    };
    let age = Timestamp::from_millisecond(newest.observed_ms)
        .map(|at| ago(at, state.now))
        .unwrap_or_default();
    if readings
        .iter()
        .all(|reading| reading.stale || reading.expired)
    {
        return (format!("▲ last reading {age}"), theme.warn_text());
    }
    let from = match (newest.provenance, source) {
        (Provenance::Official, true) => "official · status line",
        (Provenance::Experimental, true) => "experimental · usage endpoint",
        (Provenance::Official, false) => "official",
        (Provenance::Experimental, false) => "experimental",
    };
    (format!("{from} · {age}"), theme.muted())
}

/// The Overview's box of plan limits: up to three windows, each with its meter and reset.
pub fn overview_box(c: &mut Canvas, state: &State, top: i32) {
    let theme = c.theme;
    let wide = state.wide();
    let Some(status) = state.status() else {
        c.boxed((0, top, c.w, 5), Some("Plan limits"), None);
        return;
    };
    let readings = ordered(&status.limits);
    if readings.is_empty() {
        not_recorded(c, state, top);
        return;
    }
    let (note, style) = provenance(state, &readings, true);
    c.boxed((0, top, c.w, 5), Some("Plan limits"), Some((&note, style)));
    let meter_length = if wide { 30 } else { 16 };
    let label_end = if wide { 22 } else { 18 };
    let now = state.now.to_zoned(state.zone());
    for (row, reading) in (0..).zip(readings.iter().take(3)) {
        let y = top + 1 + row;
        c.text(2, y, &short_name(reading), theme.text());
        let old = reading.stale || reading.expired;
        c.right(
            label_end + 4,
            y,
            &percent(reading.used_percentage),
            if old { theme.text() } else { theme.strong() },
        );
        c.meter(
            label_end + 6,
            y,
            meter_length,
            reading.used_percentage / 100.0,
            meter_style(theme, reading),
        );
        let mut x = label_end + 8 + meter_length;
        let level = level(theme, reading);
        if let Some((mark, _)) = level {
            x = c.text(x, y, "▲ ", mark);
        }
        let reset = reading.resets_at_ms.and_then(|ms| zoned(state, ms));
        let note = match (reset, reading.expired) {
            (Some(at), true) if wide => format!("ended {} {}", day_label(at.date()), clock(&at)),
            (Some(at), true) => format!(
                "ended {} {}",
                day_label(at.date()).split(' ').next().unwrap_or_default(),
                clock(&at)
            ),
            (Some(at), false) if wide => lower_first(&resets(&at, &now)),
            (Some(at), false) => when(&at, &now),
            (None, _) if wide => "no reset time".to_owned(),
            (None, _) => String::new(),
        };
        let note = match level {
            Some((_, words)) if note.is_empty() => words.to_owned(),
            Some((_, words)) => format!("{words} · {note}"),
            None => note,
        };
        c.text(
            x,
            y,
            &fit(&note, c.w - 2 - x),
            if level.is_some() {
                theme.text()
            } else {
                theme.muted()
            },
        );
    }
}

fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(chars).collect()
    })
}

/// The Overview's box while no plan limits are recorded: the commands that record them.
fn not_recorded(c: &mut Canvas, state: &State, top: i32) {
    c.boxed((0, top, c.w, 5), Some("Plan limits"), None);
    recording(c, state, top + 1);
}

/// How plan limits get recorded, from row `row`: whether the status line is connected, and the
/// commands that record them.
fn recording(c: &mut Canvas, state: &State, row: i32) {
    let theme = c.theme;
    let wide = state.wide();
    let Some(limits) = state.limits() else {
        return;
    };
    let pick = |long: &'static str, short: &'static str| if wide { long } else { short };
    let mut row = row;
    if matches!(limits.status_line, StatusLine::Connected { .. }) {
        c.text(
            2,
            row,
            pick(
                "Connected. Your windows appear here the next time Claude Code runs in a terminal.",
                "Connected. Your windows appear once Claude Code runs in a terminal.",
            ),
            theme.text(),
        );
        row += 1;
    } else {
        c.text(
            2,
            row,
            pick(
                "Not connected. Claude Code passes plan limits to its status line while it runs in a terminal.",
                "Not connected. Claude Code passes them to its status line as it runs.",
            ),
            theme.text(),
        );
        let x = c.text(
            2,
            row + 1,
            "claude-dashboard statusline connect",
            theme.accent(),
        );
        c.text(
            x + 3,
            row + 1,
            pick(
                "records them as Claude Code runs in a terminal",
                "records them as Claude Code runs",
            ),
            theme.muted(),
        );
        row += 2;
    }
    if !limits.endpoint.enabled {
        let x = c.text(
            2,
            row,
            "claude-dashboard limits endpoint on",
            theme.accent(),
        );
        c.text(
            x + 3,
            row,
            pick(
                "asks Anthropic when no session runs there (experimental)",
                "asks Anthropic (experimental)",
            ),
            theme.muted(),
        );
    }
}

/// The views of the windows, in order.
fn views(limits: &Limits) -> Vec<&WindowView> {
    let readings: Vec<WindowReading> = limits
        .windows
        .iter()
        .map(|view| view.reading.clone())
        .collect();
    ordered(&readings)
        .into_iter()
        .filter_map(|reading| {
            limits
                .windows
                .iter()
                .find(|view| view.reading.window == reading.window)
        })
        .collect()
}

/// A window's use from its start until now, in `cells` columns on a scale to 100%, in the
/// characters of `ramp`.
fn use_line(view: &WindowView, now_ms: i64, cells: i32, ramp: &[&str; 8]) -> String {
    let Some(start) = view.start_ms else {
        return String::new();
    };
    let end = now_ms
        .min(view.reading.resets_at_ms.unwrap_or(now_ms))
        .max(start + 1);
    let values: Vec<f64> = (0..cells)
        .map(|column| {
            let at = start + (end - start) * i64::from(column + 1) / i64::from(cells.max(1));
            view.series
                .iter()
                .take_while(|point| point.ms <= at)
                .last()
                .map_or(0.0, |point| point.used)
        })
        .collect();
    spark_to(&values, 100.0, ramp)
}

/// Where the pace measured from: `since 09:20` today, `since Thu 8 Oct` before.
fn since(state: &State, view: &WindowView) -> Option<String> {
    let ms = if view.reading.window == "five_hour" {
        view.start_ms?
    } else {
        view.pace.map_or(view.start_ms?, |pace| pace.since_ms)
    };
    let at = zoned(state, ms)?;
    let now = state.now.to_zoned(state.zone());
    Some(if at.date() == now.date() {
        format!("since {}", clock(&at))
    } else {
        format!("since {}", day_label(at.date()))
    })
}

/// Where the window is heading at its pace: `about 61% when it resets (estimate)`.
fn pace_text(state: &State, view: &WindowView) -> Option<String> {
    let pace = view.pace?;
    if view.reading.stale {
        return None;
    }
    Some(
        match pace.limit_reached_ms.and_then(|ms| zoned(state, ms)) {
            Some(at) => {
                let now = state.now.to_zoned(state.zone());
                if at.date() == now.date() {
                    format!(
                        "you reach the limit around {} (estimate)",
                        clock(&rounded(&at))
                    )
                } else {
                    format!(
                        "you reach the limit {} around {} (estimate)",
                        day_label(at.date()),
                        clock(&rounded(&at))
                    )
                }
            }
            None => format!("about {} when it resets (estimate)", percent(pace.at_reset)),
        },
    )
}

/// A moment to the nearest quarter hour, as estimates say it.
fn rounded(at: &Zoned) -> Zoned {
    let quarter = 15 * 60_000;
    let ms = at.timestamp().as_millisecond();
    let rounded = (ms + quarter / 2).div_euclid(quarter) * quarter;
    Timestamp::from_millisecond(rounded).map_or_else(
        |_| at.clone(),
        |stamp| stamp.to_zoned(at.time_zone().clone()),
    )
}

pub fn draw(c: &mut Canvas, state: &State) {
    let wide = state.wide();
    header(c, state, Some(Section::Limits));
    notice(c, state, "", false);
    if let Some(limits) = state.limits() {
        if wide {
            wide_layout(c, state, limits);
        } else {
            narrow_layout(c, state, limits);
        }
    }
    footer(c, state, &KEYS, None);
}

fn resets_text(state: &State, view: &WindowView, wide: bool) -> String {
    let reading = &view.reading;
    let now = state.now.to_zoned(state.zone());
    if reading.expired {
        return "No reading since it reset".to_owned();
    }
    match reading.resets_at_ms.and_then(|ms| zoned(state, ms)) {
        Some(at) if wide => resets(&at, &now),
        Some(at) => format!("Resets {}", when(&at, &now)),
        None => "No reset time".to_owned(),
    }
}

fn window_line(c: &mut Canvas, view: &WindowView, y: i32, (meter_x, meter_length): (i32, i32)) {
    let theme = c.theme;
    let reading = &view.reading;
    c.text(2, y, &fit(&clean(&name(reading)), 22), theme.text());
    let old = reading.stale || reading.expired;
    c.right(
        26,
        y,
        &percent(reading.used_percentage),
        if old { theme.text() } else { theme.strong() },
    );
    c.meter(
        meter_x,
        y,
        meter_length,
        reading.used_percentage / 100.0,
        meter_style(theme, reading),
    );
    if let Some((mark, words)) = level(theme, reading) {
        let x = c.text(meter_x + meter_length + 2, y, "▲ ", mark);
        c.text(x, y, words, theme.text());
    }
}

fn wide_layout(c: &mut Canvas, state: &State, limits: &Limits) {
    let theme = c.theme;
    let w = c.w;
    let views = views(limits);
    let readings: Vec<&WindowReading> = views.iter().map(|view| &view.reading).collect();
    let (note, style) = provenance(state, &readings, true);
    c.boxed(
        (0, 2, w, 14),
        Some("Plan limits"),
        (!note.is_empty()).then_some((note.as_str(), style)),
    );
    let meter_length = w - 80;
    if views.is_empty() {
        recording(c, state, 3);
    }
    let now_ms = state.now.as_millisecond();
    for (row, view) in (0..).zip(views.iter().take(3)) {
        let y = 3 + row * 4;
        window_line(c, view, y, (28, meter_length));
        let mut parts = vec![resets_text(state, view, true)];
        if view.reading.stale {
            parts.push(reading_age(state, &view.reading));
        } else if let (Some(pace), Some(since)) = (pace_text(state, view), since(state, view)) {
            parts.push(format!("at the pace {since}, {pace}"));
        }
        c.text(4, y + 1, &fit(&parts.join(" · "), w - 6), theme.muted());
        if let Some(since) = since(state, view) {
            c.text(4, y + 2, &format!("Use {since}"), theme.muted());
            c.text(
                28,
                y + 2,
                &use_line(view, now_ms, meter_length, theme.ramp()),
                theme.accent(),
            );
        }
    }
    // The table keeps the board's columns whatever the width, so its figures stay by their bars.
    let cost_end = 80;
    c.boxed((0, 16, w, 10), Some("Recent five-hour windows"), None);
    if limits.recent.is_empty() {
        c.text(
            2,
            17,
            "No five-hour window has a reading yet.",
            theme.muted(),
        );
    } else {
        c.text(4, 17, "Window", theme.muted());
        c.right(40, 17, "Highest", theme.muted());
        c.right(cost_end, 17, "API-equivalent cost", theme.muted());
        for (row, window) in (0..).zip(limits.recent.iter().take(7)) {
            recent_row(c, state, window, 18 + row, (20, cost_end));
        }
    }
    c.boxed(
        (0, 26, w, c.h - 27),
        Some("Where the limits come from"),
        None,
    );
    sources(c, state, limits, 27, true);
    c.text(
        2,
        30,
        "Limits are shared between Claude Code and claude.ai.",
        theme.muted(),
    );
}

fn narrow_layout(c: &mut Canvas, state: &State, limits: &Limits) {
    let theme = c.theme;
    let w = c.w;
    let views = views(limits);
    let readings: Vec<&WindowReading> = views.iter().map(|view| &view.reading).collect();
    let (note, style) = provenance(state, &readings, false);
    c.boxed(
        (0, 2, w, 10),
        Some("Plan limits"),
        (!note.is_empty()).then_some((note.as_str(), style)),
    );
    if views.is_empty() {
        recording(c, state, 3);
    }
    let meter_length = w - 56;
    let now_ms = state.now.as_millisecond();
    for (row, view) in (0..).zip(views.iter().take(3)) {
        let y = 3 + row * 3;
        window_line(c, view, y, (28, meter_length));
        let spark_cells = 16;
        let mut parts = vec![resets_text(state, view, false)];
        if view.reading.stale {
            parts.push(reading_age(state, &view.reading));
        } else if let Some(pace) = pace_text(state, view) {
            parts.push(pace);
        }
        c.text(
            4,
            y + 1,
            &fit(&parts.join(" · "), w - 4 - 3 - spark_cells - 2),
            theme.muted(),
        );
        if view.start_ms.is_some() {
            c.right(
                w - 3,
                y + 1,
                &use_line(view, now_ms, spark_cells, theme.ramp()),
                theme.accent(),
            );
        }
    }
    let cost_end = w - 4;
    c.boxed((0, 12, w, 6), Some("Recent five-hour windows"), None);
    if limits.recent.is_empty() {
        c.text(
            2,
            13,
            "No five-hour window has a reading yet.",
            theme.muted(),
        );
    } else {
        c.text(4, 13, "Window", theme.muted());
        c.right(40, 13, "Highest", theme.muted());
        c.right(cost_end, 13, "Cost", theme.muted());
        for (row, window) in (0..).zip(limits.recent.iter().take(3)) {
            recent_row(c, state, window, 14 + row, (16, cost_end));
        }
    }
    c.boxed((0, 18, w, 4), Some("Where the limits come from"), None);
    sources(c, state, limits, 19, false);
}

/// When an old reading was taken: `This reading is from Wed 7 Oct 18:20, 3 days ago.`
fn reading_age(state: &State, reading: &WindowReading) -> String {
    let Some(at) = zoned(state, reading.observed_ms) else {
        return String::new();
    };
    let age = Timestamp::from_millisecond(reading.observed_ms)
        .map(|observed| ago(observed, state.now))
        .unwrap_or_default();
    format!(
        "reading from {} {}, {age}",
        day_label(at.date()),
        clock(&at)
    )
}

fn recent_row(
    c: &mut Canvas,
    state: &State,
    window: &FiveHourWindow,
    y: i32,
    (meter_length, cost_end): (i32, i32),
) {
    let theme = c.theme;
    c.text(4, y, &five_hour_span(&state.zone(), window), theme.text());
    let high = window.highest;
    c.right(
        40,
        y,
        &percent(high),
        if high >= 90.0 {
            theme.warn()
        } else {
            theme.strong()
        },
    );
    c.meter(
        42,
        y,
        meter_length,
        high / 100.0,
        if high >= 70.0 {
            theme.warn_text()
        } else {
            theme.accent()
        },
    );
    let totals = &window.totals;
    let cost = if totals.requests > 0 && totals.unpriced_requests == totals.requests {
        "unpriced".to_owned()
    } else {
        money(totals.cost.total)
    };
    c.right(cost_end, y, &cost, theme.strong());
}

/// How the status line and the usage endpoint are connected, with the command that changes each.
fn sources(c: &mut Canvas, state: &State, limits: &Limits, y: i32, wide: bool) {
    let theme = c.theme;
    let column = if wide { 22 } else { 18 };
    let right = c.w - 3;
    c.text(2, y, "Status line", theme.strong());
    let mut x = column;
    let end = match &limits.status_line {
        StatusLine::Connected { wrapping } => {
            x = c.text(x, y, "✓ ", theme.good());
            let text = match (wrapping, wide) {
                (Some(_), true) => "connected, wrapping your own status line",
                (Some(_), false) => "connected, wrapping your own",
                (None, _) => "connected",
            };
            c.text(x, y, text, theme.text())
        }
        StatusLine::NotConnected => {
            x = c.text(x, y, "not connected  ", theme.text());
            c.text(x, y, "claude-dashboard statusline connect", theme.accent())
        }
        StatusLine::Unknown { reason } => {
            x = c.text(x, y, "▲ ", theme.warn());
            c.text(
                x,
                y,
                &fit(
                    &format!("cannot tell whether it is connected: {}", clean(reason)),
                    right - x,
                ),
                theme.text(),
            )
        }
    };
    // The note at the right goes where the line leaves room for it.
    let note = "official, while Claude Code runs in a terminal";
    if wide && right - width(note) >= end + 2 {
        c.right(right, y, note, theme.muted());
    }
    let y = y + 1;
    c.text(2, y, "Usage endpoint", theme.strong());
    endpoint_line(c, state, &limits.endpoint, (column, y), wide);
}

fn endpoint_line(
    c: &mut Canvas,
    state: &State,
    endpoint: &Endpoint,
    (x, y): (i32, i32),
    wide: bool,
) {
    let theme = c.theme;
    let right = c.w - 3;
    let asked = endpoint.asked_ms.and_then(|ms| zoned(state, ms));
    let end = if endpoint.enabled {
        let problem = endpoint_problem(endpoint, wide);
        if let Some(problem) = problem {
            let x = c.text(x, y, "▲ ", theme.warn());
            c.text(x, y, &fit(&problem, right - x), theme.text())
        } else {
            let x = c.text(x, y, "✓ ", theme.good());
            let answered = endpoint
                .asked_ms
                .filter(|_| endpoint.outcome.as_deref() == Some("answered"))
                .and_then(|ms| Timestamp::from_millisecond(ms).ok());
            let text = answered.map_or_else(
                || "on, not asked yet".to_owned(),
                |at| format!("on, last answer {}", ago(at, state.now)),
            );
            c.text(x, y, &fit(&text, right - x), theme.text())
        }
    } else {
        let x = c.text(x, y, "off  ", theme.text());
        c.text(x, y, "claude-dashboard limits endpoint on", theme.accent())
    };
    // The note at the right goes where the line leaves room for it.
    let note = if endpoint.enabled {
        asked.as_ref().map_or_else(
            || "experimental".to_owned(),
            |at| format!("experimental · asked {}", clock(at)),
        )
    } else {
        "experimental, when no session runs in a terminal".to_owned()
    };
    if wide && right - width(&note) >= end + 2 {
        c.right(right, y, &note, theme.muted());
    }
}

/// What went wrong the last time the usage endpoint was asked, in a few words.
fn endpoint_problem(endpoint: &Endpoint, wide: bool) -> Option<String> {
    let login = |long: &str| {
        if wide {
            long.to_owned()
        } else {
            "open Claude Code to sign in again".to_owned()
        }
    };
    Some(match endpoint.outcome.as_deref()? {
        "login-expired" => {
            login("on, but Claude Code's login has expired: open Claude Code to sign in again")
        }
        "login-refused" => login(
            "on, but it did not accept Claude Code's login: open Claude Code to sign in again",
        ),
        "no-login" => "on, but Claude Code has no claude.ai login on this computer".to_owned(),
        "rate-limited" => "on, but it says it is asked too often, so it waits".to_owned(),
        "unreadable" => "on, but its answer held no plan limits this version can read".to_owned(),
        "failed" => format!(
            "on, but it could not be asked: {}",
            clean(endpoint.detail.as_deref().unwrap_or("no reason was given"))
        ),
        _ => return None,
    })
}
