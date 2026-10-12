//! The cost chart: API-equivalent cost per day, week or month, stacked by model series, with its
//! legend; and the same as a table.

use std::collections::HashMap;

use claude_dashboard_core::report::Bucket;
use claude_dashboard_server::fmt::{day_label, money, month_year};
use claude_dashboard_server::protocol::{Day, ModelRow};
use jiff::ToSpan;
use jiff::civil::{Date, Weekday};

use super::series::SeriesNames;
use crate::tui::canvas::{Canvas, money_tick, nice_scale, round, width};
use crate::tui::state::day;
use crate::tui::theme::Series;

/// One column of the chart: a day, week or month, with each series' cost.
#[derive(Debug, Clone)]
pub struct Column {
    /// Under the column: `10`, or `Oct`.
    pub label: String,
    /// In a table: `Sat 10 Oct`, `Week of Mon 5 Oct`, `October 2026`.
    pub title: String,
    pub values: Vec<(Series, f64)>,
}

impl Column {
    pub fn total(&self) -> f64 {
        self.values.iter().map(|(_, usd)| usd).sum()
    }

    pub fn value(&self, series: Series) -> f64 {
        self.values
            .iter()
            .filter(|(of, _)| *of == series)
            .map(|(_, usd)| usd)
            .sum()
    }
}

/// The cost chart's data: its columns, the series with a cost in it, what they are called, and
/// how finely the days are added up.
pub struct Chart {
    pub columns: Vec<Column>,
    /// Bottom to top, with their total cost.
    pub series: Vec<(Series, f64)>,
    pub names: SeriesNames,
    pub bucket: Bucket,
}

impl Chart {
    /// The days from `first` to `last`, as finely as `room` columns show them: by day as the
    /// web dashboard would, unless the days do not fit, and then by week or by month.
    pub fn new(
        (first, last): (&str, &str),
        by_day: &[Day],
        models: &[ModelRow],
        series_of: &HashMap<String, Series>,
        week_start: Weekday,
        room: i32,
    ) -> Self {
        let (first, last) = (day(first), day(last));
        let days = (last - first).get_days() + 1;
        let mut bucket = Bucket::of(i64::from(days));
        let starts = |bucket: Bucket| {
            let mut starts = Vec::new();
            let mut at = bucket.start(first, week_start);
            while at <= last {
                starts.push(at);
                at = bucket.end(at).checked_add(1.day()).unwrap_or(Date::MAX);
                if at == Date::MAX {
                    break;
                }
            }
            starts
        };
        let mut columns = starts(bucket);
        while i32::try_from(columns.len()).unwrap_or(i32::MAX) > room && bucket != Bucket::Month {
            bucket = if bucket == Bucket::Day {
                Bucket::Week
            } else {
                Bucket::Month
            };
            columns = starts(bucket);
        }
        let priced: Vec<ModelRow> = models
            .iter()
            .filter(|row| row.totals.cost.total > 0.0)
            .cloned()
            .collect();
        let series_of_model = |model: &str| series_of.get(model).copied().unwrap_or(Series::Other);
        let mut totals: HashMap<Series, f64> = HashMap::new();
        for row in &priced {
            *totals.entry(series_of_model(&row.model.id)).or_default() += row.totals.cost.total;
        }
        let mut series: Vec<(Series, f64)> = totals.into_iter().collect();
        series.sort_by_key(|(series, _)| *series);
        let mut values: HashMap<Date, HashMap<Series, f64>> = HashMap::new();
        for day_totals in by_day {
            let start = bucket.start(day(&day_totals.date), week_start);
            let column = values.entry(start).or_default();
            for part in &day_totals.models {
                if part.totals.cost.total > 0.0 {
                    *column.entry(series_of_model(&part.model)).or_default() +=
                        part.totals.cost.total;
                }
            }
        }
        let columns = columns
            .into_iter()
            .map(|start| {
                let mut column_values: Vec<(Series, f64)> = values
                    .remove(&start)
                    .unwrap_or_default()
                    .into_iter()
                    .collect();
                column_values.sort_by_key(|(series, _)| *series);
                Column {
                    label: match bucket {
                        Bucket::Day | Bucket::Week => start.day().to_string(),
                        Bucket::Month => month_year(start).chars().take(3).collect(),
                    },
                    title: match bucket {
                        Bucket::Day => day_label(start),
                        Bucket::Week => format!("Week of {}", day_label(start)),
                        Bucket::Month => month_year(start),
                    },
                    values: column_values,
                }
            })
            .collect();
        Self {
            columns,
            series,
            names: SeriesNames::of(&priced, series_of),
            bucket,
        }
    }

    /// `day`, `week` or `month`.
    pub fn per(&self) -> &'static str {
        match self.bucket {
            Bucket::Day => "day",
            Bucket::Week => "week",
            Bucket::Month => "month",
        }
    }
}

