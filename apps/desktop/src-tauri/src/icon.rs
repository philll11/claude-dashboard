//! The item's icon. On Windows and Linux it shows the five-hour window's use as a number on a
//! rounded square in the level's colour, `!` from 100%, or the app's gauge without a reading; on
//! macOS it is the gauge as a template image, black on clear, which the menu bar tints. Everything
//! is drawn on the design's grid and scaled, digits from a 5 by 7 bitmap font with crisp edges and
//! shapes with smoothed ones, so the icon needs no font or image library.

use claude_dashboard_server::protocol::Level;

/// An image: rows of red, green, blue and alpha bytes, top first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub size: u32,
    pub pixels: Vec<u8>,
}

/// What the icon shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Face {
    /// The five-hour window's use, in percent, at its level.
    Number { used: f64, level: Level },
    /// The app's gauge, while there is no current reading.
    Gauge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Colour(u8, u8, u8);

/// The design's colours: the dark desktop theme's page, line, meter and ink, and the status colours.
const PAGE: Colour = Colour(0x0d, 0x0f, 0x12);
const LINE: Colour = Colour(0x3a, 0x40, 0x4a);
const METER: Colour = Colour(0x5c, 0xc8, 0xe8);
const INK: Colour = Colour(0xe9, 0xec, 0xef);
const WARNING: Colour = Colour(0xfa, 0xb2, 0x19);
const CRITICAL: Colour = Colour(0xd0, 0x3b, 0x3b);
const WHITE: Colour = Colour(0xff, 0xff, 0xff);
#[cfg_attr(
    not(any(target_os = "macos", test)),
    expect(dead_code, reason = "only the macOS menu bar shows the template image")
)]
const BLACK: Colour = Colour(0, 0, 0);

/// The icon's grid, in units, which its size in pixels scales.
const GRID: f64 = 16.0;

/// The template image's grid, which is that of the app's symbol.
#[cfg_attr(
    not(any(target_os = "macos", test)),
    expect(dead_code, reason = "only the macOS menu bar shows the template image")
)]
const TEMPLATE_GRID: f64 = 24.0;

/// Each side of a pixel is sampled this many times, to smooth the edges of shapes.
const SAMPLES: u32 = 4;

/// The digits and `!`, each 5 cells wide and 7 high, one stroke wide.
const GLYPHS: [(char, [&str; 7]); 11] = [
    (
        '0',
        [
            ".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.",
        ],
    ),
    (
        '1',
        [
            "..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###.",
        ],
    ),
    (
        '2',
        [
            ".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####",
        ],
    ),
    (
        '3',
        [
            "####.", "....#", "....#", ".###.", "....#", "....#", "####.",
        ],
    ),
    (
        '4',
        [
            "...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#.",
        ],
    ),
    (
        '5',
        [
            "#####", "#....", "####.", "....#", "....#", "#...#", ".###.",
        ],
    ),
    (
        '6',
        [
            ".###.", "#....", "#....", "####.", "#...#", "#...#", ".###.",
        ],
    ),
    (
        '7',
        [
            "#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#...",
        ],
    ),
    (
        '8',
        [
            ".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###.",
        ],
    ),
    (
        '9',
        [
            ".###.", "#...#", "#...#", ".####", "....#", "....#", ".###.",
        ],
    ),
    (
        '!',
        [
            "..#..", "..#..", "..#..", "..#..", "..#..", ".....", "..#..",
        ],
    ),
];

/// The text the icon shows for a share of the limit: the whole percentage, or `!` from 100%.
#[must_use]
pub fn shown(used: f64) -> String {
    let rounded = used.max(0.0).round();
    if rounded >= 100.0 {
        "!".to_owned()
    } else {
        format!("{rounded}")
    }
}

/// The icon at `size` pixels square, for Windows and Linux.
#[must_use]
pub fn draw(face: Face, size: u32) -> Rgba {
    let (background, edge) = match face {
        Face::Number {
            level: Level::Warning,
            ..
        } => (WARNING, WARNING),
        Face::Number {
            level: Level::Critical,
            ..
        } => (CRITICAL, CRITICAL),
        Face::Number { .. } | Face::Gauge => (PAGE, LINE),
    };
    let unit = f64::from(size) / GRID;
    let mut image = paint(size, unit, |x, y| {
        // A square with corners of 3.5 units, inside a line one unit wide.
        if inside_rounded(x, y, 1.0, 15.0, 3.0) {
            Some(background)
        } else if inside_rounded(x, y, 0.0, 16.0, 4.0) {
            Some(edge)
        } else {
            None
        }
    });
    match face {
        Face::Gauge => {
            // The app's symbol: an arc from the left to the right over the top, and a needle.
            let gauge = paint(size, unit, |x, y| {
                if near_segment(x, y, (8.0, 11.0), (10.4, 8.0), 0.8) {
                    Some(INK)
                } else if near_arc(x, y, (8.0, 11.0), 4.5, 0.8) {
                    Some(METER)
                } else {
                    None
                }
            });
            image.over(&gauge);
        }
        Face::Number { used, level } => {
            let ink = match level {
                Level::Normal => METER,
                Level::Warning => PAGE,
                Level::Critical => WHITE,
            };
            image.cells(&shown(used), unit, ink);
        }
    }
    image
}

