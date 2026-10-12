//! The terminal palettes from the design tokens, which of them suits the terminal, and writing
//! styled text.

use std::fmt::Write as _;
use std::sync::OnceLock;

use anstyle::{Ansi256Color, Color, Effects, RgbColor, Style};

/// The environment variable that says whether the terminal's background is `light` or `dark`,
/// instead of asking the terminal.
pub const THEME_ENV: &str = "CLAUDE_DASHBOARD_THEME";

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

/// A terminal palette in `design/tokens.json`; a test keeps the two in step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// The background the palette is made for, which the terminal itself shows.
    pub background: Rgb,
    pub text: Rgb,
    pub strong: Rgb,
    pub muted: Rgb,
    /// Borders and other lines.
    pub line: Rgb,
    /// The empty part of a meter.
    pub track: Rgb,
    /// The background of the selected row.
    pub row: Rgb,
    pub accent: Rgb,
    pub good: Rgb,
    pub warning: Rgb,
    /// Passes WCAG AA for marks, not for text, on the dark background.
    pub critical: Rgb,
    pub removed: Rgb,
    pub added: Rgb,
    pub string: Rgb,
    pub fable: Rgb,
    pub opus: Rgb,
    pub sonnet: Rgb,
    pub haiku: Rgb,
}

impl Palette {
    /// The design's terminal palette, for a dark background.
    pub const DARK: Self = Self {
        background: Rgb(0x0b, 0x0d, 0x10),
        text: Rgb(0xd7, 0xda, 0xe0),
        strong: Rgb(0xf0, 0xf2, 0xf5),
        muted: Rgb(0x8a, 0x91, 0x9c),
        line: Rgb(0x3a, 0x40, 0x4a),
        track: Rgb(0x2a, 0x2f, 0x37),
        row: Rgb(0x1c, 0x21, 0x28),
        accent: Rgb(0x5c, 0xc8, 0xe8),
        good: Rgb(0x3f, 0xbf, 0x6b),
        warning: Rgb(0xfa, 0xb2, 0x19),
        critical: Rgb(0xd0, 0x3b, 0x3b),
        removed: Rgb(0xff, 0x8f, 0x8f),
        added: Rgb(0x7f, 0xd9, 0x9a),
        string: Rgb(0xb5, 0xd9, 0x8a),
        fable: Rgb(0x39, 0x87, 0xe5),
        opus: Rgb(0xd9, 0x59, 0x26),
        sonnet: Rgb(0x19, 0x9e, 0x70),
        haiku: Rgb(0xc9, 0x85, 0x00),
    };

    /// The design's terminal palette for a light background.
    pub const LIGHT: Self = Self {
        background: Rgb(0xff, 0xff, 0xff),
        text: Rgb(0x45, 0x4d, 0x5a),
        strong: Rgb(0x0e, 0x11, 0x16),
        muted: Rgb(0x5f, 0x67, 0x73),
        line: Rgb(0xb0, 0xb7, 0xc1),
        track: Rgb(0xd5, 0xda, 0xe1),
        row: Rgb(0xe6, 0xea, 0xef),
        accent: Rgb(0x08, 0x6a, 0x86),
        good: Rgb(0x16, 0x75, 0x3a),
        warning: Rgb(0x8c, 0x5f, 0x00),
        critical: Rgb(0xc6, 0x28, 0x28),
        removed: Rgb(0xb4, 0x23, 0x18),
        added: Rgb(0x16, 0x75, 0x3a),
        string: Rgb(0x3f, 0x6b, 0x1d),
        fable: Rgb(0x23, 0x68, 0xc4),
        opus: Rgb(0xc9, 0x4f, 0x1f),
        sonnet: Rgb(0x13, 0x90, 0x78),
        haiku: Rgb(0xa6, 0x6f, 0x00),
    };

    /// The palette that suits the terminal, as [`light_background`] tells.
    pub fn for_terminal() -> &'static Self {
        if light_background() {
            &Self::LIGHT
        } else {
            &Self::DARK
        }
    }

    /// The colour of a model's series, by its family.
    pub fn series(&self, model: &str) -> Rgb {
        if model.contains("opus") {
            self.opus
        } else if model.contains("sonnet") {
            self.sonnet
        } else if model.contains("haiku") {
            self.haiku
        } else if model.contains("fable") || model.contains("mythos") {
            self.fable
        } else {
            self.muted
        }
    }
}

