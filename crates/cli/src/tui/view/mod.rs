//! Drawing the state, as the design's terminal boards draw each screen: a header with the
//! sections, the section, and a line of keys with when the history was last read.

mod chart;
mod limits;
mod models;
mod overlays;
mod overview;
pub mod projects;
mod series;
mod session;
mod sessions;

use claude_dashboard_server::fmt::{ago, grouped};
use jiff::Timestamp;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::canvas::{Canvas, fit, width};
use super::state::{Section, State};

pub use series::series_of;

/// The key line's keys on most screens.
pub const KEYS: [(&str, &str); 4] = [
    ("q", "quit"),
    ("←→", "sections"),
    ("t", "table view"),
    ("?", "help"),
];

/// Draws the whole screen.
pub fn draw(buf: &mut Buffer, state: &State) {
    let mut canvas = Canvas::new(buf, state.theme);
    let c = &mut canvas;
    if state.too_small() {
        too_small(c);
        return;
    }
    let Some(status) = state.status() else {
        match (state.status_error(), state.health()) {
            (_, Some(health)) if health.history.is_none() && health.newer_schema.is_some() => {
                too_new(c, health.newer_schema.unwrap_or_default());
            }
            (Some(error), _) => cannot_open(c, state, error),
            _ => {
                header(c, state, Some(state.section));
                footer(c, state, &KEYS, None);
            }
        }
        return;
    };
    let nothing_yet = status.first_day.is_none()
        && reading(state).is_none()
        && status.limits.is_empty()
        && status.read_ms.is_some();
    if nothing_yet {
        empty(c, state);
    } else {
        match state.section {
            Section::Overview => overview::draw(c, state),
            Section::Sessions if state.session.is_some() => session::draw(c, state),
            Section::Sessions => sessions::draw(c, state),
            Section::Projects => projects::draw(c, state),
            Section::Models => models::draw(c, state),
            Section::Limits => limits::draw(c, state),
        }
    }
    if let Some(overlay) = &state.overlay {
        overlays::draw(c, state, overlay);
    }
}

/// How many rows the section's page has beyond the screen, for scrolling it.
pub fn page_overflow(state: &State) -> i32 {
    let page = match state.section {
        Section::Sessions if state.session.is_some() => session::height(state),
        Section::Models => models::height(state),
        _ => 0,
    };
    page - page_rows(state)
}

/// The rows a page shows at once: from the third row to the one above the key line.
pub(super) fn page_rows(state: &State) -> i32 {
    i32::from(state.height) - 3
}

/// Draws a page taller than the screen through `draw_page`, which draws it from its first row,
/// and shows the rows it is scrolled to below the header.
fn scrolled(c: &mut Canvas, state: &State, height: i32, draw_page: impl FnOnce(&mut Canvas)) {
    let rows = page_rows(state);
    let height = height.max(rows);
    let area = Rect::new(0, 0, state.width, u16::try_from(height).unwrap_or(u16::MAX));
    let mut page = Buffer::empty(area);
    draw_page(&mut Canvas::new(&mut page, state.theme));
    let top = state.scroll.clamp(0, height - rows);
    c.copy_rows(&page, top, 2, rows);
}

/// How far a read of new transcripts has got, while one is under way: as the latest notification
/// says, or, before one comes, as the status does.
pub fn reading(state: &State) -> Option<(u64, u64)> {
    state
        .progress
        .or_else(|| state.status().and_then(|status| status.reading))
        .filter(|progress| progress.done < progress.total)
        .map(|progress| (progress.done, progress.total))
}

/// The header: the product, the sections with the selected one marked, and in the wide layout
/// the range and filters.
pub fn header(c: &mut Canvas, state: &State, selected: Option<Section>) {
    let theme = c.theme;
    let mut x = c.text(1, 0, "Dashboard for Claude Code", theme.strong()) + 3;
    for section in Section::ALL {
        let style = if Some(section) == selected {
            theme.selected()
        } else {
            theme.muted()
        };
        x = c.text(x, 0, &format!(" {} ", section.name()), style) + 1;
    }
    let filters = filters(state);
    if state.wide() {
        c.right(c.w - 1, 0, &filters, theme.muted());
    }
}

/// The range and filters, as the header names them: `30 days · all projects · all models`.
fn filters(state: &State) -> String {
    let project = state
        .project
        .as_ref()
        .map_or_else(|| "all projects".to_owned(), |id| project_name(state, id));
    let model = state
        .model
        .as_ref()
        .map_or_else(|| "all models".to_owned(), |id| model_name(state, id));
    format!("{} · {project} · {model}", state.range.title())
}