/// The gauge as a template image at `size` pixels square, for the macOS menu bar.
#[cfg_attr(
    not(any(target_os = "macos", test)),
    expect(dead_code, reason = "only the macOS menu bar shows the template image")
)]
#[must_use]
pub fn template(size: u32) -> Rgba {
    let unit = f64::from(size) / TEMPLATE_GRID;
    paint(size, unit, |x, y| {
        (near_arc(x, y, (12.0, 16.0), 8.0, 1.125)
            || near_segment(x, y, (12.0, 16.0), (16.0, 11.0), 1.125))
        .then_some(BLACK)
    })
}

/// An image of `size` pixels whose every pixel is the average of `colour` at points spread over
/// it, in the grid's units; `None` is clear.
fn paint(size: u32, unit: f64, colour: impl Fn(f64, f64) -> Option<Colour>) -> Rgba {
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    let samples = f64::from(SAMPLES * SAMPLES);
    for row in 0..size {
        for column in 0..size {
            let (mut red, mut green, mut blue, mut covered) = (0.0, 0.0, 0.0, 0.0);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = (f64::from(column) + (f64::from(sx) + 0.5) / f64::from(SAMPLES)) / unit;
                    let y = (f64::from(row) + (f64::from(sy) + 0.5) / f64::from(SAMPLES)) / unit;
                    if let Some(Colour(r, g, b)) = colour(x, y) {
                        red += f64::from(r);
                        green += f64::from(g);
                        blue += f64::from(b);
                        covered += 1.0;
                    }
                }
            }
            if covered == 0.0 {
                pixels.extend([0, 0, 0, 0]);
            } else {
                pixels.extend([
                    byte(red / covered),
                    byte(green / covered),
                    byte(blue / covered),
                    byte(255.0 * covered / samples),
                ]);
            }
        }
    }
    Rgba { size, pixels }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is rounded and kept between 0 and 255 first"
)]
fn byte(value: f64) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

impl Rgba {
    /// Lays `top` over this image.
    fn over(&mut self, top: &Self) {
        let (below, _) = self.pixels.as_chunks_mut::<4>();
        let (above, _) = top.pixels.as_chunks::<4>();
        for (below, above) in below.iter_mut().zip(above) {
            let alpha = f64::from(above[3]) / 255.0;
            if alpha == 0.0 {
                continue;
            }
            let under = f64::from(below[3]) / 255.0;
            let out = alpha + under * (1.0 - alpha);
            for channel in 0..3 {
                let mixed = f64::from(above[channel]) * alpha
                    + f64::from(below[channel]) * under * (1.0 - alpha);
                below[channel] = byte(mixed / out);
            }
            below[3] = byte(out * 255.0);
        }
    }

    /// Draws `text` in the bitmap font, 4 units from the top and centred as the design centres
    /// it, with each cell's edges on whole pixels.
    fn cells(&mut self, text: &str, unit: f64, ink: Colour) {
        let glyphs: Vec<&[&str; 7]> = text
            .chars()
            .filter_map(|c| GLYPHS.iter().find(|(glyph, _)| *glyph == c))
            .map(|(_, rows)| rows)
            .collect();
        let left = if glyphs.len() == 1 { 5 } else { 3 };
        let edge = |cell: usize| pixel_edge(cell, unit, self.size);
        for (index, rows) in glyphs.iter().enumerate() {
            for (row, line) in rows.iter().enumerate() {
                for (column, mark) in line.chars().enumerate() {
                    if mark != '#' {
                        continue;
                    }
                    let x = left + index * 6 + column;
                    let y = 4 + row;
                    for py in edge(y)..edge(y + 1) {
                        for px in edge(x)..edge(x + 1) {
                            let at = ((py * self.size + px) * 4) as usize;
                            self.pixels[at..at + 4].copy_from_slice(&[ink.0, ink.1, ink.2, 255]);
                        }
                    }
                }
            }
        }
    }
}

/// The pixel a grid line falls on, at `unit` pixels a unit.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "grid lines are small whole numbers, and the result stays within the image"
)]
fn pixel_edge(cell: usize, unit: f64, size: u32) -> u32 {
    ((cell as f64 * unit).round() as u32).min(size)
}

/// Whether `(x, y)` is inside the square from `low` to `high` with corners of `radius`.
fn inside_rounded(x: f64, y: f64, low: f64, high: f64, radius: f64) -> bool {
    if x < low || x > high || y < low || y > high {
        return false;
    }
    let cx = x.clamp(low + radius, high - radius);
    let cy = y.clamp(low + radius, high - radius);
    (x - cx).hypot(y - cy) <= radius
}

