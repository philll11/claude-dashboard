//! The interface's styles: the terminal palette that suits the terminal, in 24-bit colour or the
//! nearest of 256, or no colour at all under `NO_COLOR`.

use ratatui::style::{Color, Modifier, Style};

use super::canvas::{COARSE, SPARK};
use crate::style::{Palette, Rgb, nearest_256, supports_truecolor};

/// A model family's series, in the design's fixed order, in which charts stack and list them,
/// and every other model together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Series {
    Fable,
    Opus,
    Sonnet,
    Haiku,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    palette: &'static Palette,
    /// Without colour, as `NO_COLOR` asks: bold and reverse video only, and the models told apart
    /// by pattern.
    mono: bool,
    truecolor: bool,
    /// Sparklines without eighths of a cell, which the terminal may not show.
    coarse: bool,
}

impl Theme {
    /// The theme for this terminal: its light or dark palette, and `NO_COLOR`.
    pub fn for_terminal() -> Self {
        let mono = anstyle_query::no_color();
        Self {
            palette: if mono {
                &Palette::DARK
            } else {
                Palette::for_terminal()
            },
            mono,
            truecolor: supports_truecolor(),
            coarse: coarse_blocks(),
        }
    }

    #[cfg(test)]
    pub fn new(palette: &'static Palette, mono: bool, truecolor: bool) -> Self {
        Self {
            palette,
            mono,
            truecolor,
            coarse: false,
        }
    }

    /// The theme with sparklines that need no eighths of a cell.
    #[cfg(test)]
    pub fn coarse(self) -> Self {
        Self {
            coarse: true,
            ..self
        }
    }

    pub fn mono(self) -> bool {
        self.mono
    }

    fn colour(self, Rgb(r, g, b): Rgb) -> Color {
        if self.mono {
            Color::Reset
        } else if self.truecolor {
            Color::Rgb(r, g, b)
        } else {
            Color::Indexed(nearest_256(r, g, b))
        }
    }

    fn fg(self, colour: Rgb) -> Style {
        Style::new().fg(self.colour(colour))
    }

    pub fn text(self) -> Style {
        self.fg(self.palette.text)
    }

    pub fn strong(self) -> Style {
        self.fg(self.palette.strong).add_modifier(Modifier::BOLD)
    }

    pub fn muted(self) -> Style {
        self.fg(self.palette.muted)
    }

    pub fn line(self) -> Style {
        self.fg(self.palette.line)
    }

    pub fn accent(self) -> Style {
        self.fg(self.palette.accent)
    }

    /// A key in the key line and the help.
    pub fn key(self) -> Style {
        self.accent().add_modifier(Modifier::BOLD)
    }

    /// The warning mark and figures.
    pub fn warn(self) -> Style {
        self.fg(self.palette.warning).add_modifier(Modifier::BOLD)
    }

    pub fn warn_text(self) -> Style {
        self.fg(self.palette.warning)
    }

    /// The critical mark; the colour passes WCAG AA for marks, not text.
    pub fn critical(self) -> Style {
        self.fg(self.palette.critical).add_modifier(Modifier::BOLD)
    }

    pub fn good(self) -> Style {
        self.fg(self.palette.good)
    }

    /// The empty part of a meter.
    pub fn track(self) -> Style {
        self.fg(self.palette.track)
    }

    /// The selected section's tab.
    pub fn selected(self) -> Style {
        if self.mono {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::new()
                .fg(self.colour(self.palette.background))
                .bg(self.colour(self.palette.accent))
                .add_modifier(Modifier::BOLD)
        }
    }

    /// What the selected row adds to its cells.
    pub fn row(self) -> Style {
        if self.mono {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new().bg(self.colour(self.palette.row))
        }
    }

    /// The mark at the start of the selected row.
    pub fn row_mark(self) -> Style {
        if self.mono {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            self.key().bg(self.colour(self.palette.row))
        }
    }

    pub fn series(self, series: Series) -> Style {
        let palette = self.palette;
        self.fg(match series {
            Series::Fable => palette.fable,
            Series::Opus => palette.opus,
            Series::Sonnet => palette.sonnet,
            Series::Haiku => palette.haiku,
            Series::Other => palette.muted,
        })
    }

    /// The character that fills a series' part of a chart: the same block in colour, a pattern
    /// of its own without.
    pub fn glyph(self, series: Series) -> &'static str {
        if self.mono {
            match series {
                Series::Fable => "█",
                Series::Opus => "▓",
                Series::Sonnet => "▒",
                Series::Haiku => "░",
                Series::Other => "▪",
            }
        } else {
            "█"
        }
    }

    /// A series' mark beside its name.
    pub fn swatch(self, series: Series) -> &'static str {
        if self.mono { self.glyph(series) } else { "■" }
    }

    /// The characters of sparklines and of the tops of area charts, lowest first.
    pub fn ramp(self) -> &'static [&'static str; 8] {
        if self.coarse { &COARSE } else { &SPARK }
    }
}

/// Whether the terminal may not show eighths of a cell. The Windows console host draws a run of
/// them in one colour as empty boxes, and the Linux console's fonts lack them. On Windows, a
/// terminal other than the console host names itself in the environment, as the
/// `supports-unicode` crate reads it.
fn coarse_blocks() -> bool {
    let named = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    if cfg!(windows) {
        !["WT_SESSION", "TERM_PROGRAM", "TERM", "TERMINAL_EMULATOR"]
            .into_iter()
            .any(named)
    } else {
        std::env::var_os("TERM").is_some_and(|term| term == "linux")
    }
}
