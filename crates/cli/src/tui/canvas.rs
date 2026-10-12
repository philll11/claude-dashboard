//! Drawing on the terminal's cells as the design's boards do: text at a column, boxes with a title
//! and a note, meters and sparklines. Positions may fall outside the screen; what does is left
//! out.

use ratatui::buffer::{Buffer, CellWidth};
use ratatui::style::Style;
use ratatui::text::Span;

use super::theme::Theme;

/// The characters of a sparkline, lowest first: eighths of a cell.
pub const SPARK: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

/// The characters of a sparkline where eighths of a cell do not show: a line, half a cell and a
/// whole one, each in place of the eighths nearest it.
pub const COARSE: [&str; 8] = ["_", "_", "▄", "▄", "▄", "▄", "█", "█"];

pub struct Canvas<'a> {
    buf: &'a mut Buffer,
    pub theme: Theme,
    pub w: i32,
    pub h: i32,
}

/// How many cells `text` takes on screen. Control characters take none, as they are never drawn.
pub fn width(text: &str) -> i32 {
    Span::raw(text)
        .styled_graphemes(Style::new())
        .map(|grapheme| i32::from(grapheme.symbol.cell_width()))
        .sum()
}

/// `text` cut to `cells`, ending in an ellipsis when cut.
pub fn fit(text: &str, cells: i32) -> String {
    if width(text) <= cells {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for grapheme in Span::raw(text).styled_graphemes(Style::new()) {
        let cells_of = i32::from(grapheme.symbol.cell_width());
        if used + cells_of > cells - 1 {
            break;
        }
        used += cells_of;
        out.push_str(grapheme.symbol);
    }
    if cells > 0 {
        out.push('…');
    }
    out
}

/// Text from a transcript or a path, with line breaks, tabs and other control characters as
/// spaces, so that it stays on its line.
pub fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

impl<'a> Canvas<'a> {
    pub fn new(buf: &'a mut Buffer, theme: Theme) -> Self {
        let area = buf.area;
        Self {
            buf,
            theme,
            w: i32::from(area.width),
            h: i32::from(area.height),
        }
    }

    fn inside(&self, x: i32, y: i32) -> bool {
        (0..self.w).contains(&x) && (0..self.h).contains(&y)
    }

    fn cell(&mut self, x: i32, y: i32) -> Option<&mut ratatui::buffer::Cell> {
        if !self.inside(x, y) {
            return None;
        }
        let area = self.buf.area;
        let x = u16::try_from(x).ok()? + area.x;
        let y = u16::try_from(y).ok()? + area.y;
        self.buf.cell_mut((x, y))
    }

    /// Puts one character, replacing the cell's style.
    pub fn put(&mut self, x: i32, y: i32, symbol: &str, style: Style) {
        if let Some(cell) = self.cell(x, y) {
            cell.reset();
            cell.set_symbol(symbol).set_style(style);
        }
    }

    /// Writes `text` from column `x`, and returns the column after it.
    pub fn text(&mut self, x: i32, y: i32, text: &str, style: Style) -> i32 {
        let mut x = x;
        for grapheme in Span::raw(text).styled_graphemes(Style::new()) {
            let cells = i32::from(grapheme.symbol.cell_width());
            if cells == 0 {
                continue;
            }
            if self.inside(x, y) && x + cells <= self.w {
                self.put(x, y, grapheme.symbol, style);
                // A wide character hides the cells it covers.
                for hidden in 1..cells {
                    if let Some(cell) = self.cell(x + hidden, y) {
                        cell.reset();
                        cell.set_symbol("");
                    }
                }
            }
            x += cells;
        }
        x
    }

    /// Writes `text` so that it ends just before column `end`, and returns where it starts.
    pub fn right(&mut self, end: i32, y: i32, text: &str, style: Style) -> i32 {
        let start = end - width(text);
        self.text(start, y, text, style);
        start
    }

    /// Adds `style` to `cells` cells from `x`, such as the selected row's background.
    pub fn paint(&mut self, x: i32, y: i32, cells: i32, style: Style) {
        for at in x..x + cells {
            if let Some(cell) = self.cell(at, y) {
                cell.set_style(style);
            }
        }
    }

    /// Clears a rectangle.
    pub fn clear(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let text = self.theme.text();
        for row in y..y + h {
            for column in x..x + w {
                self.put(column, row, " ", text);
            }
        }
    }

    /// Dims everything drawn so far, for an overlay above it; without colour nothing changes.
    pub fn dim(&mut self) {
        if self.theme.mono() {
            return;
        }
        let line = self.theme.line();
        for y in 0..self.h {
            for x in 0..self.w {
                if let Some(cell) = self.cell(x, y) {
                    let symbol = cell.symbol().to_owned();
                    cell.reset();
                    cell.set_symbol(&symbol).set_style(line);
                }
            }
        }
    }

    /// A box with its title at the top left and a note at the top right.
    pub fn boxed(
        &mut self,
        (x, y, w, h): (i32, i32, i32, i32),
        title: Option<&str>,
        note: Option<(&str, Style)>,
    ) {
        let line = self.theme.line();
        self.put(x, y, "┌", line);
        self.put(x + w - 1, y, "┐", line);
        self.put(x, y + h - 1, "└", line);
        self.put(x + w - 1, y + h - 1, "┘", line);
        for column in x + 1..x + w - 1 {
            self.put(column, y, "─", line);
            self.put(column, y + h - 1, "─", line);
        }
        for row in y + 1..y + h - 1 {
            self.put(x, row, "│", line);
            self.put(x + w - 1, row, "│", line);
        }
        let mut title_end = x + 2;
        if let Some(title) = title {
            let title = fit(title, w - 6);
            title_end = self.text(x + 2, y, &format!(" {title} "), self.theme.strong());
        }
        if let Some((note, style)) = note {
            let room = x + w - 3 - title_end - 1;
            if room > 2 {
                self.right(x + w - 3, y, &format!(" {} ", fit(note, room - 2)), style);
            }
        }
    }

    /// A meter of `cells` cells, filled to `share` of its length.
    pub fn meter(&mut self, x: i32, y: i32, cells: i32, share: f64, style: Style) {
        let filled = round(f64::from(cells) * share.clamp(0.0, 1.0));
        let track = self.theme.track();
        for at in 0..cells {
            if at < filled {
                self.put(x + at, y, "█", style);
            } else {
                self.put(x + at, y, "░", track);
            }
        }
    }

    /// Copies `rows` rows of `page`, from its row `from`, to this canvas from its row `to`.
    pub fn copy_rows(&mut self, page: &Buffer, from: i32, to: i32, rows: i32) {
        for row in 0..rows {
            let (Ok(source), Some(target)) =
                (u16::try_from(from + row), u16::try_from(to + row).ok())
            else {
                continue;
            };
            for x in 0..self.w {
                let Ok(column) = u16::try_from(x) else {
                    continue;
                };
                let Some(cell) = page.cell((column, source)).cloned() else {
                    continue;
                };
                if let Some(target) = self.cell(x, i32::from(target)) {
                    *target = cell;
                }
            }
        }
    }

    /// Marks a row as selected from column `x0` to `x1`, both included.
    pub fn select(&mut self, y: i32, x0: i32, x1: i32) {
        let row = self.theme.row();
        self.paint(x0, y, x1 - x0 + 1, row);
        self.put(x0 + 1, y, "›", self.theme.row_mark());
    }
}

/// Rounds half away from zero, as the design's boards do for positive values.
#[allow(clippy::cast_possible_truncation)]
pub fn round(value: f64) -> i32 {
    value.round() as i32
}

/// A sparkline of `values` in the characters of `ramp`, scaled to their largest.
pub fn spark(values: &[f64], ramp: &[&str; 8]) -> String {
    let largest = values.iter().copied().fold(0.000_1, f64::max);
    values
        .iter()
        .map(|value| ramp[usize::try_from(round(value / largest * 7.0).clamp(0, 7)).unwrap_or(0)])
        .collect()
}

/// A sparkline of `values` in the characters of `ramp`, on a scale from 0 to `top`, such as a
/// share of a limit.
pub fn spark_to(values: &[f64], top: f64, ramp: &[&str; 8]) -> String {
    values
        .iter()
        .map(|value| {
            let index = round(value / top * 8.0) - 1;
            ramp[usize::try_from(index.clamp(0, 7)).unwrap_or(0)]
        })
        .collect()
}

/// An axis from 0 to its top in `intervals` equal steps of 1, 2, 2.5 or 5 times a power of ten,
/// the smallest that reaches `max`, as the web dashboard's charts choose it.
pub fn nice_scale(max: f64, intervals: u32) -> (f64, f64) {
    let target = if max > 0.0 && max.is_finite() {
        max / f64::from(intervals)
    } else {
        1.0
    };
    let power = 10f64.powf(target.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .iter()
        .map(|factor| factor * power)
        .find(|candidate| *candidate >= target)
        .unwrap_or(10.0 * power);
    (step * f64::from(intervals), step)
}

/// An axis label in US dollars: whole dollars where every step is one, cents otherwise.
pub fn money_tick(value: f64, step: f64) -> String {
    use claude_dashboard_server::fmt::{grouped, money};
    if value == 0.0 {
        "$0".to_owned()
    } else if step.fract() == 0.0 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let whole = value.round() as u64;
        format!("${}", grouped(whole))
    } else {
        money(value)
    }
}
