//! Sessions: every session in the chosen days, newest first, with search; each opens its detail.

use claude_dashboard_server::fmt::{day_time, duration, grouped, money};
use claude_dashboard_server::protocol::SessionRow;
use jiff::Timestamp;

use super::overview::cost_text;
use super::{footer, header, notice};
use crate::tui::canvas::{Canvas, clean, fit};
use crate::tui::state::{Section, State};

/// How far the estimate may be from Claude Code's figure before prices may be out of date.
const DRIFT_LIMIT: f64 = 0.02;

/// The columns: where the title, the project and the start begin and how wide each is, and where
/// the right-aligned figures end. They keep the boards' places at 120 and 80 columns; a wider
/// window gives the title the room.
struct Columns {
    title: (i32, i32),
    project: (i32, i32),
    started: i32,
    /// Duration, subagents, requests, cost and Claude Code's own figure, in the wide layout.
    figures: Option<[i32; 5]>,
    cost: i32,
}

fn columns(state: &State, w: i32) -> Columns {
    if state.wide() {
        let extra = w - 120;
        Columns {
            title: (4, 25 + extra),
            project: (31 + extra, 16),
            started: 49 + extra,
            figures: Some([w - 47, w - 36, w - 26, w - 15, w - 2]),
            cost: w - 15,
        }
    } else {
        let extra = w - 80;
        Columns {
            title: (4, 28 + extra),
            project: (34 + extra, 16),
            started: 52 + extra,
            figures: None,
            cost: w - 4,
        }
    }
}

pub fn draw(c: &mut Canvas, state: &State) {
    let theme = c.theme;
    let w = c.w;
    header(c, state, Some(Section::Sessions));
    notice(c, state, "", false);
    let top = 2;
    let page = state.sessions_page();
    let searching = !state.search.text.is_empty() || state.search.typing;
    let note = page.map(|page| {
        if searching {
            match state.sessions_count() {
                Some(all) => format!("{} of {} match", grouped(page.total), grouped(all)),
                None => format!("{} match", grouped(page.total)),
            }
        } else {
            format!("{} · newest first", grouped(page.total))
        }
    });
    c.boxed(
        (0, top, w, c.h - 1 - top),
        Some(&format!("Sessions · {}", state.range.title())),
        note.as_deref().map(|note| (note, theme.muted())),
    );
    search_line(c, state, top + 1, searching);
    let columns = columns(state, w);
    let heading = top + 3;
    headings(c, &columns, heading);
    let rows = state.session_rows();
    if page.is_none() {
        c.text(
            columns.title.0,
            heading + 1,
            "Loading the sessions…",
            theme.muted(),
        );
    }
    if let Some(page) = page {
        let offset = state.sessions_offset();
        let start = state.sessions.top.saturating_sub(offset);
        for (index, row) in (0..).zip(page.rows.iter().skip(start).take(rows)) {
            let y = heading + 1 + index;
            session_row(c, state, row, y, &columns);
            if state.sessions.top + usize::try_from(index).unwrap_or(0) == state.sessions.selected {
                c.select(y, 1, w - 2);
            }
        }
        if page.total == 0 {
            let empty = if state.search.applied.is_empty() {
                "No sessions in these days."
            } else {
                "No session matches the search."
            };
            c.text(columns.title.0, heading + 1, empty, theme.muted());
        } else {
            shown_line(c, state, page.total, searching);
        }
    }
    footer(c, state, keys(state), None);
}

/// What the search holds, or how to start one.
fn search_line(c: &mut Canvas, state: &State, y: i32, searching: bool) {
    let theme = c.theme;
    if !searching {
        let x = c.text(2, y, "/", theme.key());
        c.text(x + 1, y, "searches titles and projects", theme.muted());
        return;
    }
    let x = c.text(2, y, "Search ", theme.muted());
    let x = c.text(x, y, &fit(&state.search.text, c.w - 12), theme.strong());
    if state.search.typing {
        c.put(x, y, "█", theme.accent());
    } else {
        let x = c.text(x + 3, y, "/", theme.key());
        c.text(x + 1, y, "changes it", theme.muted());
    }
}

