//! Overview: the plan limits, today, this week and this month, cost per day by model, the top
//! projects and the models, for the chosen days and filters.

use claude_dashboard_core::report::Bucket;
use claude_dashboard_server::fmt::{compact, day_label, grouped, money, percent, short_date};
use claude_dashboard_server::protocol::{Access, Day, FolderState, ModelRow, Overview, Totals};
use jiff::ToSpan;
use jiff::civil::Date;

use super::chart::{self, Chart};
use super::limits as windows;
use super::{KEYS, footer, header, notice, reading, series_of};
use crate::tui::canvas::{Canvas, clean, fit, spark};
use crate::tui::state::{Section, State, day};
use crate::tui::theme::Series;

pub fn draw(c: &mut Canvas, state: &State) {
    let wide = state.wide();
    header(c, state, Some(Section::Overview));
    match notice_text(state) {
        Some((text, warning)) => notice(c, state, &text, warning),
        None => notice(c, state, "", false),
    }
    windows::overview_box(c, state, 2);
    let stats_top = 7;
    tiles(c, state, stats_top);
    let chart_top = stats_top + if wide { 4 } else { 3 };
    let projects_w = if wide { 42 } else { 30 };
    let chart_w = c.w - projects_w;
    let overview = state
        .overview_request()
        .and_then(|request| state.overview(&request));
    if let Some(overview) = overview {
        // The chart takes the rows the models below do not need, and gives up to two of the
        // boards' rows for every model to fit. When they cannot all fit, the wide layout keeps the
        // models it has room for, and the narrow one, whose board has no models, leaves them out.
        let models_rows = i32::try_from(overview.models.len()).unwrap_or(0);
        let models_need = models_rows + if wide { 6 } else { 3 };
        let room = c.h - 1 - chart_top;
        let least_h = if wide { 10 } else { 9 } + 4 - 2;
        let chart_h = if room - models_need >= least_h {
            room - models_need
        } else if wide {
            least_h
        } else {
            room
        };
        let chart = cost_chart(state, overview, chart_w - 10);
        let title = format!(
            "Cost per {} by model · {}",
            chart.per(),
            state.range.title()
        );
        if state.tables {
            chart::table(
                c,
                (0, chart_top, chart_w, chart_h),
                &chart,
                &title,
                state.table_top,
            );
        } else {
            chart::draw(c, (0, chart_top, chart_w, chart_h), &chart, &title);
        }
        top_projects(
            c,
            state,
            overview,
            (chart_w, chart_top, projects_w, chart_h),
        );
        let models_top = chart_top + chart_h;
        let models_h = c.h - 1 - models_top;
        if models_h >= 4 {
            models(c, state, overview, (models_top, models_h));
        }
    } else {
        let chart_h = if wide { 10 } else { 9 } + 4;
        c.boxed(
            (0, chart_top, chart_w, chart_h),
            Some(&format!("Cost per day by model · {}", state.range.title())),
            None,
        );
        c.text(2, chart_top + 1, "Loading…", c.theme.muted());
        c.boxed(
            (chart_w, chart_top, projects_w, chart_h),
            Some(&format!("Top projects · {}", state.range.title())),
            None,
        );
    }
    footer(c, state, &KEYS, None);
}

/// The cost chart of an overview, as finely as `room` columns show it.
pub fn cost_chart(state: &State, overview: &Overview, room: i32) -> Chart {
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
    Chart::new(
        (&overview.days.first, &overview.days.last),
        &overview.by_day,
        &overview.models,
        &series,
        state.week_start(),
        room,
    )
}

/// A notice: its text for the wide and the narrow layouts, and whether it is a warning.
struct Notice {
    wide: String,
    narrow: String,
    warning: bool,
}

/// The notice the Overview leads with, if any: its text, and whether it is a warning.
fn notice_text(state: &State) -> Option<(String, bool)> {
    let notice = read_only(state)
        .or_else(|| unreadable_folder(state))
        .or_else(|| first_read(state))
        .or_else(|| stale_limits(state))
        .or_else(|| price_drift(state))
        .or_else(|| import_offer(state))?;
    Some((
        if state.wide() {
            notice.wide
        } else {
            notice.narrow
        },
        notice.warning,
    ))
}

fn read_only(state: &State) -> Option<Notice> {
    (state.status()?.access == Access::ReadOnly).then(|| Notice {
        wide: "A newer claude-dashboard upgraded the history: this one shows it but records nothing new. Update it from the releases page.".to_owned(),
        narrow: "A newer version upgraded the history: this one only shows it. Update it.".to_owned(),
        warning: true,
    })
}

