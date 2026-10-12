//! Models: cost per day by model, each model's tokens by kind, its share of fast mode and of the
//! cost, and the prices the estimate uses. A page that scrolls when it does not fit.

use claude_dashboard_server::fmt::{compact, grouped, percent, price, short_date};
use claude_dashboard_server::protocol::{ModelRow, Overview, PriceRow, Prices};

use super::chart;
use super::overview::{cost_chart, cost_text, tokens_of};
use super::{KEYS, footer, header, notice, scrolled, series_of};
use crate::tui::canvas::{Canvas, clean, fit};
use crate::tui::state::{Section, State, day};
use crate::tui::theme::Series;

/// The wide layout needs the 120 columns it was drawn for.
fn wide(state: &State) -> bool {
    state.wide() && state.width >= 120
}

fn overview(state: &State) -> Option<&Overview> {
    state
        .models_request()
        .and_then(|request| state.overview(&request))
}

fn prices(state: &State) -> Option<&Prices> {
    state
        .models_prices_request()
        .and_then(|request| state.prices(&request))
}

/// The lines under the prices: each model's long-prompt prices, and in the narrow layout its
/// fast-mode prices.
fn price_notes(state: &State, prices: Option<&Prices>) -> Vec<String> {
    let Some(prices) = prices else {
        return Vec::new();
    };
    let mut notes = Vec::new();
    for row in &prices.models {
        let name = clean(&row.model.name);
        if let Some(long) = &row.long_prompt {
            notes.push(format!(
                "{name} above {} tokens of prompt: input {}, output {}, cache writes {} and {}, cache read {}",
                compact(long.above),
                price(long.input),
                price(long.output),
                price(long.cache_write_5m),
                price(long.cache_write_1h),
                price(long.cache_read)
            ));
        }
        if !wide(state)
            && let Some(fast) = row.fast
        {
            notes.push(format!(
                "{name} in fast mode: input {}, output {}",
                price(fast.input),
                price(fast.output)
            ));
        }
    }
    notes
}

/// The rows of the page: the chart, the tokens and cost, the notes under them, the prices.
struct Layout {
    chart: (i32, i32),
    tokens: (i32, i32),
    /// Where the line about an unpriced model goes, and the hint that prices follow.
    notes: i32,
    prices: (i32, i32),
}

fn unpriced(overview: &Overview) -> Vec<String> {
    overview
        .models
        .iter()
        .filter(|row| row.model.name == row.model.id)
        .map(|row| clean(&row.model.id))
        .collect()
}

/// The page's rows, with the chart taller by any rows the screen has to spare.
fn layout(state: &State) -> Layout {
    let base = arrange(state, if wide(state) { 12 } else { 11 });
    let spare = super::page_rows(state) - (base.prices.0 + base.prices.1 + 1);
    if spare > 0 {
        arrange(state, base.chart.1 + spare)
    } else {
        base
    }
}

fn arrange(state: &State, chart_h: i32) -> Layout {
    let wide = wide(state);
    let models = overview(state).map_or(0, |overview| overview.models.len());
    let rows = i32::try_from(models).unwrap_or(0);
    let unpriced = overview(state).map_or(0, |overview| unpriced(overview).len());
    let priced_rows =
        i32::try_from(prices(state).map_or(4, |prices| prices.models.len())).unwrap_or(4);
    let notes = i32::try_from(price_notes(state, prices(state)).len()).unwrap_or(0);
    if wide {
        let chart = (0, chart_h);
        let tokens = (chart_h, (rows + 6).max(10));
        let prices_top = tokens.0 + tokens.1;
        Layout {
            chart,
            tokens,
            notes: prices_top,
            prices: (prices_top, (priced_rows + notes + 4).max(9)),
        }
    } else {
        let chart = (0, chart_h);
        let total_row = i32::from(unpriced == 0);
        let tokens = (chart_h, (rows + 3 + total_row).max(8));
        let notes_top = tokens.0 + tokens.1;
        let unpriced_rows = if unpriced > 0 { 2 } else { 0 };
        // The hint that the prices follow, and a row apart from them.
        let prices_top = notes_top + unpriced_rows + 2;
        Layout {
            chart,
            tokens,
            notes: notes_top,
            prices: (prices_top, priced_rows + notes + 4),
        }
    }
}

/// How tall the page is.
pub fn height(state: &State) -> i32 {
    let layout = layout(state);
    layout.prices.0 + layout.prices.1 + 1
}

pub fn draw(c: &mut Canvas, state: &State) {
    header(c, state, Some(Section::Models));
    notice(c, state, "", false);
    if let Some(overview) = overview(state) {
        scrolled(c, state, height(state), |page| {
            draw_page(page, state, overview);
        });
    } else {
        c.text(2, 2, "Loading the models…", c.theme.muted());
    }
    // A page taller than the screen scrolls, and its key line says how.
    let keys: &[(&str, &str)] = if super::page_overflow(state) > 0 {
        &[
            ("q", "quit"),
            ("←→", "sections"),
            ("↑↓", "scroll"),
            ("t", "table view"),
            ("e", "export"),
            ("?", "help"),
        ]
    } else {
        &KEYS
    };
    footer(c, state, keys, None);
}

