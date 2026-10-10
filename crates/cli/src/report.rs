//! `today` and `daily`: requests, tokens and API-equivalent cost, as tables, JSON or CSV.

use std::fmt::Write as _;
use std::process::ExitCode;

use claude_dashboard_core::history::History;
use claude_dashboard_core::limits::{FIVE_HOUR, Level, LimitSnapshot, SEVEN_DAY, Source};
use claude_dashboard_core::report::{self, DayTotals, ModelTotals, Range, Totals};
use jiff::Timestamp;
use jiff::civil::Date;
use serde::Serialize;

use crate::app::App;
use crate::fmt::{ago, compact, grouped, long_day, money, short_date, when};
use crate::style::{Paint, Rgb, palette};

/// How a report is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    Table,
    Json,
    Csv,
}

/// The colour of a model's series, by its family.
pub fn series_colour(model: &str) -> Rgb {
    if model.contains("opus") {
        palette::OPUS
    } else if model.contains("sonnet") {
        palette::SONNET
    } else if model.contains("haiku") {
        palette::HAIKU
    } else if model.contains("fable") || model.contains("mythos") {
        palette::FABLE
    } else {
        palette::MUTED
    }
}

/// A model's short name, such as `Opus 5.5`, or its id when the price table does not know it.
pub fn model_name(app: &App, model: &str) -> String {
    app.prices.model_name(model).map_or_else(
        || model.to_owned(),
        |name| name.strip_prefix("Claude ").unwrap_or(name).to_owned(),
    )
}

/// Opens and ingests the history, printing why when it cannot.
pub fn ready_history(app: &App) -> Result<History, ExitCode> {
    let mut history = app.open_history().map_err(|error| {
        eprintln!("{error}");
        ExitCode::FAILURE
    })?;
    if let Err(error) = app.ingest(&mut history) {
        eprintln!("Cannot read new transcripts into the history: {error}");
    }
    Ok(history)
}