/// The filters chosen, for the narrow layouts, whose header has no room for them: `None` when
/// none is.
pub fn chosen_filters(state: &State) -> Option<String> {
    let parts: Vec<String> = [
        state.project.as_ref().map(|id| project_name(state, id)),
        state.model.as_ref().map(|id| model_name(state, id)),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!parts.is_empty() && !state.wide()).then(|| parts.join(" · "))
}

pub fn project_name(state: &State, id: &str) -> String {
    super::canvas::clean(
        &state
            .choices()
            .and_then(|choices| choices.projects.iter().find(|project| project.id == id))
            .map_or_else(|| id.to_owned(), |project| project.name.clone()),
    )
}

pub fn model_name(state: &State, id: &str) -> String {
    super::canvas::clean(
        &state
            .choices()
            .and_then(|choices| choices.models.iter().find(|model| model.id == id))
            .map_or_else(|| id.to_owned(), |model| model.name.clone()),
    )
}

/// The key line: keys at the left, as many as fit before the note at the right, which says when
/// the history was last read unless `note` says otherwise.
pub fn footer(c: &mut Canvas, state: &State, keys: &[(&str, &str)], note: Option<&str>) {
    let theme = c.theme;
    let y = c.h - 1;
    let (shown, style) = if let Some(note) = note {
        (note.to_owned(), theme.muted())
    } else {
        let (read, style) = read_note(state);
        let shown = if state.wide() && !read.is_empty() {
            format!("{read} · costs are API-equivalent estimates")
        } else {
            read
        };
        (shown, style)
    };
    let note_start = c.right(c.w - 1, y, &shown, style);
    let mut x = 1;
    for (key, label) in keys {
        if x + width(key) + 1 + width(label) > note_start - 2 {
            continue;
        }
        x = c.text(x, y, key, theme.key());
        x = c.text(x + 1, y, label, theme.muted()) + 3;
    }
}

/// When the history was last read, or how far the read under way has got.
fn read_note(state: &State) -> (String, Style) {
    let theme = state.theme;
    if let Some((done, total)) = reading(state) {
        return (
            format!(
                "Reading history · {} of {} files",
                grouped(done),
                grouped(total)
            ),
            theme.text(),
        );
    }
    match state.status().and_then(|status| status.read_ms) {
        Some(read_ms) => (
            format!(
                "Read {}",
                Timestamp::from_millisecond(read_ms)
                    .map_or_else(|_| "just now".to_owned(), |read| ago(read, state.now))
            ),
            theme.muted(),
        ),
        // The server has said a read ended, and the status has not caught up with it yet.
        None if state.progress.is_some() => ("Read just now".to_owned(), theme.muted()),
        None if state.status().is_some() => ("Reading history".to_owned(), theme.text()),
        None => (String::new(), theme.muted()),
    }
}

/// A line under the header that the section needs to say first, with a warning mark when it is
/// a warning; in the narrow layouts the chosen filters share the line, at its right.
pub fn notice(c: &mut Canvas, state: &State, text: &str, warning: bool) {
    let theme = c.theme;
    let mut end = c.w - 1;
    if let Some(filters) = chosen_filters(state) {
        end = c.right(c.w - 1, 1, &fit(&filters, 40), theme.muted()) - 3;
    }
    let mut x = 1;
    if warning {
        x = c.text(x, 1, "▲ ", theme.warn());
    }
    if !text.is_empty() {
        c.text(
            x,
            1,
            &fit(text, end - x),
            if warning { theme.text() } else { theme.muted() },
        );
    }
}

/// Lines in the middle of the screen, each centred, and a key line with nothing at its right.
fn message(c: &mut Canvas, lines: &[(&str, Style)], keys: Option<&[(&str, &str)]>, state: &State) {
    let count = i32::try_from(lines.len()).unwrap_or(0);
    let top = ((c.h - count) / 2 - 1).max(2);
    for (index, (text, style)) in (0..).zip(lines) {
        if text.is_empty() {
            continue;
        }
        let text = fit(text, c.w - 2);
        let x = ((c.w - width(&text)) / 2).max(1);
        c.text(x, top + index, &text, *style);
    }
    if let Some(keys) = keys {
        footer(c, state, keys, Some(" "));
    }
}