/// Whether the terminal has a light background: as `CLAUDE_DASHBOARD_THEME` says, or else as the
/// terminal answers when asked. A terminal that does not answer counts as dark. The terminal is
/// asked once at most, and only by a command whose output is that terminal.
pub fn light_background() -> bool {
    static LIGHT: OnceLock<bool> = OnceLock::new();
    *LIGHT.get_or_init(|| match chosen_theme() {
        Some(light) => light,
        None => matches!(
            terminal_colorsaurus::theme_mode(terminal_colorsaurus::QueryOptions::default()),
            Ok(terminal_colorsaurus::ThemeMode::Light)
        ),
    })
}

/// The background `CLAUDE_DASHBOARD_THEME` names: `Some(true)` for light, `Some(false)` for dark.
pub fn chosen_theme() -> Option<bool> {
    theme_named(&std::env::var(THEME_ENV).ok()?)
}

/// Whether a theme's name, `light` or `dark` in any case, is the light one.
fn theme_named(name: &str) -> Option<bool> {
    match name.trim().to_ascii_lowercase().as_str() {
        "light" => Some(true),
        "dark" => Some(false),
        _ => None,
    }
}

/// How text is written: in colour or not, with 24-bit colour or the nearest of 256, and from which
/// palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paint {
    colour: bool,
    truecolor: bool,
    palette: &'static Palette,
}

impl Paint {
    /// Plain text, without any escape sequences.
    pub const PLAIN: Self = Self {
        colour: false,
        truecolor: false,
        palette: &Palette::DARK,
    };

    /// Colour unless `NO_COLOR` is set, as for the status line, whose output Claude Code shows
    /// in its own terminal. Claude Code runs the status line without a terminal to ask, so it
    /// takes the dark palette.
    pub fn for_claude_code() -> Self {
        Self {
            colour: !anstyle_query::no_color(),
            truecolor: supports_truecolor(),
            palette: &Palette::DARK,
        }
    }

    /// Colour when standard output is a terminal that shows it, in the palette that suits that
    /// terminal.
    pub fn for_stdout() -> Self {
        let colour = anstream::stdout().current_choice() != anstream::ColorChoice::Never;
        Self {
            colour,
            truecolor: supports_truecolor(),
            palette: if colour {
                Palette::for_terminal()
            } else {
                &Palette::DARK
            },
        }
    }

    /// The palette colours come from.
    pub fn palette(self) -> &'static Palette {
        self.palette
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
        self.paint(text, self.palette.muted, false)
    }

    pub fn strong(self, text: &str) -> String {
        self.paint(text, self.palette.strong, true)
    }

    fn colour_of(self, Rgb(r, g, b): Rgb) -> Color {
        if self.truecolor {
            Color::Rgb(RgbColor(r, g, b))
        } else {
            Color::Ansi256(Ansi256Color(nearest_256(r, g, b)))
        }
    }
}