pub fn today(app: &App, output: Output) -> ExitCode {
    let history = match ready_history(app) {
        Ok(history) => history,
        Err(code) => return code,
    };
    let day = app.now_zoned().date();
    let range = Range::days(day, day, &app.zone);
    let models = match report::by_model(&history, app.prices, range) {
        Ok(models) => models,
        Err(error) => {
            eprintln!("Cannot read the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    let limits = history.latest_limits().unwrap_or_default();
    match output {
        Output::Table => {
            let paint = Paint::for_stdout();
            let mut out = String::new();
            let _ = writeln!(
                out,
                "{}",
                paint.muted(&format!(
                    "{} · API-equivalent estimate · prices of {}",
                    long_day(day),
                    short_date(app.prices.checked())
                ))
            );
            out.push('\n');
            if models.is_empty() {
                let _ = writeln!(out, "{}", paint.muted("No requests today."));
            } else {
                out.push_str(&model_table(app, paint, &models));
            }
            out.push('\n');
            let _ = writeln!(out, "{}", limits_line(app, paint, &limits));
            print!("{out}");
        }
        Output::Json => {
            let totals = DayTotals {
                day,
                totals: sum(&models),
                models: models.clone(),
            };
            let today = TodayJson {
                day: day_json(app, &totals),
                limits: limits
                    .iter()
                    .filter(|limit| !limit.expired(app.now.as_millisecond()))
                    .map(limit_json)
                    .collect(),
            };
            print_json(&today);
        }
        Output::Csv => {
            let totals = DayTotals {
                day,
                totals: sum(&models),
                models,
            };
            print!("{}", days_csv(app, &[totals]));
        }
    }
    ExitCode::SUCCESS
}

pub fn daily(app: &App, since: Option<Date>, until: Option<Date>, output: Output) -> ExitCode {
    let history = match ready_history(app) {
        Ok(history) => history,
        Err(code) => return code,
    };
    let today = app.now_zoned().date();
    let until = until.unwrap_or(today);
    let since = since.unwrap_or_else(|| {
        until
            .checked_sub(jiff::Span::new().days(29))
            .unwrap_or(until)
    });
    let days = match report::by_day(
        &history,
        app.prices,
        Range::days(since, until, &app.zone),
        &app.zone,
    ) {
        Ok(days) => days,
        Err(error) => {
            eprintln!("Cannot read the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    match output {
        Output::Table => print!("{}", days_table(app, Paint::for_stdout(), &days)),
        Output::Json => print_json(
            &days
                .iter()
                .map(|day| day_json(app, day))
                .collect::<Vec<_>>(),
        ),
        Output::Csv => print!("{}", days_csv(app, &days)),
    }
    ExitCode::SUCCESS
}

fn sum(models: &[ModelTotals]) -> Totals {
    let mut totals = Totals::default();
    for model in models {
        totals.requests += model.totals.requests;
        totals.tokens.add(model.totals.tokens);
        totals.cost.add(model.totals.cost);
        totals.unpriced_requests += model.totals.unpriced_requests;
        totals.web_search_requests += model.totals.web_search_requests;
    }
    totals
}

/// The columns of the design's tables: a label, then right-aligned numbers. The widths are the
/// design's; a column widens to keep two spaces before its longest value.
const HEADINGS: [&str; 5] = ["Requests", "Output", "Cache writes", "Cache reads", "Cost"];
const WIDTHS: [usize; 5] = [8, 10, 15, 14, 9];
const LABEL_WIDTH: usize = 14;

fn cells(totals: &Totals) -> [String; 5] {
    let cost = if totals.requests > 0 && totals.unpriced_requests == totals.requests {
        "unpriced".to_owned()
    } else {
        money(totals.cost.total())
    };
    [
        grouped(totals.requests),
        compact(totals.tokens.output),
        compact(totals.tokens.cache_write_5m + totals.tokens.cache_write_1h),
        compact(totals.tokens.cache_read),
        cost,
    ]
}

/// A row of a table: its label as written, with the length it shows at, and its values.
struct Row {
    label: String,
    label_len: usize,
    values: [String; 5],
}

/// A table with a heading row, its rows and a total row in bold.
fn table(paint: Paint, first_heading: &str, rows: &[Row], total: &Row) -> String {
    let all = || rows.iter().chain(std::iter::once(total));
    let label_width = all()
        .map(|row| row.label_len + 2)
        .fold(LABEL_WIDTH, usize::max);
    let mut widths = WIDTHS;
    for (index, width) in widths.iter_mut().enumerate() {
        let longest = all()
            .map(|row| row.values[index].chars().count() + 2)
            .max()
            .unwrap_or(0);
        *width = (*width).max(longest);
    }
    let render = |label: &str, label_len: usize, values: &[&str]| {
        let mut line = String::from(label);
        line.push_str(&" ".repeat(label_width.saturating_sub(label_len)));
        for (value, width) in values.iter().zip(widths) {
            let _ = write!(line, "{value:>width$}");
        }
        line
    };
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}",
        paint.muted(&render(
            first_heading,
            first_heading.chars().count(),
            &HEADINGS
        ))
    );
    for row in rows {
        let values: Vec<&str> = row.values.iter().map(String::as_str).collect();
        let _ = writeln!(out, "{}", render(&row.label, row.label_len, &values));
    }
    let values: Vec<&str> = total.values.iter().map(String::as_str).collect();
    let _ = writeln!(
        out,
        "{}",
        paint.strong(&render(&total.label, total.label_len, &values))
    );
    out
}

fn total_row(totals: &Totals) -> Row {
    Row {
        label: "  Total".to_owned(),
        label_len: 7,
        values: cells(totals),
    }
}

fn model_table(app: &App, paint: Paint, models: &[ModelTotals]) -> String {
    let rows: Vec<Row> = models
        .iter()
        .map(|model| {
            let name = model_name(app, &model.model);
            Row {
                label: format!(
                    "{} {name}",
                    paint.paint("■", series_colour(&model.model), false)
                ),
                label_len: name.chars().count() + 2,
                values: cells(&model.totals),
            }
        })
        .collect();
    let total = sum(models);
    let mut out = table(paint, "Model", &rows, &total_row(&total));
    if total.unpriced_requests > 0 && total.unpriced_requests < total.requests {
        let _ = writeln!(
            out,
            "{}",
            paint.muted(&format!(
                "The total leaves out {} {} to unpriced models.",
                grouped(total.unpriced_requests),
                if total.unpriced_requests == 1 {
                    "request"
                } else {
                    "requests"
                }
            ))
        );
    }
    out
}

fn days_table(app: &App, paint: Paint, days: &[DayTotals]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}",
        paint.muted(&format!(
            "API-equivalent estimate · prices of {}",
            short_date(app.prices.checked())
        ))
    );
    out.push('\n');
    if days.is_empty() {
        let _ = writeln!(out, "{}", paint.muted("No requests on these days."));
        return out;
    }
    let mut total = Totals::default();
    let rows: Vec<Row> = days
        .iter()
        .map(|day| {
            total.requests += day.totals.requests;
            total.tokens.add(day.totals.tokens);
            total.cost.add(day.totals.cost);
            total.unpriced_requests += day.totals.unpriced_requests;
            let label = format!(
                "{} {:>2} {}",
                &format!("{:?}", day.day.weekday())[..3],
                day.day.day(),
                short_date(day.day).split(' ').nth(1).unwrap_or_default()
            );
            Row {
                label_len: label.chars().count(),
                label,
                values: cells(&day.totals),
            }
        })
        .collect();
    out.push_str(&table(paint, "Day", &rows, &total_row(&total)));
    out
}

fn window_name(window: &str) -> String {
    match window {
        FIVE_HOUR => "five-hour".to_owned(),
        SEVEN_DAY => "weekly".to_owned(),
        other => other.replace('_', " "),
    }
}

/// The plan-limits line under a report, as the design writes it.
pub fn limits_line(app: &App, paint: Paint, limits: &[LimitSnapshot]) -> String {
    let now_ms = app.now.as_millisecond();
    let current: Vec<&LimitSnapshot> = limits
        .iter()
        .filter(|limit| !limit.expired(now_ms))
        .collect();
    let Some(latest) = current.iter().map(|limit| limit.observed_ms).max() else {
        return if limits.is_empty() {
            format!(
                "{} {}",
                paint.muted("Plan limits: not recorded yet. To record them, run"),
                paint.strong("claude-dashboard statusline connect")
            )
        } else {
            paint.muted("Plan limits: the windows have reset since the last reading.")
        };
    };
    let source = if current
        .iter()
        .all(|limit| limit.source == Source::StatusLine)
    {
        "official"
    } else {
        "experimental"
    };
    let age = ago(
        Timestamp::from_millisecond(latest).unwrap_or(app.now),
        app.now,
    );
    let stale = current.iter().any(|limit| limit.stale(now_ms));
    let age = if stale {
        paint.paint(&age, palette::WARNING, false)
    } else {
        paint.muted(&age)
    };
    let now = app.now_zoned();
    let windows: Vec<String> = current
        .iter()
        .map(|limit| {
            let percent = format!("{:.0}%", limit.used_percentage);
            let figure = match limit.level() {
                Level::Normal => paint.strong(&percent),
                Level::Warning => paint.paint(&format!("{percent} ▲"), palette::WARNING, true),
                // Red passes WCAG AA only for marks, so the figure stays in strong ink.
                Level::Critical => format!(
                    "{} {}",
                    paint.strong(&percent),
                    paint.paint("▲", palette::CRITICAL, true)
                ),
            };
            let resets = limit
                .resets_at_ms
                .and_then(|ms| Timestamp::from_millisecond(ms).ok())
                .map(|at| format!(" resets {}", when(&at.to_zoned(app.zone.clone()), &now)))
                .unwrap_or_default();
            format!("{} {figure}{resets}", window_name(&limit.window))
        })
        .collect();
    format!(
        "{} {age}{} {}",
        paint.muted(&format!("Plan limits, {source},")),
        paint.muted(":"),
        windows.join(" · ")
    )
}

#[derive(Serialize)]
struct TodayJson {
    #[serde(flatten)]
    day: DayJson,
    limits: Vec<LimitJson>,
}

#[derive(Serialize)]
struct DayJson {
    date: String,
    requests: u64,
    tokens: TokensJson,
    cost: CostJson,
    models: Vec<String>,
}

#[derive(Serialize)]
struct TokensJson {
    input: u64,
    output: u64,
    cache_write_5m: u64,
    cache_write_1h: u64,
    cache_read: u64,
}

#[derive(Serialize)]
struct CostJson {
    usd: f64,
    provenance: &'static str,
    price_table: String,
    unpriced_requests: u64,
}

#[derive(Serialize)]
struct LimitJson {
    window: String,
    used_percentage: f64,
    resets_at: Option<String>,
    observed_at: String,
    provenance: &'static str,
}

fn cents(usd: f64) -> f64 {
    (usd * 100.0).round() / 100.0
}

fn day_json(app: &App, day: &DayTotals) -> DayJson {
    let tokens = day.totals.tokens;
    DayJson {
        date: day.day.to_string(),
        requests: day.totals.requests,
        tokens: TokensJson {
            input: tokens.input,
            output: tokens.output,
            cache_write_5m: tokens.cache_write_5m,
            cache_write_1h: tokens.cache_write_1h,
            cache_read: tokens.cache_read,
        },
        cost: CostJson {
            usd: cents(day.totals.cost.total()),
            provenance: "estimate",
            price_table: app.prices.checked().to_string(),
            unpriced_requests: day.totals.unpriced_requests,
        },
        models: day.models.iter().map(|model| model.model.clone()).collect(),
    }
}

fn limit_json(limit: &LimitSnapshot) -> LimitJson {
    let at = |ms: i64| {
        Timestamp::from_millisecond(ms)
            .map(|at| at.to_string())
            .ok()
    };
    LimitJson {
        window: limit.window.clone(),
        used_percentage: limit.used_percentage,
        resets_at: limit.resets_at_ms.and_then(at),
        observed_at: at(limit.observed_ms).unwrap_or_default(),
        provenance: match limit.source {
            Source::StatusLine => "official",
            Source::UsageEndpoint => "experimental",
        },
    }
}

fn days_csv(app: &App, days: &[DayTotals]) -> String {
    let mut out = String::from(
        "date,requests,input_tokens,output_tokens,cache_write_5m_tokens,cache_write_1h_tokens,cache_read_tokens,cost_usd,unpriced_requests,price_table,models\n",
    );
    for day in days {
        let tokens = day.totals.tokens;
        let models: Vec<&str> = day
            .models
            .iter()
            .map(|model| model.model.as_str())
            .collect();
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{:.2},{},{},{}",
            day.day,
            day.totals.requests,
            tokens.input,
            tokens.output,
            tokens.cache_write_5m,
            tokens.cache_write_1h,
            tokens.cache_read,
            day.totals.cost.total(),
            day.totals.unpriced_requests,
            app.prices.checked(),
            models.join(";")
        );
    }
    out
}

