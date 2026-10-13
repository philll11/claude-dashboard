//! One session: its cost against Claude Code's own figure, its cost as it went, its tokens by type,
//! who did the work, and the skills and MCP tools its requests are credited to. A page that
//! scrolls when it does not fit.

use claude_dashboard_server::fmt::{clock, compact, day_label, duration, grouped, money};
use claude_dashboard_server::protocol::{Credit, SessionDetail, Totals, WhoRow};
use jiff::{Timestamp, Zoned};

use super::overview::{cost_text, tokens_of};
use super::{footer, header, scrolled, series_of};
use crate::tui::canvas::{Canvas, clean, fit, money_tick, nice_scale, round};
use crate::tui::state::{Section, State};
use crate::tui::theme::Series;

/// How far the estimate may be from Claude Code's figure before prices may be out of date.
const DRIFT_LIMIT: f64 = 0.02;
/// How recently a session that Claude Code's own figure does not cover made a request, to count
/// as running.
const RUNNING_MS: i64 = 15 * 60_000;

/// How Claude Code was started, as the session's line says it.
fn started_from(entrypoint: &str) -> &str {
    match entrypoint {
        "cli" => "terminal",
        "claude-vscode" => "VS Code",
        "claude-desktop" => "desktop app",
        "sdk-cli" | "sdk-ts" | "sdk-py" => "Agent SDK",
        other => other,
    }
}

/// The rows of the page above its bottom boxes, and the boxes' heights: the title box, the cost
/// chart, who did the work, and in the narrow layout the tokens by type.
struct Layout {
    chart: (i32, i32),
    who: (i32, i32),
    tokens: Option<(i32, i32)>,
    hint: Option<i32>,
    credits: i32,
}

fn layout(state: &State, detail: &SessionDetail) -> Layout {
    let wide = state.wide();
    let chart_h = if state.tables {
        i32::try_from(detail.cumulative_cost.len())
            .unwrap_or(0)
            .max(1)
            + 4
    } else if wide {
        11
    } else {
        9
    };
    let chart = (4, chart_h.max(if wide { 11 } else { 9 }));
    let who_rows = i32::try_from(detail.who.len()).unwrap_or(0);
    let who = (
        chart.0 + chart.1,
        3 + who_rows.max(if wide { 5 } else { 4 }),
    );
    if wide {
        Layout {
            chart,
            who,
            tokens: None,
            hint: None,
            credits: who.0 + who.1,
        }
    } else {
        let hint = who.0 + who.1;
        let tokens = (hint + 1, 10);
        Layout {
            chart,
            who,
            tokens: Some(tokens),
            hint: Some(hint),
            credits: tokens.0 + tokens.1,
        }
    }
}

/// The rows the credits boxes need: a heading, each credit, and the line of what is credited to
/// neither, a row apart.
fn credit_rows(detail: &SessionDetail, wide: bool) -> i32 {
    let skills = i32::try_from(detail.skills.len()).unwrap_or(0).max(1);
    let tools = i32::try_from(detail.mcp_tools.len()).unwrap_or(0).max(1);
    if wide {
        skills.max(tools) + 5
    } else {
        skills + 3 + tools + 5
    }
}

/// How tall the page is.
pub fn height(state: &State) -> i32 {
    let Some(detail) = state.session_detail() else {
        return 0;
    };
    let layout = layout(state, detail);
    layout.credits + credit_rows(detail, state.wide())
}