fn unreadable_folder(state: &State) -> Option<Notice> {
    state.health()?.folders.iter().find_map(|folder| {
        let (FolderState::Unreadable { reason } | FolderState::Unreachable { reason }) =
            &folder.state
        else {
            return None;
        };
        Some(Notice {
            wide: format!(
                "{} cannot be read: {}. claude-dashboard doctor says more.",
                clean(&folder.path),
                clean(reason)
            ),
            narrow: "A Claude Code folder cannot be read: claude-dashboard doctor says which"
                .to_owned(),
            warning: true,
        })
    })
}

fn first_read(state: &State) -> Option<Notice> {
    (state.status()?.read_ms.is_none() && reading(state).is_some()).then(|| Notice {
        wide: "Reading your history, newest first: older days appear as they are read.".to_owned(),
        narrow: "Reading your history, newest first: older days follow.".to_owned(),
        warning: false,
    })
}

fn stale_limits(state: &State) -> Option<Notice> {
    let limits = &state.status()?.limits;
    if limits.is_empty()
        || !limits
            .iter()
            .all(|reading| reading.stale || reading.expired)
    {
        return None;
    }
    let newest = limits.iter().map(|reading| reading.observed_ms).max()?;
    let since = jiff::Timestamp::from_millisecond(newest)
        .map(|at| day_label(at.to_zoned(state.zone()).date()))
        .ok()?;
    let endpoint_on = state.limits().is_some_and(|limits| limits.endpoint.enabled);
    Some(Notice {
        wide: format!(
            "No Claude Code session has run in a terminal since {since}: these are the last readings.{}",
            if endpoint_on {
                ""
            } else {
                " The usage endpoint can fill the gap."
            }
        ),
        narrow: format!("No Claude Code session in a terminal since {since}: last readings"),
        warning: true,
    })
}

fn price_drift(state: &State) -> Option<Notice> {
    let drift = state
        .price_check()?
        .out_of_line
        .iter()
        .map(|model| model.drift.abs())
        .reduce(f64::max)?;
    let off = format!("{:.1}%", drift * 100.0);
    Some(Notice {
        wide: format!(
            "Prices may be out of date: estimates are {off} off Claude Code's own session costs. A newer claude-dashboard may fix them."
        ),
        narrow: format!("Prices may be out of date: {off} off Claude Code's own session costs"),
        warning: true,
    })
}

fn import_offer(state: &State) -> Option<Notice> {
    let offer = state.import_offer()?;
    if state.status()?.access != Access::ReadWrite || offer.requests == 0 || offer.dismissed {
        return None;
    }
    Some(Notice {
        wide: format!(
            "A claude-usage history holds {} {} this one does not. Bring them in: claude-dashboard import claude-usage",
            grouped(offer.requests),
            if offer.requests == 1 {
                "request"
            } else {
                "requests"
            }
        ),
        narrow: "A claude-usage history was found: claude-dashboard import claude-usage".to_owned(),
        warning: false,
    })
}

/// Every token, of every kind.
pub fn tokens_of(totals: &Totals) -> u64 {
    let tokens = &totals.tokens;
    tokens.input + tokens.output + tokens.cache_read + tokens.cache_write_5m + tokens.cache_write_1h
}

/// A change as a signed whole percentage, with a real minus sign: `+12%`, `−8%`.
fn signed_percent(now: f64, before: f64) -> Option<String> {
    if before <= 0.0 {
        return None;
    }
    let change = ((now - before) / before * 100.0).round();
    #[allow(clippy::cast_possible_truncation)]
    let change = change as i64;
    Some(format!(
        "{}{}%",
        if change >= 0 { "+" } else { "−" },
        change.abs()
    ))
}

/// The cost and tokens of the days from `first` to `last`.
fn sum(by_day: &[Day], first: Date, last: Date) -> (f64, u64) {
    by_day
        .iter()
        .filter(|entry| (first..=last).contains(&day(&entry.date)))
        .fold((0.0, 0), |(cost, tokens), entry| {
            (
                cost + entry.totals.cost.total,
                tokens + tokens_of(&entry.totals),
            )
        })
}

