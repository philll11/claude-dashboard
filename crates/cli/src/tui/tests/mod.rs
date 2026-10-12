//! The interface's tests: every screen and state drawn from made-up answers, as a terminal shows
//! them, and how keys and events change the state.

mod fixture;

use std::cell::RefCell;
use std::io::{self, Write};
use std::rc::Rc;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::prelude::CrosstermBackend;
use ratatui::{Terminal, TerminalOptions, Viewport};
use serde_json::json;

use claude_dashboard_server::protocol::{Dataset, ExportFormat, Exported};

use self::fixture::{Fixture, Variant};
use super::state::{Event, ExportNotice, Flow, Overlay, Range, Section, State};
use super::theme::Theme;
use crate::style::Palette;

/// Bytes a terminal would receive.
#[derive(Clone, Default)]
struct Received(Rc<RefCell<Vec<u8>>>);

impl Write for Received {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn dark() -> Theme {
    Theme::new(&Palette::DARK, false, true)
}

/// A state of `width` by `height` with every answer its screen needs from `fixture`.
fn state(
    fixture: &Fixture,
    (width, height): (u16, u16),
    theme: Theme,
    set: impl FnOnce(&mut State),
) -> State {
    let mut state = State::new(theme, (width, height), fixture.now);
    set(&mut state);
    answer(&mut state, fixture);
    state
}

/// Answers what the state asks, until it asks nothing more.
fn answer(state: &mut State, fixture: &Fixture) {
    for _ in 0..10 {
        let asks = state.asks();
        if asks.is_empty() {
            return;
        }
        for ask in asks {
            let result = fixture.answer(ask.method, &ask.params);
            state.update(Event::Answer { id: ask.id, result });
        }
    }
}

/// What a terminal shows once the state is drawn through crossterm: each row's text, each row's
/// styles as letters, and what each letter stands for.
fn shown(state: &State) -> String {
    ratatui::crossterm::style::force_color_output(true);
    let received = Received::default();
    let area = Rect::new(0, 0, state.width, state.height);
    let mut terminal = Terminal::with_options(
        CrosstermBackend::new(received.clone()),
        TerminalOptions {
            viewport: Viewport::Fixed(area),
        },
    )
    .expect("a terminal of a fixed size needs nothing from the backend");
    terminal
        .draw(|frame| super::view::draw(frame.buffer_mut(), state))
        .expect("drawing into memory cannot fail");
    drop(terminal);
    let mut parser = vt100::Parser::new(state.height, state.width, 0);
    parser.process(&received.0.borrow());
    describe(parser.screen())
}

fn colour(colour: vt100::Color) -> Option<String> {
    match colour {
        vt100::Color::Default => None,
        vt100::Color::Idx(index) => Some(format!("colour {index}")),
        vt100::Color::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
    }
}

fn describe(screen: &vt100::Screen) -> String {
    const MARKS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let (rows, columns) = screen.size();
    let mut text = Vec::new();
    let mut marks = Vec::new();
    let mut legend: Vec<String> = Vec::new();
    for row in 0..rows {
        let mut line = String::new();
        let mut styled = String::new();
        for column in 0..columns {
            let Some(cell) = screen.cell(row, column) else {
                continue;
            };
            if cell.is_wide_continuation() {
                continue;
            }
            let contents = cell.contents();
            line.push_str(if contents.is_empty() { " " } else { contents });
            let mut parts = Vec::new();
            if let Some(fg) = colour(cell.fgcolor()) {
                parts.push(fg);
            }
            if let Some(bg) = colour(cell.bgcolor()) {
                parts.push(format!("on {bg}"));
            }
            if cell.bold() {
                parts.push("bold".to_owned());
            }
            if cell.inverse() {
                parts.push("reverse".to_owned());
            }
            let style = parts.join(" ");
            if style.is_empty() {
                styled.push(' ');
                continue;
            }
            let index = legend
                .iter()
                .position(|known| *known == style)
                .unwrap_or_else(|| {
                    legend.push(style);
                    legend.len() - 1
                });
            styled.push(MARKS.chars().nth(index).unwrap_or('?'));
        }
        text.push(format!("{row:>2} │{line}│"));
        marks.push(format!("{row:>2} │{styled}│"));
    }
    let legend: Vec<String> = legend
        .iter()
        .enumerate()
        .map(|(index, style)| format!("{} = {style}", MARKS.chars().nth(index).unwrap_or('?')))
        .collect();
    format!(
        "{}\n\nstyles\n{}\n\n{}\n",
        text.join("\n"),
        marks.join("\n"),
        legend.join("\n")
    )
}

/// Draws the screen `set` leads to, at the board sizes, and checks it against its snapshot.
fn check(name: &str, variant: Variant, size: (u16, u16), set: impl FnOnce(&mut State)) {
    let fixture = Fixture::new(variant);
    let state = state(&fixture, size, dark(), set);
    insta::assert_snapshot!(format!("{name}_{}x{}", size.0, size.1), shown(&state));
}

const WIDE: (u16, u16) = (120, 36);
const NARROW: (u16, u16) = (80, 24);

#[test]
fn the_overview() {
    check("overview", Variant::Normal, WIDE, |_| {});
    check("overview", Variant::Normal, NARROW, |_| {});
}

#[test]
fn the_overview_makes_room_for_more_models_and_adds_up_the_rest() {
    check("overview_many_models", Variant::ManyModels, WIDE, |_| {});
    check(
        "overview_many_models",
        Variant::ManyModels,
        (120, 38),
        |_| {},
    );
}

#[test]
fn the_sessions() {
    check("sessions", Variant::Normal, WIDE, |state| {
        state.section = Section::Sessions;
        state.sessions.selected = 2;
    });
    check("sessions_searching", Variant::Normal, NARROW, |state| {
        state.section = Section::Sessions;
        state.search.text = "ingest".to_owned();
        state.search.applied = "ingest".to_owned();
        state.search.typing = true;
    });
}

#[test]
fn a_session() {
    for size in [WIDE, NARROW] {
        check("session", Variant::Normal, size, |state| {
            state.section = Section::Sessions;
            state.session = Some("4f1c2a3b-0000-4000-8000-000000000000".to_owned());
        });
    }
}

#[test]
fn the_projects() {
    for size in [WIDE, NARROW] {
        check("projects", Variant::Normal, size, |state| {
            state.section = Section::Projects;
        });
    }
}

#[test]
fn the_models() {
    for size in [WIDE, NARROW] {
        check("models", Variant::Normal, size, |state| {
            state.section = Section::Models;
        });
    }
}

#[test]
fn the_limits() {
    for size in [WIDE, NARROW] {
        check("limits", Variant::Normal, size, |state| {
            state.section = Section::Limits;
        });
        check("limits_not_recorded", Variant::LimitsOff, size, |state| {
            state.section = Section::Limits;
        });
    }
}

#[test]
fn the_keys_and_the_pickers() {
    check("help", Variant::Normal, NARROW, |state| {
        state.overlay = Some(Overlay::Help);
    });
    check("range", Variant::Normal, NARROW, |state| {
        state.overlay = Some(Overlay::Range { hover: 3 });
    });
    check("project_picker", Variant::Normal, NARROW, |state| {
        state.overlay = Some(Overlay::Project {
            hover: 1,
            filter: String::new(),
        });
    });
}

/// Where the boards save an export.
const SAVED_TO: &str =
    r"C:\Users\you\Downloads\claude-dashboard-sessions-2026-09-11-to-2026-10-10.csv";

#[test]
fn the_export_box_and_what_it_saved() {
    let (mut state, fixture) = ready(NARROW);
    state.update(key(KeyCode::Right));
    answer(&mut state, &fixture);
    state.sessions.selected = 2;
    state.update(typed('e'));
    let Some(Overlay::Export(export)) = state.overlay.as_mut() else {
        panic!("e opens the export box");
    };
    export.path = SAVED_TO.to_owned();
    insta::assert_snapshot!("export_80x24", shown(&state));
    check("export_saved", Variant::Normal, NARROW, |state| {
        state.section = Section::Sessions;
        state.sessions.selected = 2;
        state.export = Some(ExportNotice::Saved {
            what: "214 sessions".to_owned(),
            path: SAVED_TO.to_owned(),
        });
    });
}

#[test]
fn e_exports_the_section_shown_and_says_where_it_went() {
    let (mut state, fixture) = ready(NARROW);
    state.update(key(KeyCode::Right));
    answer(&mut state, &fixture);
    state.update(typed('e'));
    let Some(Overlay::Export(export)) = &state.overlay else {
        panic!("e opens the export box");
    };
    assert_eq!(export.title, "Export the sessions");
    assert_eq!(
        export.scope,
        "All 214 sessions in the last 30 days that match."
    );
    assert_eq!(export.params.dataset, Dataset::Sessions);
    assert!(
        export
            .path
            .ends_with("claude-dashboard-sessions-2026-09-11-to-2026-10-10.csv"),
        "{}",
        export.path
    );

    state.update(key(KeyCode::Down));
    let Some(Overlay::Export(export)) = &state.overlay else {
        panic!("the box stays open");
    };
    assert_eq!(export.params.format, ExportFormat::Json);
    assert!(export.path.ends_with("-2026-10-10.json"), "{}", export.path);

    state.update(key(KeyCode::Enter));
    assert_eq!(state.overlay, None);
    let (params, path) = state.take_export().expect("Enter starts the export");
    assert_eq!(params.format, ExportFormat::Json);
    assert!(path.ends_with("claude-dashboard-sessions-2026-09-11-to-2026-10-10.json"));
    assert!(matches!(state.export, Some(ExportNotice::Running(_))));
    assert_eq!(state.take_export(), None);

    state.update(Event::Exported(Ok(Exported {
        rows: 214,
        what: "214 sessions".to_owned(),
        path: Some(path.display().to_string()),
        content: None,
    })));
    assert!(matches!(state.export, Some(ExportNotice::Saved { .. })));
    state.update(key(KeyCode::Down));
    assert_eq!(state.export, None, "the next key puts the notice away");

    state.update(typed('e'));
    state.update(key(KeyCode::Enter));
    state.take_export();
    state.update(Event::Exported(Err(
        r"Cannot write to D:\: the disk is full".to_owned(),
    )));
    assert!(matches!(state.export, Some(ExportNotice::Failed(_))));
}

#[test]
fn a_file_there_already_is_replaced_only_when_you_say_so() {
    let folder = tempfile::tempdir().unwrap();
    let there = folder.path().join("sessions.csv");
    std::fs::write(&there, "kept").unwrap();
    let (mut state, fixture) = ready(NARROW);
    state.update(key(KeyCode::Right));
    answer(&mut state, &fixture);
    state.update(typed('e'));
    if let Some(Overlay::Export(export)) = state.overlay.as_mut() {
        export.path = there.display().to_string();
    }

    state.update(key(KeyCode::Enter));
    assert!(matches!(&state.overlay, Some(Overlay::Export(export)) if export.replacing));
    assert_eq!(state.take_export(), None);
    state.update(typed('n'));
    assert!(matches!(&state.overlay, Some(Overlay::Export(export)) if !export.replacing));

    state.update(key(KeyCode::Enter));
    state.update(typed('y'));
    assert_eq!(state.take_export().map(|(_, path)| path), Some(there));
}

#[test]
fn the_states() {
    for (name, variant) in [
        ("reading", Variant::Reading),
        ("empty", Variant::Empty),
        ("import", Variant::Import),
        ("limits_off", Variant::LimitsOff),
        ("stale", Variant::Stale),
        ("drift", Variant::Drift),
        ("folder", Variant::Folder),
        ("read_only", Variant::ReadOnly),
        ("too_new", Variant::TooNew),
        ("error", Variant::Error),
    ] {
        check(name, variant, NARROW, |_| {});
    }
    check("endpoint", Variant::Endpoint, NARROW, |state| {
        state.section = Section::Limits;
    });
    check("unpriced", Variant::Unpriced, NARROW, |state| {
        state.section = Section::Models;
    });
    check("too_small", Variant::Normal, (72, 20), |_| {});
}

#[test]
fn the_layouts_stretch_to_the_window() {
    for size in [(100, 30), (160, 48)] {
        check("overview", Variant::Normal, size, |_| {});
        check("sessions", Variant::Normal, size, |state| {
            state.section = Section::Sessions;
        });
        check("session", Variant::Normal, size, |state| {
            state.section = Section::Sessions;
            state.session = Some("4f1c2a3b-0000-4000-8000-000000000000".to_owned());
        });
        check("projects", Variant::Normal, size, |state| {
            state.section = Section::Projects;
        });
        check("models", Variant::Normal, size, |state| {
            state.section = Section::Models;
        });
        check("limits", Variant::Normal, size, |state| {
            state.section = Section::Limits;
        });
    }
}

#[test]
fn the_charts_as_tables() {
    check("overview_tables", Variant::Normal, WIDE, |state| {
        state.tables = true;
    });
    check("session_tables", Variant::Normal, NARROW, |state| {
        state.tables = true;
        state.section = Section::Sessions;
        state.session = Some("4f1c2a3b-0000-4000-8000-000000000000".to_owned());
    });
}

#[test]
fn a_page_taller_than_the_window_scrolls() {
    check("session_scrolled", Variant::Normal, NARROW, |state| {
        state.section = Section::Sessions;
        state.session = Some("4f1c2a3b-0000-4000-8000-000000000000".to_owned());
        state.scroll = 19;
    });
}

#[test]
fn the_overview_on_a_light_background_and_without_colour() {
    let fixture = Fixture::new(Variant::Normal);
    let light = state(
        &fixture,
        WIDE,
        Theme::new(&Palette::LIGHT, false, true),
        |_| {},
    );
    insta::assert_snapshot!("overview_light_120x36", shown(&light));
    let mono = state(
        &fixture,
        WIDE,
        Theme::new(&Palette::DARK, true, true),
        |_| {},
    );
    insta::assert_snapshot!("overview_no_colour_120x36", shown(&mono));
    let ansi = state(
        &fixture,
        NARROW,
        Theme::new(&Palette::DARK, false, false),
        |_| {},
    );
    insta::assert_snapshot!("overview_256_colours_80x24", shown(&ansi));
}

#[test]
fn sparklines_do_without_eighths_of_a_cell_where_they_do_not_show() {
    let fixture = Fixture::new(Variant::Normal);
    let overview = state(&fixture, WIDE, dark().coarse(), |_| {});
    insta::assert_snapshot!("overview_coarse_120x36", shown(&overview));
    let limits = state(&fixture, WIDE, dark().coarse(), |state| {
        state.section = Section::Limits;
    });
    insta::assert_snapshot!("limits_coarse_120x36", shown(&limits));
}

// How keys and events change the state.

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn typed(c: char) -> Event {
    key(KeyCode::Char(c))
}

fn ready(size: (u16, u16)) -> (State, Fixture) {
    let fixture = Fixture::new(Variant::Normal);
    let state = state(&fixture, size, dark(), |_| {});
    (state, fixture)
}

#[test]
fn sections_change_with_arrows_tabs_and_numbers() {
    let (mut state, _) = ready(NARROW);
    state.update(key(KeyCode::Right));
    assert_eq!(state.section, Section::Sessions);
    state.update(key(KeyCode::Left));
    state.update(key(KeyCode::Left));
    assert_eq!(state.section, Section::Limits, "counting round");
    state.update(key(KeyCode::Tab));
    assert_eq!(state.section, Section::Overview);
    state.update(key(KeyCode::BackTab));
    assert_eq!(state.section, Section::Limits);
    state.update(typed('3'));
    assert_eq!(state.section, Section::Projects);
}

#[test]
fn q_and_ctrl_c_quit_but_q_types_in_the_search() {
    let (mut state, fixture) = ready(NARROW);
    state.update(typed('/'));
    assert_eq!(state.section, Section::Sessions);
    assert_eq!(state.update(typed('q')), Flow::Continue);
    assert_eq!(state.search.text, "q");
    state.update(key(KeyCode::Esc));
    assert_eq!(state.search.text, "");
    assert!(!state.search.typing);
    answer(&mut state, &fixture);
    assert_eq!(state.update(typed('q')), Flow::Quit);
    let control_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert_eq!(state.update(control_c), Flow::Quit);
}

#[test]
fn a_character_typed_with_altgr_is_text_in_the_search() {
    let (mut state, _) = ready(NARROW);
    state.update(typed('/'));
    let altgr = Event::Key(KeyEvent::new(
        KeyCode::Char('@'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    ));
    assert_eq!(state.update(altgr), Flow::Continue);
    assert_eq!(state.search.text, "@");
}

#[test]
fn the_search_is_asked_once_typing_pauses() {
    let (mut state, fixture) = ready(NARROW);
    state.update(typed('/'));
    for c in "ingest".chars() {
        state.update(typed(c));
    }
    assert_eq!(state.search.applied, "", "not while typing");
    let due = state.deadline().expect("a search is due");
    state.at(due);
    assert_eq!(state.search.applied, "ingest");
    answer(&mut state, &fixture);
    let page = state.sessions_page().expect("the search is answered");
    assert!(page.rows.iter().all(|row| {
        row.title
            .as_deref()
            .is_some_and(|title| title.to_lowercase().contains("ingest"))
    }));
    assert_eq!(state.sessions_count(), Some(214));
}

#[test]
fn enter_opens_the_selected_session_and_esc_goes_back() {
    let (mut state, fixture) = ready(NARROW);
    state.update(typed('2'));
    answer(&mut state, &fixture);
    state.update(key(KeyCode::Down));
    state.update(key(KeyCode::Down));
    state.update(key(KeyCode::Enter));
    assert_eq!(
        state.session.as_deref(),
        Some("4f1c2a3b-0000-4000-8000-000000000002")
    );
    state.update(key(KeyCode::Esc));
    assert_eq!(state.session, None);
    assert_eq!(state.sessions.selected, 2, "the list stays where it was");
}

#[test]
fn the_list_scrolls_a_page_at_a_time_and_to_its_ends() {
    let (mut state, fixture) = ready(NARROW);
    state.update(typed('2'));
    answer(&mut state, &fixture);
    let rows = state.session_rows();
    state.update(key(KeyCode::PageDown));
    assert_eq!(state.sessions.selected, rows);
    assert_eq!(state.sessions.top, 1);
    state.update(key(KeyCode::End));
    assert_eq!(state.sessions.selected, 213);
    assert_eq!(state.sessions.top, 214 - rows);
    state.update(key(KeyCode::Home));
    assert_eq!((state.sessions.selected, state.sessions.top), (0, 0));
}

#[test]
fn the_range_picker_applies_a_range_and_starts_lists_from_the_top() {
    let (mut state, _) = ready(NARROW);
    state.sessions.selected = 5;
    state.update(typed('r'));
    assert_eq!(state.overlay, Some(Overlay::Range { hover: 2 }));
    state.update(key(KeyCode::Down));
    state.update(key(KeyCode::Enter));
    assert_eq!(state.range, Range::ThisMonth);
    assert_eq!(state.overlay, None);
    assert_eq!(state.sessions.selected, 0);
}

#[test]
fn the_project_picker_filters_as_it_is_typed() {
    let (mut state, _) = ready(NARROW);
    state.update(typed('p'));
    for c in "gate".chars() {
        state.update(typed(c));
    }
    let Some(Overlay::Project { filter, .. }) = &state.overlay else {
        panic!("the picker is open");
    };
    assert_eq!(filter, "gate");
    assert_eq!(
        state.project_choices(filter),
        [
            (None, "All projects".to_owned()),
            (
                Some("C--code-api-gateway".to_owned()),
                "api-gateway".to_owned()
            )
        ]
    );
    state.update(key(KeyCode::Down));
    state.update(key(KeyCode::Enter));
    assert_eq!(state.project.as_deref(), Some("C--code-api-gateway"));
}

#[test]
fn enter_on_a_project_filters_the_dashboard_to_it() {
    let (mut state, fixture) = ready(WIDE);
    state.update(typed('3'));
    answer(&mut state, &fixture);
    state.update(key(KeyCode::Down));
    state.update(key(KeyCode::Enter));
    assert_eq!(state.project.as_deref(), Some("C--code-api-gateway"));
    assert_eq!(state.section, Section::Overview);
}

#[test]
fn a_window_smaller_than_80_by_24_only_quits() {
    let (mut state, _) = ready(NARROW);
    state.update(Event::Resize(72, 20));
    assert!(state.too_small());
    state.update(typed('2'));
    assert_eq!(state.section, Section::Overview);
    assert_eq!(state.update(typed('q')), Flow::Quit);
}

#[test]
fn the_history_changing_asks_again_for_what_shows() {
    let (mut state, _) = ready(NARROW);
    assert_eq!(state.asks(), []);
    state.update(Event::Notification {
        method: "history/changed".to_owned(),
        params: json!({ "days": null }),
    });
    let asked: Vec<&str> = state.asks().iter().map(|ask| ask.method).collect();
    assert!(asked.contains(&"status"));
    assert!(asked.contains(&"overview"));
}

#[test]
fn a_read_in_progress_shows_until_it_ends_whatever_an_older_status_says() {
    let fixture = Fixture::new(Variant::Reading);
    let mut state = state(&fixture, NARROW, dark(), |_| {});
    assert!(shown(&state).contains("Reading history · 412 of 2,748 files"));
    state.update(Event::Notification {
        method: "ingest/progress".to_owned(),
        params: json!({ "done": 12, "total": 40 }),
    });
    assert!(shown(&state).contains("Reading history · 12 of 40 files"));
    state.update(Event::Notification {
        method: "ingest/progress".to_owned(),
        params: json!({ "done": 40, "total": 40 }),
    });
    assert!(!shown(&state).contains("Reading history"));
    let asked: Vec<&str> = state.asks().iter().map(|ask| ask.method).collect();
    assert!(
        asked.contains(&"status"),
        "when the history was read changes"
    );
}

#[test]
fn r_reads_new_transcripts_and_asks_again_for_everything() {
    let (mut state, _) = ready(NARROW);
    state.update(typed('R'));
    let asked: Vec<&str> = state.asks().iter().map(|ask| ask.method).collect();
    assert!(asked.contains(&"refresh"));
    assert!(asked.contains(&"status"));
}

#[test]
fn text_from_transcripts_stays_on_its_line_and_never_reaches_the_terminal_as_a_sequence() {
    let fixture = Fixture::new(Variant::Normal);
    let mut state = state(&fixture, NARROW, dark(), |state| {
        state.section = Section::Sessions;
        state.search.applied = "dist".to_owned();
        state.search.text = "dist".to_owned();
    });
    state.update(typed('/'));
    state.update(typed('x'));
    let due = state.deadline().expect("a search is due");
    state.at(due);
    let ask = state
        .asks()
        .into_iter()
        .find(|ask| ask.method == "sessions" && ask.params["search"] == "distx")
        .expect("the new search is asked");
    let mut page = fixture
        .answer(
            "sessions",
            &json!({ "days": ask.params["days"], "search": "dist", "limit": 500 }),
        )
        .expect("the fixture answers");
    page["rows"][0]["title"] = json!("Evil\u{1b}[31mred\nline\ttab");
    state.update(Event::Answer {
        id: ask.id,
        result: Ok(page),
    });
    let screen = shown(&state);
    let row = screen
        .lines()
        .find(|line| line.contains("Evil"))
        .expect("the title shows");
    assert!(row.contains("Evil [31mred line tab"), "{row}");
}