pub fn draw(c: &mut Canvas, state: &State) {
    let theme = c.theme;
    header(c, state, Some(Section::Sessions));
    let detail = state.session_detail();
    let title = detail
        .and_then(|detail| detail.title.as_deref())
        .map_or_else(|| "Untitled session".to_owned(), clean);
    let x = c.text(1, 1, "Sessions", theme.muted());
    let x = c.text(x, 1, " › ", theme.muted());
    if detail.is_some() {
        c.text(x, 1, &fit(&title, c.w - 1 - x), theme.text());
    } else if let Some(error) = state.session_error() {
        c.text(x, 1, &fit(&clean(error), c.w - 1 - x), theme.text());
    } else {
        c.text(x, 1, "Loading the session…", theme.muted());
    }
    if let Some(detail) = detail {
        let page = height(state);
        scrolled(c, state, page, |page| {
            draw_page(page, state, detail, &title);
        });
    }
    footer(
        c,
        state,
        &[
            ("Esc", "back"),
            ("↑↓", "scroll"),
            ("t", "table view"),
            ("e", "export"),
            ("?", "help"),
        ],
        None,
    );
}

fn zoned(state: &State, ms: i64) -> Option<Zoned> {
    Timestamp::from_millisecond(ms)
        .ok()
        .map(|at| at.to_zoned(state.zone()))
}

/// From when to when: `Sat 10 Oct 13:42–14:54`, or across days with both days.
fn span(state: &State, first_ms: i64, last_ms: i64) -> String {
    let (Some(first), Some(last)) = (zoned(state, first_ms), zoned(state, last_ms)) else {
        return String::new();
    };
    if first.date() == last.date() {
        format!(
            "{} {}–{}",
            day_label(first.date()),
            clock(&first),
            clock(&last)
        )
    } else {
        format!(
            "{} {}–{} {}",
            day_label(first.date()),
            clock(&first),
            day_label(last.date()),
            clock(&last)
        )
    }
}