fn draw_page(c: &mut Canvas, state: &State, overview: &Overview) {
    let theme = c.theme;
    let wide = wide(state);
    let w = c.w;
    let layout = layout(state);
    let chart = cost_chart(state, overview, w - 10);
    let title = format!(
        "Cost per {} by model · {}",
        chart.per(),
        state.range.title()
    );
    let chart_box = (0, layout.chart.0, w, layout.chart.1);
    if state.tables {
        chart::table(c, chart_box, &chart, &title, state.table_top);
    } else {
        chart::draw(c, chart_box, &chart, &title);
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
    tokens_box(c, state, overview, &series, layout.tokens);
    let unpriced = unpriced(overview);
    let mut y = layout.notes;
    if !wide {
        if let Some(model) = unpriced.first() {
            let x = c.text(1, y, "▲ ", theme.warn());
            c.text(
                x,
                y,
                &fit(
                    &format!("No price for {model} yet: its tokens count, its cost does not."),
                    w - 1 - x,
                ),
                theme.text(),
            );
            c.text(
                3,
                y + 1,
                "claude-dashboard prices update looks for its price.",
                theme.muted(),
            );
            y += 2;
        }
        c.text(2, y, "↓ prices used", theme.muted());
    }
    let prices = prices(state);
    prices_box(c, state, prices, &series, layout.prices);
    if let Some(check) = state.price_check() {
        let line = layout.prices.0 + layout.prices.1;
        let text = match (check.out_of_line.first(), check.drift) {
            (Some(model), _) => Some(format!(
                "{} is {:.1}% off Claude Code's own session costs: the price table may be out of date",
                clean(&model.model.name),
                model.drift.abs() * 100.0
            )),
            (None, Some(drift)) => Some(format!(
                "Within {:.1}% of Claude Code's own session costs, over {} {}",
                (drift.abs() * 100.0).max(0.1),
                grouped(check.sessions_compared),
                if check.sessions_compared == 1 {
                    "session"
                } else {
                    "sessions"
                }
            )),
            (None, None) => None,
        };
        if let Some(text) = text {
            c.text(2, line, &fit(&text, w - 4), theme.muted());
        }
    }
}

/// The column a table's `label` heads, by where it ends.
fn end_of(columns: &[(&str, i32)], label: &str) -> i32 {
    columns
        .iter()
        .find(|(name, _)| *name == label)
        .map_or(0, |(_, end)| *end)
}

/// Each model's requests, tokens by kind, fast mode, cost and share.
fn tokens_box(
    c: &mut Canvas,
    state: &State,
    overview: &Overview,
    series: &std::collections::HashMap<String, Series>,
    (top, h): (i32, i32),
) {
    let theme = c.theme;
    let wide = wide(state);
    let w = c.w;
    c.boxed(
        (0, top, w, h),
        Some(&format!("Tokens and cost · {}", state.range.title())),
        None,
    );
    let unpriced_any = overview
        .models
        .iter()
        .any(|row| row.model.name == row.model.id);
    let (heading, first) = if wide {
        (top + 2, top + 3)
    } else {
        (top + 1, top + 2)
    };
    let columns: Vec<(&str, i32)> = if wide {
        c.text(w - 64, top + 1, "Cache writes", theme.muted());
        c.right(w - 33, top + 1, "Cache", theme.muted());
        vec![
            ("Requests", w - 92),
            ("Input", w - 83),
            ("Output", w - 74),
            ("5 min", w - 59),
            ("1 h", w - 46),
            ("reads", w - 33),
            ("Fast", w - 26),
            ("Cost", w - 14),
            ("Share", w - 3),
        ]
    } else {
        vec![
            ("Requests", w - 48),
            ("Tokens", w - 34),
            ("Cost", w - 18),
            ("Share", w - 4),
        ]
    };
    c.text(4, heading, "Model", theme.muted());
    for (label, end) in &columns {
        c.right(*end, heading, label, theme.muted());
    }
    let mut y = first;
    for row in &overview.models {
        if y >= top + h - 1 {
            break;
        }
        token_row(c, row, y, &columns, (overview.totals.cost.total, series));
        y += 1;
    }
    if (wide || !unpriced_any) && y < top + h - 1 {
        c.text(4, y, "Total", theme.strong());
        c.right(
            end_of(&columns, "Cost"),
            y,
            &cost_text(&overview.totals),
            theme.strong(),
        );
        c.right(end_of(&columns, "Share"), y, "100%", theme.text());
    }
}

/// One model's row of the table: its tokens by kind in the wide layout, all its tokens in the
/// narrow one, and its cost and share of `total`.
fn token_row(
    c: &mut Canvas,
    row: &ModelRow,
    y: i32,
    columns: &[(&str, i32)],
    (total, series): (f64, &std::collections::HashMap<String, Series>),
) {
    let theme = c.theme;
    let end = |label: &str| end_of(columns, label);
    let unpriced = row.model.name == row.model.id;
    let of = series.get(&row.model.id).copied().unwrap_or(Series::Other);
    if unpriced {
        c.put(2, y, "?", theme.muted());
    } else {
        c.put(2, y, theme.swatch(of), theme.series(of));
    }
    c.text(
        4,
        y,
        &fit(&clean(&row.model.name), end("Requests") - 13),
        theme.text(),
    );
    let totals = &row.totals;
    let tokens = &totals.tokens;
    c.right(end("Requests"), y, &grouped(totals.requests), theme.text());
    if end("Input") > 0 {
        for (label, count) in [
            ("Input", tokens.input),
            ("Output", tokens.output),
            ("5 min", tokens.cache_write_5m),
            ("1 h", tokens.cache_write_1h),
            ("reads", tokens.cache_read),
        ] {
            c.right(end(label), y, &compact(count), theme.text());
        }
        if totals.fast_requests > 0 && totals.requests > 0 {
            #[allow(clippy::cast_precision_loss)]
            let share = totals.fast_requests as f64 / totals.requests as f64 * 100.0;
            c.right(end("Fast"), y, &percent(share), theme.text());
        }
    } else {
        c.right(end("Tokens"), y, &compact(tokens_of(totals)), theme.text());
    }
    if unpriced {
        let start = c.right(end("Cost"), y, "unpriced", theme.warn_text());
        c.put(start - 2, y, "▲", theme.warn());
        c.right(end("Share"), y, "–", theme.muted());
    } else {
        c.right(end("Cost"), y, &cost_text(totals), theme.strong());
        let share = if total > 0.0 {
            totals.cost.total / total * 100.0
        } else {
            0.0
        };
        c.right(end("Share"), y, &percent(share), theme.text());
    }
}

/// The prices of the models listed, per million tokens, with the notes on long prompts and fast
/// mode.
fn prices_box(
    c: &mut Canvas,
    state: &State,
    prices: Option<&Prices>,
    series: &std::collections::HashMap<String, Series>,
    (top, h): (i32, i32),
) {
    let theme = c.theme;
    let wide = wide(state);
    let w = c.w;
    let note = state.price_check().map(|check| {
        format!(
            "per million tokens · table of {}",
            short_date(day(&check.checked))
        )
    });
    c.boxed(
        (0, top, w, h),
        Some("Prices used"),
        note.as_deref().map(|note| (note, theme.muted())),
    );
    let columns: Vec<(&str, i32)> = if wide {
        vec![
            ("Input", w - 90),
            ("Output", w - 78),
            ("Cache write 5 min", w - 58),
            ("Cache write 1 h", w - 40),
            ("Cache read", w - 26),
            ("Fast in / out", w - 3),
        ]
    } else {
        vec![
            ("Input", w - 50),
            ("Output", w - 40),
            ("Write 5m", w - 28),
            ("Write 1h", w - 16),
            ("Read", w - 4),
        ]
    };
    let heading = top + 1;
    c.text(4, heading, "Model", theme.muted());
    for (label, end) in &columns {
        c.right(*end, heading, label, theme.muted());
    }
    let Some(prices) = prices else {
        return;
    };
    let mut y = heading + 1;
    for row in &prices.models {
        price_row(c, row, series, &columns, y);
        y += 1;
    }
    for note in price_notes(state, Some(prices)) {
        c.text(4, y, &fit(&note, w - 6), theme.muted());
        y += 1;
    }
}

fn price_row(
    c: &mut Canvas,
    row: &PriceRow,
    series: &std::collections::HashMap<String, Series>,
    columns: &[(&str, i32)],
    y: i32,
) {
    let theme = c.theme;
    let of = series.get(&row.model.id).copied().unwrap_or(Series::Other);
    c.put(2, y, theme.swatch(of), theme.series(of));
    let first = columns.first().map_or(30, |(_, end)| *end);
    c.text(
        4,
        y,
        &fit(&clean(&row.model.name), first - 9 - 4),
        theme.text(),
    );
    let values = [
        Some(price(row.input)),
        Some(price(row.output)),
        Some(price(row.cache_write_5m)),
        Some(price(row.cache_write_1h)),
        Some(price(row.cache_read)),
        row.fast
            .map(|fast| format!("{} / {}", price(fast.input), price(fast.output))),
    ];
    for ((_, end), value) in columns.iter().zip(values) {
        if let Some(value) = value {
            c.right(*end, y, &value, theme.text());
        }
    }
}