/// Each day's cost from `first` to `last`, with nothing on days without requests.
pub fn daily_cost(costs: impl Iterator<Item = (Date, f64)>, first: Date, last: Date) -> Vec<f64> {
    let costs: std::collections::HashMap<Date, f64> = costs.collect();
    let mut out = Vec::new();
    let mut at = first;
    while at <= last {
        out.push(costs.get(&at).copied().unwrap_or(0.0));
        let Ok(next) = at.checked_add(1.day()) else {
            break;
        };
        at = next;
    }
    out
}

/// Today, this week and this month: in three boxes with sparklines in the wide layout, and on one
/// line in the narrow one.
fn tiles(c: &mut Canvas, state: &State, top: i32) {
    let theme = c.theme;
    let (Some(status), Some(recent)) = (
        state.status(),
        state
            .tiles_request()
            .and_then(|request| state.overview(&request)),
    ) else {
        if state.wide() {
            let column = c.w / 3;
            for (index, label) in (0..).zip(["Today", "This week", "This month"]) {
                let x = index * column;
                let w = if index == 2 { c.w - x } else { column };
                c.boxed((x, top, w, 4), Some(label), None);
            }
        } else {
            c.boxed((0, top, c.w, 3), Some("Cost, API-equivalent"), None);
        }
        return;
    };
    let today = day(&status.today);
    let yesterday = today.checked_sub(1.day()).unwrap_or(today);
    let week = Bucket::Week.start(today, state.week_start());
    let month = today.first_of_month();
    let back = |date: Date, days: i64| date.checked_sub(days.days()).unwrap_or(date);
    let todays = sum(&recent.by_day, today, today);
    let yesterdays = sum(&recent.by_day, yesterday, yesterday);
    let this_week = sum(&recent.by_day, week, today);
    let last_week = sum(&recent.by_day, back(week, 7), back(today, 7));
    let this_month = sum(&recent.by_day, month, today);
    if !state.wide() {
        c.boxed((0, top, c.w, 3), Some("Cost, API-equivalent"), None);
        let mut x = c.text(2, top + 1, "Today ", theme.muted());
        x = c.text(x, top + 1, &money(todays.0), theme.strong());
        x = c.text(x + 3, top + 1, "Week ", theme.muted());
        x = c.text(x, top + 1, &money(this_week.0), theme.strong());
        x = c.text(x + 3, top + 1, "Month ", theme.muted());
        c.text(x, top + 1, &money(this_month.0), theme.strong());
        return;
    }
    let still_reading = status.read_ms.is_none() && reading(state).is_some();
    let compared = |now: f64, before: f64, what: &str| match signed_percent(now, before) {
        Some(change) => format!("{change} on {what}"),
        None if still_reading => "earlier days still to read".to_owned(),
        None => format!("nothing {what}"),
    };
    let costs = || {
        recent
            .by_day
            .iter()
            .map(|entry| (day(&entry.date), entry.totals.cost.total))
    };
    let tiles = [
        (
            "Today",
            todays,
            compared(todays.0, yesterdays.0, "yesterday"),
            12,
        ),
        (
            "This week",
            this_week,
            compared(this_week.0, last_week.0, "last week"),
            18,
        ),
        (
            "This month",
            this_month,
            if still_reading {
                "so far, still reading".to_owned()
            } else {
                "month to date".to_owned()
            },
            30,
        ),
    ];
    let column = c.w / 3;
    for (index, (label, (cost, tokens), delta, days)) in (0..).zip(tiles) {
        let x = index * column;
        let w = if index == 2 { c.w - x } else { column };
        c.boxed((x, top, w, 4), Some(label), None);
        let value_end = c.text(x + 2, top + 1, &money(cost), theme.strong());
        c.text(
            value_end + 2,
            top + 1,
            &format!("{} tokens", compact(tokens)),
            theme.muted(),
        );
        c.text(x + 2, top + 2, &delta, theme.muted());
        let room = w - 6 - super::super::canvas::width(&delta);
        let shown = days.min(i64::from(room.max(0)));
        if shown > 0 {
            let values = daily_cost(costs(), back(today, shown - 1), today);
            c.right(
                x + w - 2,
                top + 2,
                &spark(&values, theme.ramp()),
                theme.accent(),
            );
        }
    }
}

