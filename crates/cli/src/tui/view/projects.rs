//! Projects: each project's sessions, last activity, requests, tokens, daily cost and cost, for the
//! chosen days and model, with the selected project's cost per day by model below.

use claude_dashboard_server::fmt::{clock, compact, day_label, grouped, money};
use claude_dashboard_server::protocol::ProjectRow;
use jiff::Timestamp;

use super::chart;
use super::overview::{cost_chart, cost_text, daily_cost, tokens_of};
use super::{footer, header, notice};
use crate::tui::canvas::{Canvas, clean, fit, spark};
use crate::tui::state::{Section, State, day};

/// Projects with a smaller share of the cost than this are grouped together.
const SMALL_SHARE: f64 = 0.02;

/// A row of the table: one project, or the small ones together.
pub enum Row<'a> {
    Project(&'a ProjectRow),
    Others(Vec<&'a ProjectRow>),
}

/// The table's rows: the projects, highest cost first, with those under 2% of the cost together
/// when there are several.
pub fn rows(state: &State) -> Vec<Row<'_>> {
    let Some(overview) = state
        .projects_request()
        .and_then(|request| state.overview(&request))
    else {
        return Vec::new();
    };
    let total = overview.totals.cost.total;
    let small: Vec<&ProjectRow> = overview
        .projects
        .iter()
        .filter(|row| total > 0.0 && row.totals.cost.total / total < SMALL_SHARE)
        .collect();
    if small.len() > 1 {
        let mut rows: Vec<Row<'_>> = overview
            .projects
            .iter()
            .filter(|row| !small.iter().any(|other| std::ptr::eq(*other, *row)))
            .map(Row::Project)
            .collect();
        rows.push(Row::Others(small));
        rows
    } else {
        overview.projects.iter().map(Row::Project).collect()
    }
}

/// The table's height, box included.
fn table_height(state: &State) -> i32 {
    if state.wide() { 12 } else { 11 }
}

/// How many rows the table shows at once.
pub fn visible_rows(state: &State) -> usize {
    usize::try_from(table_height(state) - 4).unwrap_or(1).max(1)
}

/// A row's figures, for one project or several together.
struct Figures {
    name: String,
    sessions: u64,
    last_ms: i64,
    requests: u64,
    tokens: u64,
    cost: String,
    daily: Vec<(jiff::civil::Date, f64)>,
}

fn figures(row: &Row<'_>) -> Figures {
    let projects: Vec<&ProjectRow> = match row {
        Row::Project(project) => vec![*project],
        Row::Others(projects) => projects.clone(),
    };
    let mut daily: std::collections::HashMap<jiff::civil::Date, f64> =
        std::collections::HashMap::new();
    for project in &projects {
        for cost in &project.daily_cost {
            *daily.entry(day(&cost.date)).or_default() += cost.usd;
        }
    }
    Figures {
        name: match row {
            Row::Project(project) => clean(&project.project.name),
            Row::Others(_) => "Other projects".to_owned(),
        },
        sessions: projects.iter().map(|project| project.sessions).sum(),
        last_ms: projects
            .iter()
            .map(|project| project.last_ms)
            .max()
            .unwrap_or(0),
        requests: projects.iter().map(|project| project.totals.requests).sum(),
        tokens: projects
            .iter()
            .map(|project| tokens_of(&project.totals))
            .sum(),
        cost: match row {
            Row::Project(project) => cost_text(&project.totals),
            Row::Others(projects) => money(
                projects
                    .iter()
                    .map(|project| project.totals.cost.total)
                    .sum(),
            ),
        },
        daily: daily.into_iter().collect(),
    }
}

/// Each day's cost in the characters of `ramp`, in at most `cells` columns: days added together
/// when there are more days than columns.
fn trend(
    figures: &Figures,
    (first, last): (jiff::civil::Date, jiff::civil::Date),
    cells: usize,
    ramp: &[&str; 8],
) -> String {
    let days = daily_cost(figures.daily.iter().copied(), first, last);
    if days.len() <= cells || cells == 0 {
        return spark(&days, ramp);
    }
    let per = days.len().div_ceil(cells);
    let grouped: Vec<f64> = days.chunks(per).map(|chunk| chunk.iter().sum()).collect();
    spark(&grouped, ramp)
}

/// Where the table's columns are. They keep the boards' places at 120 and 80 columns; a wider
/// window widens the daily cost while the range has more days than it shows, and the cost stays
/// beside it.
struct Columns {
    wide: bool,
    sessions_end: i32,
    cost_end: i32,
    trend_x: i32,
    trend_cells: usize,
}

fn columns(state: &State, w: i32) -> Columns {
    let wide = state.wide();
    let days = state.days().map_or(30, |days| {
        (day(&days.last) - day(&days.first)).get_days() + 1
    });
    if wide {
        let trend_x = 78;
        let trend_cells = days.min(w - 2 - 10 - trend_x).max(0);
        Columns {
            wide,
            sessions_end: 36,
            cost_end: (trend_x + trend_cells.max(30) + 10).min(w - 2),
            trend_x,
            trend_cells: usize::try_from(trend_cells).unwrap_or(0),
        }
    } else {
        let trend_x = 44;
        Columns {
            wide,
            sessions_end: 30,
            cost_end: 41,
            trend_x,
            trend_cells: usize::try_from((w - 3 - trend_x).max(0)).unwrap_or(0),
        }
    }
}

