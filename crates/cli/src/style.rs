//! The terminal palette from the design tokens, and writing styled text.

use std::fmt::Write as _;

use anstyle::{Ansi256Color, Color, Effects, RgbColor, Style};

/// A colour, by its red, green and blue values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// The colour as the design tokens write it: `#f0f2f5`.
    #[cfg(test)]
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

/// The terminal palette in `design/tokens.json`; a test keeps the two in step.
pub mod palette {
    use super::Rgb;

    pub const STRONG: Rgb = Rgb(0xf0, 0xf2, 0xf5);
    pub const MUTED: Rgb = Rgb(0x8a, 0x91, 0x9c);
    pub const ACCENT: Rgb = Rgb(0x5c, 0xc8, 0xe8);
    pub const GOOD: Rgb = Rgb(0x3f, 0xbf, 0x6b);
    pub const WARNING: Rgb = Rgb(0xfa, 0xb2, 0x19);
    pub const CRITICAL: Rgb = Rgb(0xd0, 0x3b, 0x3b);
    pub const REMOVED: Rgb = Rgb(0xff, 0x8f, 0x8f);
    pub const ADDED: Rgb = Rgb(0x7f, 0xd9, 0x9a);
    pub const STRING: Rgb = Rgb(0xb5, 0xd9, 0x8a);
    pub const FABLE: Rgb = Rgb(0x39, 0x87, 0xe5);
    pub const OPUS: Rgb = Rgb(0xd9, 0x59, 0x26);
    pub const SONNET: Rgb = Rgb(0x19, 0x9e, 0x70);
    pub const HAIKU: Rgb = Rgb(0xc9, 0x85, 0x00);
}

/// How text is written: in colour or not, and with 24-bit colour or the nearest of 256.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paint {
    colour: bool,
    truecolor: bool,
}

impl Paint {
    /// Plain text, without any escape sequences.
    pub const PLAIN: Self = Self {
        colour: false,
        truecolor: false,
    };

    /// Colour unless `NO_COLOR` is set, as for the status line, whose output Claude Code shows
    /// in its own terminal.
    pub fn for_claude_code() -> Self {
        Self {
            colour: !anstyle_query::no_color(),
            truecolor: supports_truecolor(),
        }
    }

    /// Colour when standard output is a terminal that shows it.
    pub fn for_stdout() -> Self {
        let colour = anstream::stdout().current_choice() != anstream::ColorChoice::Never;
        Self {
            colour,
            truecolor: supports_truecolor(),
        }
    }

    /// `text` in `colour`, bold when `bold` is set.
    pub fn paint(self, text: &str, colour: Rgb, bold: bool) -> String {
        if !self.colour {
            return text.to_owned();
        }
        let style = Style::new()
            .fg_color(Some(self.colour_of(colour)))
            .effects(if bold { Effects::BOLD } else { Effects::new() });
        let mut out = String::new();
        let _ = write!(out, "{style}{text}{style:#}");
        out
    }

    pub fn muted(self, text: &str) -> String {
        self.paint(text, palette::MUTED, false)
    }

    pub fn strong(self, text: &str) -> String {
        self.paint(text, palette::STRONG, true)
    }

    fn colour_of(self, Rgb(r, g, b): Rgb) -> Color {
        if self.truecolor {
            Color::Rgb(RgbColor(r, g, b))
        } else {
            Color::Ansi256(Ansi256Color(nearest_256(r, g, b)))
        }
    }
}

/// Whether the terminal shows 24-bit colour: it says so through `COLORTERM`, or it is one known
/// to, such as Windows Terminal or VS Code's terminal.
fn supports_truecolor() -> bool {
    anstyle_query::truecolor()
        || std::env::var_os("WT_SESSION").is_some()
        || std::env::var("TERM_PROGRAM").is_ok_and(|program| {
            matches!(
                program.as_str(),
                "vscode" | "iTerm.app" | "WezTerm" | "ghostty"
            )
        })
}