/// The projects that cost the most, and the rest together, with bars for their share.
fn top_projects(
    c: &mut Canvas,
    state: &State,
    overview: &Overview,
    (x0, y0, w, h): (i32, i32, i32, i32),
) {
    let theme = c.theme;
    c.boxed(
        (x0, y0, w, h),
        Some(&format!("Top projects · {}", state.range.title())),
        None,
    );
    let rows = &overview.projects;
    if rows.is_empty() {
        c.text(
            x0 + 2,
            y0 + 1,
            &fit("No projects in these days.", w - 4),
            theme.muted(),
        );
        return;
    }
    let shown = if rows.len() > 6 {
        &rows[..5]
    } else {
        &rows[..]
    };
    let mut items: Vec<(String, f64)> = shown
        .iter()
        .map(|row| (clean(&row.project.name), row.totals.cost.total))
        .collect();
    if rows.len() > shown.len() {
        let rest: f64 = rows[shown.len()..]
            .iter()
            .map(|row| row.totals.cost.total)
            .sum();
        items.push(("Other projects".to_owned(), rest));
    }
    let largest = items.iter().map(|(_, cost)| *cost).fold(0.0, f64::max);
    let bar = if state.wide() { 12 } else { 6 };
    let figures: Vec<String> = items.iter().map(|(_, cost)| money(*cost)).collect();
    let figure_width = figures
        .iter()
        .map(|figure| super::super::canvas::width(figure))
        .max()
        .unwrap_or(0);
    let bar_end = (x0 + w - 10).min(x0 + w - 2 - figure_width - 1);
    let rows_room = usize::try_from(h - 2).unwrap_or(0);
    for ((index, (name, cost)), figure) in (0..).zip(&items).zip(&figures).take(rows_room) {
        let y = y0 + 1 + index;
        let name_room = bar_end - bar - 1 - (x0 + 2);
        let style = if name == "Other projects" && rows.len() > shown.len() {
            theme.muted()
        } else {
            theme.text()
        };
        c.text(x0 + 2, y, &fit(name, name_room), style);
        let share = if largest > 0.0 { cost / largest } else { 0.0 };
        c.meter(bar_end - bar, y, bar, share, theme.muted());
        c.right(x0 + w - 2, y, figure, theme.strong());
    }
}

/// The models in the chosen days: tokens, cache reads, cost and share, and how the prices
/// compare with Claude Code's own figures.
fn models(c: &mut Canvas, state: &State, overview: &Overview, (top, h): (i32, i32)) {
    let theme = c.theme;
    let wide = state.wide();
    let w = c.w;
    c.boxed(
        (0, top, w, h),
        Some(&format!("Models · {}", state.range.title())),
        None,
    );
    // Columns end at the box's right, as on the 120 and 80 column boards.
    let columns: Vec<(&str, i32)> = if wide {
        vec![
            ("Requests", w - 86),
            ("Tokens", w - 70),
            ("Cache reads", w - 52),
            ("API-equivalent cost", w - 28),
            ("Share", w - 16),
        ]
    } else {
        vec![("Tokens", w - 46), ("Cost", w - 30), ("Share", w - 18)]
    };
    c.text(2, top + 1, "Model", theme.muted());
    for (label, end) in &columns {
        c.right(*end, top + 1, label, theme.muted());
    }
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
    // Below the models, the wide layout has the total, a blank line and the price check.
    let below = if wide { 3 } else { 0 };
    let lines = usize::try_from(h - 3 - below).unwrap_or(0);
    let (shown, more) = fold(&overview.models, lines);
    let total = overview.totals.cost.total;
    let mut y = top + 2;
    for row in shown {
        let of = series.get(&row.model.id).copied().unwrap_or(Series::Other);
        model_line(
            c,
            (y, &clean(&row.model.name), Some(of)),
            &row.totals,
            &columns,
            total,
        );
        y += 1;
    }
    if !more.is_empty() {
        let name = if shown.is_empty() {
            format!("{} models", more.len())
        } else {
            format!("{} more models", more.len())
        };
        model_line(c, (y, &name, None), &added_up(more), &columns, total);
        y += 1;
    }
    let (cost_end, share_end) = (columns[columns.len() - 2].1, columns[columns.len() - 1].1);
    if wide && y < top + h - 1 {
        c.text(4, y, "Total", theme.strong());
        c.right(cost_end, y, &cost_text(&overview.totals), theme.strong());
        c.right(share_end, y, "100%", theme.text());
        let line = y + 2;
        if line < top + h - 1 {
            price_line(c, state, overview, line);
        }
    }
}

/// The models that `lines` lines have room for, most costly first, and the rest, which the last
/// line adds up.
fn fold(models: &[ModelRow], lines: usize) -> (&[ModelRow], &[ModelRow]) {
    if models.len() <= lines {
        (models, &[])
    } else if lines == 0 {
        (&[], &[])
    } else {
        models.split_at(lines - 1)
    }
}