/// Whether `(x, y)` is within `half` of the line from `from` to `to`, rounded at its ends.
fn near_segment(x: f64, y: f64, from: (f64, f64), to: (f64, f64), half: f64) -> bool {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let along = (((x - from.0) * dx + (y - from.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    (x - (from.0 + along * dx)).hypot(y - (from.1 + along * dy)) <= half
}

/// Whether `(x, y)` is within `half` of the upper half of the circle of `radius` around `centre`,
/// rounded at its ends.
fn near_arc(x: f64, y: f64, centre: (f64, f64), radius: f64, half: f64) -> bool {
    let (dx, dy) = (x - centre.0, y - centre.1);
    if dy <= 0.0 {
        (dx.hypot(dy) - radius).abs() <= half
    } else {
        (dx + radius).hypot(dy) <= half || (dx - radius).hypot(dy) <= half
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sizes the icon is drawn at: 16 pixels at 100% scaling, up to 48 at 300%.
    const SIZES: [u32; 6] = [16, 20, 24, 32, 40, 48];

    /// The icon as text, a character a pixel: clear, part covered, or the colour it is closest
    /// to.
    fn picture(image: &Rgba) -> String {
        let named = [
            (PAGE, 'p'),
            (LINE, 'l'),
            (METER, 'm'),
            (INK, 'i'),
            (WARNING, 'w'),
            (CRITICAL, 'c'),
            (WHITE, 'W'),
            (BLACK, 'b'),
        ];
        let mut out = String::new();
        for row in image.pixels.chunks_exact(image.size as usize * 4) {
            for pixel in row.as_chunks::<4>().0 {
                let mark = match pixel[3] {
                    0 => ' ',
                    255 => named
                        .iter()
                        .find(|(Colour(r, g, b), _)| [*r, *g, *b] == pixel[..3])
                        .map_or('+', |(_, mark)| *mark),
                    _ => '.',
                };
                out.push(mark);
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn the_icon_shows_the_whole_percentage_and_a_mark_from_100() {
        assert_eq!(shown(42.4), "42");
        assert_eq!(shown(7.0), "7");
        assert_eq!(shown(-1.0), "0");
        assert_eq!(shown(99.4), "99");
        assert_eq!(shown(99.5), "!");
        assert_eq!(shown(104.0), "!");
    }

    #[test]
    fn every_level_and_size_draws_as_its_snapshot() {
        let faces = [
            (
                "normal-42",
                Face::Number {
                    used: 42.0,
                    level: Level::Normal,
                },
            ),
            (
                "normal-7",
                Face::Number {
                    used: 7.0,
                    level: Level::Normal,
                },
            ),
            (
                "warning-71",
                Face::Number {
                    used: 71.0,
                    level: Level::Warning,
                },
            ),
            (
                "critical-93",
                Face::Number {
                    used: 93.0,
                    level: Level::Critical,
                },
            ),
            (
                "critical-104",
                Face::Number {
                    used: 104.0,
                    level: Level::Critical,
                },
            ),
            ("gauge", Face::Gauge),
        ];
        for (name, face) in faces {
            for size in SIZES {
                insta::assert_snapshot!(format!("{name}-{size}"), picture(&draw(face, size)));
            }
        }
        for size in [18, 36] {
            insta::assert_snapshot!(format!("template-{size}"), picture(&template(size)));
        }
    }

    #[test]
    fn digits_sit_on_whole_pixels_at_every_size() {
        for size in SIZES {
            let unit = f64::from(size) / GRID;
            let image = draw(
                Face::Number {
                    used: 88.0,
                    level: Level::Normal,
                },
                size,
            );
            // The cells the two digits take: columns 3 to 13 and rows 4 to 10.
            for y in pixel_edge(4, unit, size)..pixel_edge(11, unit, size) {
                for x in pixel_edge(3, unit, size)..pixel_edge(14, unit, size) {
                    let at = ((y * size + x) * 4) as usize;
                    let pixel = &image.pixels[at..at + 4];
                    assert!(
                        pixel == [PAGE.0, PAGE.1, PAGE.2, 255]
                            || pixel == [METER.0, METER.1, METER.2, 255],
                        "{size}: {pixel:?} at {x}, {y}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_colours_are_the_design_tokens() {
        let tokens: serde_json::Value =
            serde_json::from_str(include_str!("../../../../design/tokens.json")).unwrap();
        let hex = |Colour(r, g, b): Colour| format!("#{r:02x}{g:02x}{b:02x}");
        let token = |pointer: &str| {
            tokens
                .pointer(pointer)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        };
        assert_eq!(hex(PAGE), token("/themes/desktop-dark/page"));
        assert_eq!(hex(INK), token("/themes/desktop-dark/ink"));
        assert_eq!(hex(METER), token("/themes/desktop-dark/meter"));
        assert_eq!(hex(LINE), token("/terminal/line"));
        assert_eq!(hex(WARNING), token("/status/dark/warning"));
        assert_eq!(hex(CRITICAL), token("/status/dark/critical"));
    }
}