fn headings(c: &mut Canvas, columns: &Columns, y: i32) {
    let theme = c.theme;
    c.text(columns.title.0, y, "Session", theme.muted());
    c.text(columns.project.0, y, "Project", theme.muted());
    c.text(columns.started, y, "Started", theme.muted());
    if let Some([duration, subagents, requests, cost, own]) = columns.figures {
        c.right(duration, y, "Duration", theme.muted());
        c.right(subagents, y, "Subagents", theme.muted());
        c.right(requests, y, "Requests", theme.muted());
        c.right(cost, y, "Cost", theme.muted());
        c.right(own, y, "Claude Code", theme.muted());
    } else {
        c.right(columns.cost, y, "Cost", theme.muted());
    }
}

/// Which sessions show: `1–27 of 214 · PgDn for more`.
fn shown_line(c: &mut Canvas, state: &State, total: u64, searching: bool) {
    let total = usize::try_from(total).unwrap_or(usize::MAX);
    let from = state.sessions.top + 1;
    let to = (state.sessions.top + state.session_rows()).min(total);
    let more = to < total && !searching;
    let text = format!(
        "{}–{} of {}{}",
        grouped(u64::try_from(from).unwrap_or(0)),
        grouped(u64::try_from(to).unwrap_or(0)),
        grouped(u64::try_from(total).unwrap_or(0)),
        if more { " · PgDn for more" } else { "" }
    );
    c.text(2, c.h - 3, &text, c.theme.muted());
}

fn keys(state: &State) -> &'static [(&'static str, &'static str)] {
    if state.search.typing {
        &[
            ("Enter", "open"),
            ("↑↓", "move"),
            ("Esc", "clear the search"),
        ]
    } else if state.search.applied.is_empty() {
        &[
            ("q", "quit"),
            ("←→", "sections"),
            ("Enter", "open"),
            ("/", "search"),
            ("e", "export"),
            ("?", "help"),
        ]
    } else {
        &[
            ("q", "quit"),
            ("Enter", "open"),
            ("Esc", "clear the search"),
            ("e", "export"),
            ("?", "help"),
        ]
    }
}

fn session_row(c: &mut Canvas, state: &State, row: &SessionRow, y: i32, columns: &Columns) {
    let theme = c.theme;
    let title = row.title.as_deref().map(clean);
    c.text(
        columns.title.0,
        y,
        &fit(
            title.as_deref().unwrap_or("Untitled session"),
            columns.title.1,
        ),
        if title.is_some() {
            theme.text()
        } else {
            theme.muted()
        },
    );
    c.text(
        columns.project.0,
        y,
        &fit(&clean(&row.project.name), columns.project.1),
        theme.muted(),
    );
    let started = Timestamp::from_millisecond(row.first_ms)
        .map(|at| day_time(&at.to_zoned(state.zone())))
        .unwrap_or_default();
    c.text(columns.started, y, &started, theme.muted());
    let Some([length, subagents, requests, cost, own]) = columns.figures else {
        c.right(columns.cost, y, &cost_text(&row.totals), theme.strong());
        return;
    };
    c.right(
        length,
        y,
        &duration(row.last_ms - row.first_ms),
        theme.text(),
    );
    if row.subagents > 0 {
        c.right(subagents, y, &grouped(row.subagents), theme.text());
    }
    c.right(requests, y, &grouped(row.totals.requests), theme.text());
    c.right(cost, y, &cost_text(&row.totals), theme.strong());
    if let Some(reported) = row.reported_usd.filter(|usd| *usd > 0.0) {
        let off = row.drift.is_some_and(|drift| drift.abs() > DRIFT_LIMIT);
        let text = if off {
            format!("▲ {}", money(reported))
        } else {
            money(reported)
        };
        c.right(
            own,
            y,
            &text,
            if off { theme.warn_text() } else { theme.text() },
        );
    }
}