fn draw_page(c: &mut Canvas, state: &State, detail: &SessionDetail, title: &str) {
    let theme = c.theme;
    let wide = state.wide();
    let w = c.w;
    c.boxed(
        (0, 0, w, 4),
        Some(&fit(title, w - 30)),
        Some((&clean(&detail.project.name), theme.muted())),
    );
    let mut meta = vec![
        span(state, detail.first_ms, detail.last_ms),
        duration(detail.last_ms - detail.first_ms),
    ];
    if let Some(version) = &detail.claude_code_version {
        meta.push(format!("Claude Code {}", clean(version)));
    }
    if wide {
        if let Some(entrypoint) = &detail.entrypoint {
            meta.push(clean(started_from(entrypoint)));
        }
        meta.push(format!(
            "session {}",
            detail.id.chars().take(8).collect::<String>()
        ));
    }
    c.text(2, 1, &fit(&meta.join(" · "), w - 4), theme.muted());
    own_figure(c, state, detail, 2);
    let layout = layout(state, detail);
    let series = series_of(
        state
            .choices()
            .map(|choices| {
                choices
                    .models
                    .iter()
                    .map(|model| model.id.as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    );
    if wide {
        cost_box(c, state, detail, (0, layout.chart.0, 62, layout.chart.1));
        tokens_box(c, &detail.totals, (62, layout.chart.0, w - 62, 11));
    } else {
        cost_box(c, state, detail, (0, layout.chart.0, w, layout.chart.1));
    }
    who_box(c, state, detail, &series, layout.who);
    if let Some(hint) = layout.hint {
        c.text(
            2,
            hint,
            "↓ tokens by type, skills and MCP tools",
            theme.muted(),
        );
    }
    if let Some((top, h)) = layout.tokens {
        tokens_box(c, &detail.totals, (0, top, w, h));
    }
    let rows = credit_rows(detail, wide);
    let bottom = c.h.max(layout.credits + rows);
    if wide {
        let half = w / 2;
        let h = bottom - layout.credits;
        credits_box(c, "Skills", &detail.skills, (0, layout.credits, half, h));
        credits_box(
            c,
            "MCP tools",
            &detail.mcp_tools,
            (half, layout.credits, w - half, h),
        );
        uncredited(c, &detail.uncredited, bottom - 2);
    } else {
        let skills_h = i32::try_from(detail.skills.len()).unwrap_or(0).max(1) + 3;
        credits_box(
            c,
            "Skills",
            &detail.skills,
            (0, layout.credits, w, skills_h),
        );
        let h = bottom - layout.credits - skills_h;
        credits_box(
            c,
            "MCP tools",
            &detail.mcp_tools,
            (0, layout.credits + skills_h, w, h),
        );
        uncredited(c, &detail.uncredited, bottom - 2);
    }
}

/// The estimate against Claude Code's own figure for the session.
fn own_figure(c: &mut Canvas, state: &State, detail: &SessionDetail, y: i32) {
    let theme = c.theme;
    let wide = state.wide();
    let mut x = c.text(2, y, "Estimate ", theme.muted());
    x = c.text(x, y, &cost_text(&detail.totals), theme.strong());
    x = c.text(
        x,
        y,
        if wide {
            "   Claude Code's own figure "
        } else {
            "   Claude Code "
        },
        theme.muted(),
    );
    let Some(reported) = detail.reported_usd.filter(|usd| *usd > 0.0) else {
        c.text(x, y, "none yet", theme.muted());
        return;
    };
    x = c.text(x, y, &money(reported), theme.strong());
    let unpriced = detail
        .who
        .iter()
        .filter(|row| row.model.name == row.model.id)
        .map(|row| row.model.id.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len();
    let running =
        detail.drift.is_none() && state.now.as_millisecond() - detail.last_ms < RUNNING_MS;
    let room = c.w - 2;
    if unpriced > 0 {
        let x = c.text(x + 3, y, "▲ ", theme.warn());
        let text = if unpriced == 1 {
            "1 model unpriced".to_owned()
        } else {
            format!("{unpriced} models unpriced")
        };
        c.text(x, y, &fit(&text, room - x), theme.text());
    } else if running {
        c.text(x + 3, y, "not ended yet", theme.muted());
    } else if let Some(drift) = detail.drift {
        if drift.abs() <= DRIFT_LIMIT {
            c.text(
                x + 3,
                y,
                &format!("within {:.1}%", (drift.abs() * 100.0).max(0.1)),
                theme.muted(),
            );
        } else {
            let x = c.text(x + 3, y, "▲ ", theme.warn());
            let text = format!(
                "{}% {}, prices may be out of date",
                round(drift.abs() * 100.0),
                if drift < 0.0 { "lower" } else { "higher" }
            );
            c.text(x, y, &fit(&text, room - x), theme.text());
        }
    } else {
        c.text(
            x + 3,
            y,
            &fit("counts other requests, so not compared", room - x - 3),
            theme.muted(),
        );
    }
}

/// The cost so far at `at`.
fn cost_at(detail: &SessionDetail, at: i64) -> f64 {
    detail
        .cumulative_cost
        .iter()
        .take_while(|point| point.ms <= at)
        .last()
        .map_or(0.0, |point| point.usd)
}

/// The cumulative cost over the session's time, or its requests as a table.
fn cost_box(
    c: &mut Canvas,
    state: &State,
    detail: &SessionDetail,
    (x0, y0, w, h): (i32, i32, i32, i32),
) {
    let theme = c.theme;
    if state.tables {
        c.boxed((x0, y0, w, h), Some("Requests and their cost"), None);
        c.text(x0 + 2, y0 + 1, "Time", theme.muted());
        c.right(x0 + w - 16, y0 + 1, "Request", theme.muted());
        c.right(x0 + w - 3, y0 + 1, "So far", theme.muted());
        let mut before = 0.0;
        for (row, point) in (0..).zip(&detail.cumulative_cost) {
            let y = y0 + 2 + row;
            let at = zoned(state, point.ms)
                .map(|at| clock(&at))
                .unwrap_or_default();
            c.text(x0 + 2, y, &at, theme.text());
            c.right(x0 + w - 16, y, &money(point.usd - before), theme.text());
            c.right(x0 + w - 3, y, &money(point.usd), theme.strong());
            before = point.usd;
        }
        return;
    }
    c.boxed(
        (x0, y0, w, h),
        Some("Cost over the session"),
        Some(("cumulative", theme.muted())),
    );
    let rows = h - 3;
    let first = detail.first_ms;
    let last = detail.last_ms.max(first + 60_000);
    let total = detail.cumulative_cost.last().map_or(0.0, |point| point.usd);
    let (top, step) = nice_scale(total, 2);
    let top_label = money_tick(top, step);
    let plot_x = x0 + super::chart::scale_room(&[&top_label]);
    let plot_w = x0 + w - 3 - plot_x;
    let values: Vec<f64> = (0..plot_w)
        .map(|column| {
            let at = first + (last - first) * i64::from(column + 1) / i64::from(plot_w.max(1));
            cost_at(detail, at)
        })
        .collect();
    for r in 0..rows {
        let y = y0 + 1 + (rows - 1 - r);
        if r == rows - 1 {
            c.right(plot_x - 1, y, &top_label, theme.muted());
        }
        if r == 0 {
            c.right(plot_x - 1, y, "$0", theme.muted());
        }
        for (column, value) in (0..).zip(&values) {
            let level = value / top * f64::from(rows) - f64::from(r);
            if level >= 1.0 {
                c.put(plot_x + column, y, "█", theme.accent());
            } else if level > 0.0 {
                let index = (round(level * 8.0) - 1).clamp(0, 7);
                c.put(
                    plot_x + column,
                    y,
                    theme.ramp()[usize::try_from(index).unwrap_or(0)],
                    theme.accent(),
                );
            }
        }
    }
    let axis = y0 + h - 2;
    let time = |ms: i64| zoned(state, ms).map(|at| clock(&at)).unwrap_or_default();
    c.text(plot_x, axis, &time(first), theme.muted());
    c.right(plot_x + plot_w, axis, &time(last), theme.muted());
    if plot_w > 30 {
        c.text(
            plot_x + plot_w / 2 - 2,
            axis,
            &time(first + (last - first) / 2),
            theme.muted(),
        );
    }
}

/// Tokens and cost by type: input, output, each kind of cache write, cache reads and web searches.
fn tokens_box(c: &mut Canvas, totals: &Totals, (x0, y0, w, h): (i32, i32, i32, i32)) {
    let theme = c.theme;
    c.boxed((x0, y0, w, h), Some("Tokens by type"), None);
    let right = x0 + w;
    c.right(right - 18, y0 + 1, "Tokens", theme.muted());
    c.right(right - 3, y0 + 1, "Cost", theme.muted());
    let tokens = &totals.tokens;
    let cost = &totals.cost;
    let mut rows = vec![
        ("Input", compact(tokens.input), cost.input),
        ("Output", compact(tokens.output), cost.output),
        (
            "Cache writes, 5 minutes",
            compact(tokens.cache_write_5m),
            cost.cache_write_5m,
        ),
        (
            "Cache writes, 1 hour",
            compact(tokens.cache_write_1h),
            cost.cache_write_1h,
        ),
        ("Cache reads", compact(tokens.cache_read), cost.cache_read),
    ];
    if totals.web_search_requests > 0 {
        rows.push((
            "Web searches",
            grouped(totals.web_search_requests),
            cost.web_search,
        ));
    }
    for (row, (label, count, usd)) in (0..).zip(rows) {
        let y = y0 + 2 + row;
        c.text(x0 + 2, y, label, theme.text());
        c.right(right - 18, y, &count, theme.text());
        c.right(right - 3, y, &money(usd), theme.strong());
    }
    c.text(x0 + 2, y0 + h - 2, "Total", theme.strong());
    c.right(right - 3, y0 + h - 2, &cost_text(totals), theme.strong());
}

/// Who did the work: the session itself and each subagent, on each model.
fn who_box(
    c: &mut Canvas,
    state: &State,
    detail: &SessionDetail,
    series: &std::collections::HashMap<String, Series>,
    (y0, h): (i32, i32),
) {
    let theme = c.theme;
    let wide = state.wide();
    let w = c.w;
    c.boxed((0, y0, w, h), Some("Models and subagents"), None);
    let heading = y0 + 1;
    let (model_x, requests_end, cost_end) = if wide {
        (38, w - 40, w - 14)
    } else {
        (22, w - 28, w - 4)
    };
    c.text(2, heading, "Who", theme.muted());
    c.text(model_x, heading, "Model", theme.muted());
    c.right(requests_end, heading, "Requests", theme.muted());
    c.right(cost_end, heading, "Cost", theme.muted());
    if wide {
        c.text(24, heading, "Type", theme.muted());
        c.text(54, heading, "When", theme.muted());
        c.right(w - 28, heading, "Tokens", theme.muted());
    }
    for (row, who) in (0..).zip(&detail.who) {
        let y = heading + 1 + row;
        let (name, kind) = who_name(who);
        c.text(2, y, &fit(&name, if wide { 21 } else { 19 }), theme.text());
        if wide {
            c.text(24, y, kind, theme.muted());
            let span = match (zoned(state, who.first_ms), zoned(state, who.last_ms)) {
                (Some(first), Some(last)) if clock(&first) == clock(&last) => clock(&first),
                (Some(first), Some(last)) => format!("{}–{}", clock(&first), clock(&last)),
                _ => String::new(),
            };
            c.text(54, y, &span, theme.muted());
            c.right(w - 28, y, &compact(tokens_of(&who.totals)), theme.text());
        }
        let of = series.get(&who.model.id).copied().unwrap_or(Series::Other);
        c.put(model_x, y, theme.swatch(of), theme.series(of));
        let name_room = if wide {
            13
        } else {
            requests_end - 9 - (model_x + 2)
        };
        c.text(
            model_x + 2,
            y,
            &fit(&clean(&who.model.name), name_room),
            theme.text(),
        );
        c.right(requests_end, y, &grouped(who.totals.requests), theme.text());
        c.right(cost_end, y, &cost_text(&who.totals), theme.strong());
    }
}

/// Who a row is: `The session`, or a subagent by its kind.
fn who_name(who: &WhoRow) -> (String, &'static str) {
    match &who.subagent {
        None => ("The session".to_owned(), ""),
        Some(_) => (
            who.agent_type
                .as_deref()
                .map_or_else(|| "Unnamed".to_owned(), clean),
            "subagent",
        ),
    }
}

fn credits_box(
    c: &mut Canvas,
    title: &str,
    credits: &[Credit],
    (x0, y0, w, h): (i32, i32, i32, i32),
) {
    let theme = c.theme;
    c.boxed((x0, y0, w, h), Some(title), None);
    let right = x0 + w;
    c.right(right - 14, y0 + 1, "Requests", theme.muted());
    c.right(right - 3, y0 + 1, "Cost", theme.muted());
    if credits.is_empty() {
        c.text(x0 + 2, y0 + 2, "None", theme.muted());
    }
    for (row, credit) in (0..).zip(credits) {
        let y = y0 + 2 + row;
        let name = match &credit.tool {
            Some(tool) => format!("{} · {}", clean(&credit.name), clean(tool)),
            None => clean(&credit.name),
        };
        c.text(x0 + 2, y, &fit(&name, w - 4 - 22), theme.text());
        c.right(
            right - 14,
            y,
            &grouped(credit.totals.requests),
            theme.text(),
        );
        c.right(right - 3, y, &cost_text(&credit.totals), theme.strong());
    }
}

/// What is credited to neither a skill nor an MCP server.
fn uncredited(c: &mut Canvas, totals: &Totals, y: i32) {
    let theme = c.theme;
    let x = c.text(
        2,
        y,
        "Not credited to a skill or an MCP server: ",
        theme.muted(),
    );
    c.text(x, y, &cost_text(totals), theme.strong());
}