/// Models' use added up.
fn added_up(models: &[ModelRow]) -> Totals {
    let mut sum = Totals::default();
    for row in models {
        let more = &row.totals;
        sum.requests += more.requests;
        sum.tokens.input += more.tokens.input;
        sum.tokens.output += more.tokens.output;
        sum.tokens.cache_read += more.tokens.cache_read;
        sum.tokens.cache_write_5m += more.tokens.cache_write_5m;
        sum.tokens.cache_write_1h += more.tokens.cache_write_1h;
        sum.cost.total += more.cost.total;
        sum.unpriced_requests += more.unpriced_requests;
    }
    sum
}

/// One line of the models, for a model in its series' colour or for several added up: its
/// tokens, and in the wide layout its requests and cache reads, then its cost and its share of
/// `total`. The columns end with the cost's and the share's.
fn model_line(
    c: &mut Canvas,
    (y, name, of): (i32, &str, Option<Series>),
    totals: &Totals,
    columns: &[(&str, i32)],
    total: f64,
) {
    let theme = c.theme;
    let (cost_end, share_end) = (columns[columns.len() - 2].1, columns[columns.len() - 1].1);
    let name_style = match of {
        Some(of) => {
            c.put(2, y, theme.swatch(of), theme.series(of));
            theme.text()
        }
        None => theme.muted(),
    };
    c.text(4, y, &fit(name, columns[0].1 - 12), name_style);
    let figures: Vec<String> = if columns.len() > 3 {
        vec![
            grouped(totals.requests),
            compact(tokens_of(totals)),
            compact(totals.tokens.cache_read),
        ]
    } else {
        vec![compact(tokens_of(totals))]
    };
    for ((_, end), figure) in columns.iter().zip(&figures) {
        c.right(*end, y, figure, theme.text());
    }
    if totals.requests > 0 && totals.unpriced_requests == totals.requests {
        let start = c.right(cost_end, y, "unpriced", theme.warn_text());
        c.put(start - 2, y, "▲", theme.warn());
        c.right(share_end, y, "–", theme.muted());
    } else {
        c.right(cost_end, y, &cost_text(totals), theme.strong());
        let share = if total > 0.0 {
            totals.cost.total / total * 100.0
        } else {
            0.0
        };
        c.right(share_end, y, &percent(share), theme.text());
    }
}

/// How the estimate is made: the price table's date, how it agrees with Claude Code's own session
/// costs, and how much input the prompt cache served.
fn price_line(c: &mut Canvas, state: &State, overview: &Overview, y: i32) {
    let theme = c.theme;
    let Some(prices) = state.price_check() else {
        return;
    };
    let mut parts = vec![format!("Prices of {}", short_date(day(&prices.checked)))];
    let out_of_line = prices
        .out_of_line
        .iter()
        .map(|model| model.drift.abs())
        .reduce(f64::max);
    match (out_of_line, prices.drift) {
        (Some(drift), _) => parts.push(format!(
            "{:.1}% off Claude Code's own session costs",
            drift * 100.0
        )),
        (None, Some(drift)) => parts.push(format!(
            "within {:.1}% of Claude Code's own session costs",
            (drift.abs() * 100.0).max(0.1)
        )),
        (None, None) => {}
    }
    if tokens_of(&overview.totals) > 0 {
        parts.push(format!(
            "{} of input read from cache",
            percent(cache_share(&overview.totals) * 100.0)
        ));
    }
    let text = parts.join(" · ");
    if out_of_line.is_some() {
        let x = c.text(2, y, "▲ ", theme.warn());
        c.text(x, y, &fit(&text, c.w - 3 - x), theme.text());
    } else {
        c.text(2, y, &fit(&text, c.w - 4), theme.muted());
    }
}

/// The share of input tokens read from the prompt cache.
pub fn cache_share(totals: &Totals) -> f64 {
    let tokens = &totals.tokens;
    let all = tokens.input + tokens.cache_read + tokens.cache_write_5m + tokens.cache_write_1h;
    if all == 0 {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        let share = tokens.cache_read as f64 / all as f64;
        share
    }
}

/// API-equivalent cost, saying when some requests could not be priced: `$5.05 + unpriced`.
pub fn cost_text(totals: &Totals) -> String {
    if totals.requests > 0 && totals.unpriced_requests == totals.requests {
        "unpriced".to_owned()
    } else if totals.unpriced_requests > 0 {
        format!("{} + unpriced", money(totals.cost.total))
    } else {
        money(totals.cost.total)
    }
}