/// Whether the terminal shows 24-bit colour: every console on Windows 10 from version 1703 does,
/// and elsewhere a terminal says so through `COLORTERM` or is one known to, such as Windows
/// Terminal under WSL or VS Code's terminal.
pub fn supports_truecolor() -> bool {
    cfg!(windows)
        || anstyle_query::truecolor()
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
pub fn nearest_256(r: u8, g: u8, b: u8) -> u8 {
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
        assert_eq!(
            Paint::PLAIN.paint("42%", Palette::DARK.warning, true),
            "42%"
        );
    }

    #[test]
    fn colours_are_24_bit_or_the_nearest_of_256() {
        let truecolor = Paint {
            colour: true,
            truecolor: true,
            palette: &Palette::DARK,
        };
        assert_eq!(
            truecolor.paint("x", Palette::DARK.warning, true),
            "\u{1b}[1m\u{1b}[38;2;250;178;25mx\u{1b}[0m"
        );
        let ansi256 = Paint {
            colour: true,
            truecolor: false,
            palette: &Palette::DARK,
        };
        assert_eq!(ansi256.muted("x"), "\u{1b}[38;5;246mx\u{1b}[0m");
        let light = Paint {
            colour: true,
            truecolor: true,
            palette: &Palette::LIGHT,
        };
        assert_eq!(light.muted("x"), "\u{1b}[38;2;95;103;115mx\u{1b}[0m");
    }

    #[test]
    fn a_theme_is_named_light_or_dark() {
        assert_eq!(theme_named("light"), Some(true));
        assert_eq!(theme_named(" Dark "), Some(false));
        assert_eq!(theme_named("sepia"), None);
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

    /// Each palette, by its section in the design tokens.
    const PALETTES: [(&str, Palette); 2] = [
        ("terminal", Palette::DARK),
        ("terminal-light", Palette::LIGHT),
    ];

    /// The light backgrounds the light palette is checked on: white, GNOME Terminal's and
    /// Solarized Light's.
    const LIGHT_BACKGROUNDS: [&str; 3] = ["#ffffff", "#eeeeec", "#fdf6e3"];

    #[test]
    fn the_palettes_are_the_terminal_palettes_of_the_design_tokens() {
        let tokens = tokens();
        for (section, palette) in PALETTES {
            for (name, colour) in [
                ("background", palette.background),
                ("text", palette.text),
                ("strong", palette.strong),
                ("muted", palette.muted),
                ("line", palette.line),
                ("track", palette.track),
                ("row", palette.row),
                ("accent", palette.accent),
                ("good", palette.good),
                ("warning", palette.warning),
                ("critical", palette.critical),
                ("removed", palette.removed),
                ("added", palette.added),
                ("string", palette.string),
            ] {
                assert_eq!(
                    colour.hex(),
                    token(&tokens, &format!("/{section}/{name}")),
                    "{section} {name}"
                );
            }
            let series = token(&tokens, &format!("/{section}/series"));
            for (name, colour) in [
                ("fable", palette.fable),
                ("opus", palette.opus),
                ("sonnet", palette.sonnet),
                ("haiku", palette.haiku),
            ] {
                assert_eq!(
                    colour.hex(),
                    token(&tokens, &format!("/series/{series}/{name}")),
                    "{section} {name}"
                );
            }
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

    /// The backgrounds each palette's colours appear on: the terminal's own, and the selected
    /// row.
    fn backgrounds(palette: &Palette) -> Vec<String> {
        let mut backgrounds = if *palette == Palette::LIGHT {
            LIGHT_BACKGROUNDS.map(str::to_owned).to_vec()
        } else {
            vec![palette.background.hex()]
        };
        backgrounds.push(palette.row.hex());
        backgrounds
    }

    #[test]
    fn every_text_colour_meets_wcag_aa_on_its_background() {
        for (section, palette) in PALETTES {
            for (name, colour) in [
                ("text", palette.text),
                ("strong", palette.strong),
                ("muted", palette.muted),
                ("accent", palette.accent),
                ("good", palette.good),
                ("warning", palette.warning),
                ("removed", palette.removed),
                ("added", palette.added),
                ("string", palette.string),
            ] {
                for background in backgrounds(&palette) {
                    let ratio = contrast(&colour.hex(), &background);
                    assert!(ratio >= 4.5, "{section} {name} on {background}: {ratio:.2}");
                }
            }
            // The selected tab is written in the background's colour on the accent.
            let ratio = contrast(&palette.background.hex(), &palette.accent.hex());
            assert!(ratio >= 4.5, "{section} selected tab: {ratio:.2}");
        }
        let tokens = tokens();
        let mut pairs = Vec::new();
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
    fn marks_meet_wcag_aa_for_graphics_on_their_background() {
        for (section, palette) in PALETTES {
            for (name, colour) in [
                ("critical", palette.critical),
                ("fable", palette.fable),
                ("opus", palette.opus),
                ("sonnet", palette.sonnet),
                ("haiku", palette.haiku),
            ] {
                for background in backgrounds(&palette) {
                    let ratio = contrast(&colour.hex(), &background);
                    assert!(ratio >= 3.0, "{section} {name} on {background}: {ratio:.2}");
                }
            }
        }
    }

    #[test]
    fn the_dark_critical_colour_is_for_marks_not_text() {
        let ratio = contrast(
            &Palette::DARK.critical.hex(),
            &Palette::DARK.background.hex(),
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