/// The xterm 256-colour palette entry nearest to a colour: one of the 6×6×6 cube or the 24
/// greys.
fn nearest_256(r: u8, g: u8, b: u8) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let level = |value: u8| {
        LEVELS
            .iter()
            .enumerate()
            .min_by_key(|(_, level)| value.abs_diff(**level))
            .map_or(0, |(index, _)| index)
    };
    let distance = |(r2, g2, b2): (u8, u8, u8)| {
        [(r, r2), (g, g2), (b, b2)]
            .iter()
            .map(|(a, b)| u32::from(a.abs_diff(*b)).pow(2))
            .sum::<u32>()
    };
    let (ri, gi, bi) = (level(r), level(g), level(b));
    let cube = (LEVELS[ri], LEVELS[gi], LEVELS[bi]);
    let average = (u32::from(r) + u32::from(g) + u32::from(b)) / 3;
    let grey_index = u8::try_from((average.saturating_sub(8) + 5) / 10)
        .unwrap_or(23)
        .min(23);
    let grey_level = 8 + 10 * grey_index;
    let cube_index = 16 + 36 * ri + 6 * gi + bi;
    if distance((grey_level, grey_level, grey_level)) < distance(cube) {
        232 + grey_index
    } else {
        u8::try_from(cube_index).unwrap_or(16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_has_no_escape_sequences() {
        assert_eq!(Paint::PLAIN.paint("42%", palette::WARNING, true), "42%");
    }

    #[test]
    fn colours_are_24_bit_or_the_nearest_of_256() {
        let truecolor = Paint {
            colour: true,
            truecolor: true,
        };
        assert_eq!(
            truecolor.paint("x", palette::WARNING, true),
            "\u{1b}[1m\u{1b}[38;2;250;178;25mx\u{1b}[0m"
        );
        let ansi256 = Paint {
            colour: true,
            truecolor: false,
        };
        assert_eq!(
            ansi256.paint("x", palette::MUTED, false),
            "\u{1b}[38;5;246mx\u{1b}[0m"
        );
    }

    fn tokens() -> serde_json::Value {
        serde_json::from_str(include_str!("../../../design/tokens.json")).unwrap()
    }

    fn token(tokens: &serde_json::Value, pointer: &str) -> String {
        tokens
            .pointer(pointer)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("{pointer} is not in the design tokens"))
            .to_owned()
    }

    #[test]
    fn the_palette_is_the_terminal_palette_of_the_design_tokens() {
        let tokens = tokens();
        for (name, colour) in [
            ("strong", palette::STRONG),
            ("muted", palette::MUTED),
            ("accent", palette::ACCENT),
            ("good", palette::GOOD),
            ("warning", palette::WARNING),
            ("critical", palette::CRITICAL),
            ("removed", palette::REMOVED),
            ("added", palette::ADDED),
            ("string", palette::STRING),
        ] {
            assert_eq!(
                colour.hex(),
                token(&tokens, &format!("/terminal/{name}")),
                "{name}"
            );
        }
        let series = token(&tokens, "/terminal/series");
        for (name, colour) in [
            ("fable", palette::FABLE),
            ("opus", palette::OPUS),
            ("sonnet", palette::SONNET),
            ("haiku", palette::HAIKU),
        ] {
            assert_eq!(
                colour.hex(),
                token(&tokens, &format!("/series/{series}/{name}")),
                "{name}"
            );
        }
    }

    /// WCAG's relative luminance of a `#rrggbb` colour.
    fn luminance(hex: &str) -> f64 {
        let channel = |at: usize| {
            let value = f64::from(u8::from_str_radix(&hex[at..at + 2], 16).unwrap()) / 255.0;
            if value <= 0.040_45 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
    }

    fn contrast(a: &str, b: &str) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn every_text_colour_meets_wcag_aa_on_its_background() {
        let tokens = tokens();
        let mut pairs = Vec::new();
        for text in [
            "text", "strong", "muted", "accent", "good", "warning", "removed", "added", "string",
        ] {
            pairs.push((
                format!("/terminal/{text}"),
                "/terminal/background".to_owned(),
            ));
        }
        for theme in ["desktop-dark", "desktop-light"] {
            for text in ["ink", "ink-2", "muted"] {
                for background in ["page", "panel"] {
                    pairs.push((
                        format!("/themes/{theme}/{text}"),
                        format!("/themes/{theme}/{background}"),
                    ));
                }
            }
            pairs.push((
                format!("/themes/{theme}/selected-ink"),
                format!("/themes/{theme}/selected"),
            ));
        }
        for (text, background) in pairs {
            let ratio = contrast(&token(&tokens, &text), &token(&tokens, &background));
            assert!(ratio >= 4.5, "{text} on {background}: {ratio:.2}");
        }
    }

    #[test]
    fn the_critical_colour_is_for_marks_not_text() {
        let tokens = tokens();
        let ratio = contrast(
            &token(&tokens, "/terminal/critical"),
            &token(&tokens, "/terminal/background"),
        );
        assert!((3.0..4.5).contains(&ratio), "{ratio:.2}");
    }

    #[test]
    fn the_nearest_256_colour_prefers_greys_for_greys() {
        assert_eq!(nearest_256(0x8a, 0x91, 0x9c), 246);
        assert_eq!(nearest_256(0xff, 0x00, 0x00), 196);
        assert_eq!(nearest_256(0x00, 0x00, 0x00), 16);
    }
}