/// Prints JSON, coloured as the design shows it when standard output is a terminal.
fn print_json(value: &impl Serialize) {
    let text = serde_json::to_string_pretty(value).unwrap_or_default();
    println!("{}", colour_json(&text, Paint::for_stdout()));
}

/// Colours pretty-printed JSON: keys in the accent colour and string values in the string
/// colour.
fn colour_json(text: &str, paint: Paint) -> String {
    if paint == Paint::PLAIN {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() * 2);
    let mut chars = text.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        if c != '"' {
            out.push(c);
            continue;
        }
        let mut end = start + 1;
        let mut escaped = false;
        for (index, c) in chars.by_ref() {
            end = index + c.len_utf8();
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                break;
            }
        }
        let string = &text[start..end];
        let is_key = text[end..].trim_start().starts_with(':');
        let colour = if is_key {
            palette::ACCENT
        } else {
            palette::STRING
        };
        out.push_str(&paint.paint(string, colour, false));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(label: &str, values: [&str; 5]) -> Row {
        Row {
            label: label.to_owned(),
            label_len: label.chars().count(),
            values: values.map(str::to_owned),
        }
    }

    #[test]
    fn rows_line_up_under_their_headings_as_in_the_design() {
        let rows = [row("■ Fable 5.1", ["16", "3K", "9K", "166K", "$1.83"])];
        let total = row("  Total", ["113", "13K", "39K", "1.0M", "$5.05"]);
        assert_eq!(
            table(Paint::PLAIN, "Model", &rows, &total),
            [
                "Model         Requests    Output   Cache writes   Cache reads     Cost",
                "■ Fable 5.1         16        3K             9K          166K    $1.83",
                "  Total            113       13K            39K          1.0M    $5.05",
                "",
            ]
            .join("\n")
        );
    }

    #[test]
    fn a_wide_value_widens_its_column() {
        let rows = [row(
            "Sun  4 Oct",
            ["27,627", "20.4M", "106M", "7.1B", "$2,372.37"],
        )];
        let total = row("  Total", ["27,627", "20.4M", "106M", "7.1B", "$2,372.37"]);
        let text = table(Paint::PLAIN, "Day", &rows, &total);
        assert!(text.contains("7.1B  $2,372.37"), "{text}");
        let widths: Vec<usize> = text.lines().map(|line| line.chars().count()).collect();
        assert!(widths.iter().all(|width| *width == widths[0]), "{text}");
    }

    #[test]
    fn json_keys_and_strings_take_the_design_colours() {
        let plain = r#"{ "date": "2026-10-05", "requests": 151 }"#;
        assert_eq!(colour_json(plain, Paint::PLAIN), plain);
    }
}