pub fn draw(c: &mut Canvas, state: &State) {
    header(c, state, Some(Section::Projects));
    notice(c, state, "", false);
    let rows = rows(state);
    table(c, state, &rows);
    chart_below(c, state, &rows);
    footer(
        c,
        state,
        &[
            ("q", "quit"),
            ("←→", "sections"),
            ("Enter", "filter"),
            ("t", "table view"),
            ("e", "export"),
            ("?", "help"),
        ],
        None,
    );
}

/// The table of projects, with the selected one marked and a total.
fn table(c: &mut Canvas, state: &State, rows: &[Row<'_>]) {
    let theme = c.theme;
    let w = c.w;
    let columns = columns(state, w);
    c.boxed(
        (0, 2, w, table_height(state)),
        Some(&format!("Projects · {}", state.range.title())),
        columns
            .wide
            .then(|| ("Enter filters the dashboard to one", theme.muted())),
    );
    let heading = 3;
    c.text(4, heading, "Project", theme.muted());
    c.right(columns.sessions_end, heading, "Sessions", theme.muted());
    c.right(columns.cost_end, heading, "Cost", theme.muted());
    c.text(columns.trend_x, heading, "Daily cost", theme.muted());
    if columns.wide {
        c.text(40, heading, "Last active", theme.muted());
        c.right(66, heading, "Requests", theme.muted());
        c.right(75, heading, "Tokens", theme.muted());
    }
    let visible = visible_rows(state);
    let top = state.projects.top.min(rows.len().saturating_sub(visible));
    for (index, row) in (0..).zip(rows.iter().skip(top).take(visible)) {
        let y = heading + 1 + index;
        project_line(c, state, row, y, &columns);
        if top + usize::try_from(index).unwrap_or(0) == state.projects.selected {
            c.select(y, 1, w - 2);
        }
    }
    let Some(overview) = state
        .projects_request()
        .and_then(|request| state.overview(&request))
    else {
        c.text(4, heading + 1, "Loading the projects…", theme.muted());
        return;
    };
    if rows.is_empty() {
        c.text(4, heading + 1, "No projects in these days.", theme.muted());
        return;
    }
    let all: Vec<Figures> = rows.iter().map(figures).collect();
    let y = heading + 1 + i32::try_from(rows.len().min(visible)).unwrap_or(0);
    c.text(4, y, "Total", theme.strong());
    c.right(
        columns.sessions_end,
        y,
        &grouped(all.iter().map(|row| row.sessions).sum()),
        theme.text(),
    );
    if columns.wide {
        c.right(
            66,
            y,
            &grouped(all.iter().map(|row| row.requests).sum()),
            theme.text(),
        );
        c.right(
            75,
            y,
            &compact(all.iter().map(|row| row.tokens).sum()),
            theme.text(),
        );
    }
    c.right(
        columns.cost_end,
        y,
        &cost_text(&overview.totals),
        theme.strong(),
    );
}

fn project_line(c: &mut Canvas, state: &State, row: &Row<'_>, y: i32, columns: &Columns) {
    let theme = c.theme;
    let figures = figures(row);
    let others = matches!(row, Row::Others(_));
    c.text(
        4,
        y,
        &fit(&figures.name, columns.sessions_end - 9 - 4),
        if others { theme.muted() } else { theme.text() },
    );
    c.right(
        columns.sessions_end,
        y,
        &grouped(figures.sessions),
        theme.text(),
    );
    if columns.wide {
        let last = Timestamp::from_millisecond(figures.last_ms)
            .map(|at| {
                let at = at.to_zoned(state.zone());
                format!("{} {}", day_label(at.date()), clock(&at))
            })
            .unwrap_or_default();
        c.text(40, y, &last, theme.muted());
        c.right(66, y, &grouped(figures.requests), theme.text());
        c.right(75, y, &compact(figures.tokens), theme.text());
    }
    if let Some(days) = state.days() {
        let days = (day(&days.first), day(&days.last));
        c.text(
            columns.trend_x,
            y,
            &trend(&figures, days, columns.trend_cells, theme.ramp()),
            theme.accent(),
        );
    }
    c.right(columns.cost_end, y, &figures.cost, theme.strong());
}

/// The selected project's cost per day by model; every project's while the small ones together
/// are selected.
fn chart_below(c: &mut Canvas, state: &State, rows: &[Row<'_>]) {
    let table_h = table_height(state);
    let area = (0, 2 + table_h, c.w, c.h - 3 - table_h);
    let (name, overview) = match rows.get(state.projects.selected) {
        Some(Row::Project(project)) => (
            clean(&project.project.name),
            state
                .project_chart_request()
                .and_then(|request| state.overview(&request)),
        ),
        _ => (
            "All projects".to_owned(),
            state
                .projects_request()
                .and_then(|request| state.overview(&request)),
        ),
    };
    let Some(overview) = overview else {
        return;
    };
    let chart = cost_chart(state, overview, c.w - 10);
    let title = format!(
        "{name} · cost per {} by model · {}",
        chart.per(),
        state.range.title()
    );
    if state.tables {
        chart::table(c, area, &chart, &title, state.table_top);
    } else {
        chart::draw(c, area, &chart, &title);
    }
}