fn too_small(c: &mut Canvas) {
    let theme = c.theme;
    let size = format!("The window is {} by {}.", c.w, c.h);
    let lines = [
        (size.as_str(), theme.strong()),
        ("The dashboard needs at least 80 by 24.", theme.text()),
        ("", theme.text()),
        ("q quits", theme.muted()),
    ];
    let count = i32::try_from(lines.len()).unwrap_or(0);
    let top = ((c.h - count) / 2 - 1).max(0);
    for (index, (text, style)) in (0..).zip(lines) {
        let text = fit(text, c.w - 2);
        let x = ((c.w - width(&text)) / 2).max(1);
        c.text(x, top + index, &text, style);
    }
}

/// No history found, with the folders that were checked.
fn empty(c: &mut Canvas, state: &State) {
    let theme = c.theme;
    header(c, state, Some(Section::Overview));
    let folders: Vec<String> = state.health().map_or_else(Vec::new, |health| {
        health
            .folders
            .iter()
            .map(|folder| super::canvas::clean(&folder.path))
            .collect()
    });
    let looked = match folders.as_slice() {
        [] => String::new(),
        [one] => format!("We looked in {one}."),
        [rest @ .., last] => format!("We looked in {} and {last}.", rest.join(", ")),
    };
    message(
        c,
        &[
            ("No Claude Code history found", theme.strong()),
            ("", theme.text()),
            (&looked, theme.text()),
            (
                "Claude Code saves its history there after your first session.",
                theme.text(),
            ),
            (
                "If you keep it somewhere else, set CLAUDE_CONFIG_DIR to that folder.",
                theme.text(),
            ),
        ],
        Some(&[("q", "quit"), ("R", "check again"), ("?", "help")]),
        state,
    );
}

/// A history that a newer claude-dashboard upgraded beyond what this one reads: nothing else
/// shows.
fn too_new(c: &mut Canvas, schema: u32) {
    let theme = c.theme;
    c.text(1, 0, "Dashboard for Claude Code", theme.strong());
    let upgraded =
        format!("A newer version upgraded it (schema {schema}), and this one cannot read it.");
    let lines = [
        (
            "This history is newer than this claude-dashboard",
            theme.strong(),
        ),
        ("", theme.text()),
        (upgraded.as_str(), theme.text()),
        ("Update claude-dashboard to see it:", theme.text()),
        (
            "github.com/philll11/claude-dashboard/releases/latest",
            theme.accent(),
        ),
    ];
    let count = i32::try_from(lines.len()).unwrap_or(0);
    let top = ((c.h - count) / 2 - 1).max(2);
    for (index, (text, style)) in (0..).zip(lines) {
        if text.is_empty() {
            continue;
        }
        let text = fit(text, c.w - 2);
        let x = ((c.w - width(&text)) / 2).max(1);
        c.text(x, top + index, &text, style);
    }
    let y = c.h - 1;
    let x = c.text(1, y, "q", theme.key());
    c.text(x + 1, y, "quit", theme.muted());
}

/// The history cannot be opened: why, and where to find out more.
fn cannot_open(c: &mut Canvas, state: &State, error: &str) {
    let theme = c.theme;
    header(c, state, Some(Section::Overview));
    let error = super::canvas::clean(error);
    let mut lines: Vec<(String, Style)> = vec![
        ("The history cannot be opened".to_owned(), theme.strong()),
        (String::new(), theme.text()),
    ];
    lines.extend(
        wrap(&sentence(&error), c.w - 8)
            .into_iter()
            .map(|line| (line, theme.text())),
    );
    lines.push((String::new(), theme.text()));
    lines.push((
        "claude-dashboard doctor checks what it can find.".to_owned(),
        theme.muted(),
    ));
    let lines: Vec<(&str, Style)> = lines
        .iter()
        .map(|(text, style)| (text.as_str(), *style))
        .collect();
    message(
        c,
        &lines,
        Some(&[("q", "quit"), ("R", "try again"), ("?", "help")]),
        state,
    );
}

/// A message as a sentence: its first letter capital, and a full stop at its end.
fn sentence(text: &str) -> String {
    let mut chars = text.trim().chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out: String = first.to_uppercase().chain(chars).collect();
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}

/// Words in lines of at most `cells`, breaking a line after a colon where one falls.
fn wrap(text: &str, cells: i32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if width(&candidate) > cells && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            word.clone_into(&mut line);
        } else {
            line = candidate;
        }
        if line.ends_with(':') {
            lines.push(std::mem::take(&mut line));
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}
