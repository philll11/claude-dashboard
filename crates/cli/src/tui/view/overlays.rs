//! What shows above a section, dimming it: the keys, the range, project and model pickers, and
//! the export box.

use claude_dashboard_server::protocol::ExportFormat;

use super::{footer, tail};
use crate::tui::canvas::{Canvas, clean, fit};
use crate::tui::state::{ExportBox, Overlay, Range, State};

pub fn draw(c: &mut Canvas, state: &State, overlay: &Overlay) {
    c.dim();
    match overlay {
        Overlay::Help => keys(c, state),
        Overlay::Range { hover } => range(c, state, *hover),
        Overlay::Project { hover, filter } => {
            let choices = state.project_choices(filter);
            picker(
                c,
                state,
                ("Project", filter),
                &choices,
                state.project.as_ref(),
                *hover,
            );
        }
        Overlay::Model { hover, filter } => {
            let choices = state.model_choices(filter);
            picker(
                c,
                state,
                ("Model", filter),
                &choices,
                state.model.as_ref(),
                *hover,
            );
        }
        Overlay::Export(export) => export_box(c, state, export),
    }
}

/// Clears the key line, for an overlay's own keys.
fn clear_footer(c: &mut Canvas) {
    let y = c.h - 1;
    c.clear(0, y, c.w, 1);
}

/// The keys the help lists, and what each does; a test presses every one of them.
pub const HELP_KEYS: [(&str, &str); 13] = [
    ("← →  Tab  1–5", "sections"),
    ("↑ ↓  j k", "move in a list"),
    ("PgUp PgDn", "a page at a time"),
    ("Home End", "top, bottom"),
    ("Enter", "open"),
    ("Esc", "back, close"),
    ("/", "search the sessions"),
    ("r  p  m", "range, project, model"),
    ("t", "charts or tables"),
    ("e", "export the section"),
    ("R", "read new transcripts now"),
    ("?", "this help"),
    ("q  Ctrl-C", "quit"),
];

fn keys(c: &mut Canvas, state: &State) {
    let theme = c.theme;
    let (w, h) = (58, 18);
    let x0 = (c.w - w) / 2;
    let y0 = (c.h - h) / 2;
    c.clear(x0, y0, w, h);
    c.boxed((x0, y0, w, h), Some("Keys"), None);
    for (row, (key, what)) in (0..).zip(HELP_KEYS) {
        c.text(x0 + 3, y0 + 2 + row, key, theme.key());
        c.text(x0 + 22, y0 + 2 + row, what, theme.text());
    }
    clear_footer(c);
    footer(c, state, &[("Esc", "close")], None);
}

fn range(c: &mut Canvas, state: &State, hover: usize) {
    let theme = c.theme;
    let w = 26;
    let x0 = c.w - w;
    c.clear(x0, 1, w, 9);
    c.boxed((x0, 1, w, 9), Some("Range"), None);
    for (row, range) in (0..).zip(Range::ALL) {
        let y = 2 + row;
        c.text(x0 + 4, y, range.name(), theme.text());
        if range == state.range {
            c.right(x0 + w - 2, y, "✓", theme.good());
        }
        if usize::try_from(row).is_ok_and(|row| row == hover) {
            c.select(y, x0 + 1, x0 + w - 2);
        }
    }
    c.text(x0 + 2, 8, "Enter applies", theme.muted());
    clear_footer(c);
    footer(
        c,
        state,
        &[("↑↓", "choose"), ("Enter", "apply"), ("Esc", "close")],
        None,
    );
}

/// A list to choose from, filtered by what is typed, with the current choice ticked.
fn picker(
    c: &mut Canvas,
    state: &State,
    (title, filter): (&str, &str),
    choices: &[(Option<String>, String)],
    chosen: Option<&String>,
    hover: usize,
) {
    let theme = c.theme;
    let (width, height) = (44, 15);
    let left = (c.w - width) / 2;
    let top = 4;
    c.clear(left, top, width, height);
    c.boxed((left, top, width, height), Some(title), None);
    let typed = c.text(left + 2, top + 1, "Filter ", theme.muted());
    let typed = c.text(
        typed,
        top + 1,
        &fit(filter, left + width - 3 - typed),
        theme.strong(),
    );
    c.put(typed, top + 1, "█", theme.accent());
    let rows = usize::try_from(height - 7).unwrap_or(1);
    let first = hover.saturating_sub(rows - 1);
    for (row, (id, name)) in (0..).zip(choices.iter().skip(first).take(rows)) {
        let y = top + 3 + row;
        c.text(left + 4, y, &fit(&clean(name), width - 8), theme.text());
        if id.as_ref() == chosen {
            c.right(left + width - 2, y, "✓", theme.good());
        }
        if first + usize::try_from(row).unwrap_or(0) == hover {
            c.select(y, left + 1, left + width - 2);
        }
    }
    if choices.is_empty() {
        c.text(left + 4, top + 3, "Nothing matches", theme.muted());
    }
    c.text(
        left + 2,
        top + height - 2,
        "Typing filters the list",
        theme.muted(),
    );
    clear_footer(c);
    footer(
        c,
        state,
        &[("↑↓", "choose"), ("Enter", "apply"), ("Esc", "close")],
        None,
    );
}

/// The export box: the format, the file to save to with a cursor at its end, what the export
/// holds, and a question before a file that is there already is replaced.
fn export_box(c: &mut Canvas, state: &State, export: &ExportBox) {
    let theme = c.theme;
    let w = (c.w - 6).min(68);
    let h = 11;
    let x0 = (c.w - w) / 2;
    let y0 = (c.h - h) / 2 - 1;
    c.clear(x0, y0, w, h);
    c.boxed((x0, y0, w, h), Some(&export.title), None);
    c.text(x0 + 2, y0 + 2, "Format", theme.muted());
    let formats = [
        (ExportFormat::Csv, "CSV", "for spreadsheets"),
        (ExportFormat::Json, "JSON", "for scripts"),
    ];
    for (row, (format, name, use_for)) in (0..).zip(formats) {
        let y = y0 + 2 + row;
        let chosen = format == export.params.format;
        if chosen {
            c.select(y, x0 + 11, x0 + w - 2);
        }
        c.text(
            x0 + 14,
            y,
            name,
            if chosen { theme.strong() } else { theme.text() },
        );
        c.text(x0 + 20, y, use_for, theme.muted());
    }
    c.text(x0 + 2, y0 + 5, "Save to", theme.muted());
    let end = c.text(
        x0 + 14,
        y0 + 5,
        &tail(&clean(&export.path), w - 14 - 3),
        theme.text(),
    );
    if !export.replacing {
        c.put(end, y0 + 5, "█", theme.accent());
    }
    c.text(x0 + 2, y0 + 7, &fit(&export.scope, w - 4), theme.muted());
    if export.replacing {
        let x = c.text(x0 + 2, y0 + 8, "▲ ", theme.warn());
        c.text(
            x,
            y0 + 8,
            &fit("A file is there already. Replace it?", x0 + w - 2 - x),
            theme.text(),
        );
    } else {
        c.text(
            x0 + 2,
            y0 + 8,
            &fit(
                "A file there already is replaced only when you say so.",
                w - 4,
            ),
            theme.muted(),
        );
    }
    clear_footer(c);
    let keys: &[(&str, &str)] = if export.replacing {
        &[("y", "replace"), ("n", "keep it")]
    } else {
        &[("↑↓", "format"), ("Enter", "save"), ("Esc", "cancel")]
    };
    footer(c, state, keys, None);
}