/// The chart in a box: columns stacked by series from the bottom, a money scale at the left,
/// labels under every fifth column and the last, and the legend in the box's last row.
pub fn draw(c: &mut Canvas, (x0, y0, w, h): (i32, i32, i32, i32), chart: &Chart, title: &str) {
    let theme = c.theme;
    c.boxed((x0, y0, w, h), Some(title), None);
    let columns = &chart.columns;
    let count = i32::try_from(columns.len()).unwrap_or(1).max(1);
    let plot_rows = h - 4;
    let highest = columns.iter().map(Column::total).fold(0.0, f64::max);
    let (top, half) = nice_scale(highest, 2);
    let (top_label, half_label) = (money_tick(top, half), money_tick(half, half));
    let plot_x = x0 + scale_room(&[&top_label, &half_label]);
    let step = ((x0 + w - 3 - plot_x) / count).max(1);
    let per_row = top / f64::from(plot_rows);
    for r in 0..plot_rows {
        let y = y0 + 1 + (plot_rows - 1 - r);
        if r == plot_rows - 1 {
            c.right(plot_x - 1, y, &top_label, theme.muted());
        }
        if r == round(f64::from(plot_rows) / 2.0) - 1 {
            c.right(plot_x - 1, y, &half_label, theme.muted());
        }
        if r == 0 {
            c.right(plot_x - 1, y, "$0", theme.muted());
        }
        let middle = (f64::from(r) + 0.5) * per_row;
        for (index, column) in (0..).zip(columns) {
            let mut below = 0.0;
            for (series, usd) in &column.values {
                if middle >= below && middle < below + usd {
                    c.put(
                        plot_x + index * step,
                        y,
                        theme.glyph(*series),
                        theme.series(*series),
                    );
                    break;
                }
                below += usd;
            }
        }
    }
    // Every fifth column and the last are labelled; the last wins where two would touch.
    let mut labels: Vec<(i32, &str)> = Vec::new();
    for (index, column) in (0..).zip(columns) {
        if index % 5 != 0 && index != count - 1 {
            continue;
        }
        let x = plot_x + index * step;
        if let Some(&(last_x, last_label)) = labels.last()
            && x <= last_x + width(last_label)
        {
            if index != count - 1 {
                continue;
            }
            labels.pop();
        }
        labels.push((x, &column.label));
    }
    let axis = y0 + 1 + plot_rows;
    for (x, label) in labels {
        c.text(x, axis, label, theme.muted());
    }
    legend(c, x0 + 2, y0 + h - 2, chart, w >= 70);
}

/// Where a chart's plot starts, from its box's left: after its scale's widest label, with a space
/// either side, and never before the seventh column, as on the boards.
pub fn scale_room(labels: &[&str]) -> i32 {
    let widest = labels.iter().map(|label| width(label)).max().unwrap_or(0);
    7.max(widest + 3)
}

/// The series' marks and names, in a row from `x`: full names when `long`, otherwise the
/// family's alone.
pub fn legend(c: &mut Canvas, x: i32, y: i32, chart: &Chart, long: bool) {
    let theme = c.theme;
    let mut x = x;
    for (series, _) in &chart.series {
        c.put(x, y, theme.swatch(*series), theme.series(*series));
        let name = chart.names.get(*series);
        let name = if long {
            name.to_owned()
        } else {
            name.split(' ').next().unwrap_or(name).to_owned()
        };
        x = c.text(x + 2, y, &name, theme.text()) + 2;
    }
}

/// The chart as a table in a box: a row per column, newest first from `skip`, with each
/// series' cost and the total.
pub fn table(
    c: &mut Canvas,
    (x0, y0, width, height): (i32, i32, i32, i32),
    chart: &Chart,
    title: &str,
    skip: usize,
) {
    let theme = c.theme;
    let rows = usize::try_from(height - 3).unwrap_or(0);
    let columns: Vec<&Column> = chart.columns.iter().rev().collect();
    let skip = skip.min(columns.len().saturating_sub(rows));
    let shown = columns.len().min(rows);
    let note = (columns.len() > rows).then(|| {
        format!(
            "{}–{} of {} · ↑↓ scroll",
            skip + 1,
            skip + shown.min(columns.len() - skip),
            columns.len()
        )
    });
    c.boxed(
        (x0, y0, width, height),
        Some(title),
        note.as_deref().map(|note| (note, theme.muted())),
    );
    let heading = match chart.bucket {
        Bucket::Day => "Day",
        Bucket::Week => "Week",
        Bucket::Month => "Month",
    };
    let right = x0 + width - 2;
    let total_width = 11;
    let series_width = ((width - 4 - 22 - total_width)
        / i32::try_from(chart.series.len()).unwrap_or(1).max(1))
    .clamp(9, 14);
    let long = series_width >= 12;
    c.text(x0 + 2, y0 + 1, heading, theme.muted());
    let mut x = right - total_width;
    let mut ends = Vec::new();
    for (series, _) in chart.series.iter().rev() {
        let name = chart.names.get(*series);
        let name = if long {
            name.to_owned()
        } else {
            name.split(' ').next().unwrap_or(name).to_owned()
        };
        c.right(x, y0 + 1, &name, theme.muted());
        ends.push((*series, x));
        x -= series_width;
    }
    c.right(right, y0 + 1, "Total", theme.muted());
    for (row, column) in (0..).zip(columns.iter().skip(skip).take(rows)) {
        let y = y0 + 2 + row;
        c.text(x0 + 2, y, &column.title, theme.text());
        for (series, end) in &ends {
            c.right(*end, y, &money(column.value(*series)), theme.text());
        }
        c.right(right, y, &money(column.total()), theme.strong());
    }
}
