//! The interface's state, changed only by events: keys, answers, notifications and the passing of
//! time. Drawing is a function of the state alone.

use std::time::{Duration, Instant};

use claude_dashboard_core::report::Bucket;
use claude_dashboard_server::app::weekday;
use claude_dashboard_server::protocol::{
    Choices, Days, Health, ImportOffer, Limits, Overview, Prices, Progress, SessionDetail,
    SessionsPage, Status,
};
use jiff::civil::{Date, Weekday};
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use super::answers::{Answer, Answers, Ask, Request};
use super::theme::Theme;

/// How long typing in the search pauses before the search is asked.
const SEARCH_PAUSE: Duration = Duration::from_millis(250);
/// How many sessions one request asks for; the list shows a window of them.
const SESSIONS_PAGE: usize = 500;

/// What happens to the interface.
#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    Resize(u16, u16),
    Answer {
        id: u64,
        result: Result<Value, String>,
    },
    Notification {
        method: String,
        params: Value,
    },
    /// A minute has passed, or about to: the times shown move on.
    Tick(Timestamp),
    /// The terminal is going away, or the process is asked to end, as only signals outside
    /// Windows say.
    #[cfg_attr(not(unix), allow(dead_code))]
    Quit,
    /// Something the interface depends on stopped working.
    Failed(String),
}

/// Whether the interface carries on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Overview,
    Sessions,
    Projects,
    Models,
    Limits,
}

impl Section {
    pub const ALL: [Self; 5] = [
        Self::Overview,
        Self::Sessions,
        Self::Projects,
        Self::Models,
        Self::Limits,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Sessions => "Sessions",
            Self::Projects => "Projects",
            Self::Models => "Models",
            Self::Limits => "Limits",
        }
    }

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|section| *section == self)
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    Today,
    SevenDays,
    ThirtyDays,
    ThisMonth,
    AllTime,
}

impl Range {
    pub const ALL: [Self; 5] = [
        Self::Today,
        Self::SevenDays,
        Self::ThirtyDays,
        Self::ThisMonth,
        Self::AllTime,
    ];

    /// The range as its choice names it: `30 days`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::SevenDays => "7 days",
            Self::ThirtyDays => "30 days",
            Self::ThisMonth => "This month",
            Self::AllTime => "All time",
        }
    }

    /// The range in a title: `30 days`, `this month`.
    pub fn title(self) -> String {
        match self {
            Self::SevenDays | Self::ThirtyDays => self.name().to_owned(),
            _ => self.name().to_lowercase(),
        }
    }

    /// The days the range covers, as of the server's today.
    pub fn days(self, status: &Status) -> Days {
        let today = day(&status.today);
        let first = match self {
            Self::Today => today,
            Self::SevenDays => today.checked_sub(6.days()).unwrap_or(today),
            Self::ThirtyDays => today.checked_sub(29.days()).unwrap_or(today),
            Self::ThisMonth => today.first_of_month(),
            Self::AllTime => status
                .first_day
                .as_deref()
                .map(day)
                .filter(|first| *first < today)
                .unwrap_or(today),
        };
        Days {
            first: first.to_string(),
            last: today.to_string(),
        }
    }
}

/// A day as the server writes it, such as `2026-10-10`.
pub fn day(text: &str) -> Date {
    text.parse().unwrap_or_default()
}

/// What shows above the section, taking the keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overlay {
    Help,
    Range { hover: usize },
    Project { hover: usize, filter: String },
    Model { hover: usize, filter: String },
}

/// The sessions' search: what is typed, and what was last asked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Search {
    pub text: String,
    /// Set while keys type into the search.
    pub typing: bool,
    /// The search the list shows.
    pub applied: String,
    /// When typing pauses long enough to ask.
    due: Option<Instant>,
}

/// A position in a list: the selected row, and the first row shown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct List {
    pub selected: usize,
    pub top: usize,
}

impl List {
    /// Moves the selection by `by` rows within `len` rows, keeping it in a window of `rows`.
    fn step(&mut self, by: isize, len: usize, rows: usize) {
        if len == 0 {
            *self = Self::default();
            return;
        }
        self.selected = self.selected.saturating_add_signed(by).min(len - 1);
        self.show(rows);
    }

    fn show(&mut self, rows: usize) {
        let rows = rows.max(1);
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top + rows {
            self.top = self.selected + 1 - rows;
        }
    }
}

#[derive(Debug)]
pub struct State {
    pub theme: Theme,
    pub width: u16,
    pub height: u16,
    pub section: Section,
    pub range: Range,
    /// A project's id, or every project.
    pub project: Option<String>,
    /// A model's id, or every model.
    pub model: Option<String>,
    /// Charts show as tables.
    pub tables: bool,
    pub overlay: Option<Overlay>,
    pub search: Search,
    pub sessions: List,
    pub projects: List,
    /// The session open in Sessions.
    pub session: Option<String>,
    /// How far a page that does not fit the screen is scrolled.
    pub scroll: i32,
    /// How far a chart's table is scrolled.
    pub table_top: usize,
    pub answers: Answers,
    /// How far the latest read of new transcripts has got, as the server last said; it says so
    /// sooner than an answer to `status` does.
    pub progress: Option<Progress>,
    /// The time the screen shows, moved on once a minute.
    pub now: Timestamp,
    commands: Vec<Ask>,
}

impl State {
    pub fn new(theme: Theme, (width, height): (u16, u16), now: Timestamp) -> Self {
        Self {
            theme,
            width,
            height,
            section: Section::Overview,
            range: Range::ThirtyDays,
            project: None,
            model: None,
            tables: false,
            overlay: None,
            search: Search::default(),
            sessions: List::default(),
            projects: List::default(),
            session: None,
            scroll: 0,
            table_top: 0,
            answers: Answers::default(),
            progress: None,
            now,
            commands: Vec::new(),
        }
    }

    /// Wide enough and tall enough for the wide layouts of the boards.
    pub fn wide(&self) -> bool {
        self.width >= 110 && self.height >= 36
    }

    /// Smaller than the dashboard needs.
    pub fn too_small(&self) -> bool {
        self.width < 80 || self.height < 24
    }

    /// The rows of the sessions list.
    pub fn session_rows(&self) -> usize {
        usize::from(self.height.saturating_sub(9)).max(1)
    }

    // The answers the screen shows.

    pub fn status(&self) -> Option<&Status> {
        match self.answers.get(&status_request()) {
            Some(Answer::Status(status)) => Some(status),
            _ => None,
        }
    }

    /// Why the server could not say how the history stands.
    pub fn status_error(&self) -> Option<&str> {
        self.answers.error(&status_request())
    }

    pub fn health(&self) -> Option<&Health> {
        match self.answers.get(&Request::new("health", Value::Null)) {
            Some(Answer::Health(health)) => Some(health),
            _ => None,
        }
    }

    pub fn choices(&self) -> Option<&Choices> {
        match self.answers.get(&Request::new("choices", Value::Null)) {
            Some(Answer::Choices(choices)) => Some(choices),
            _ => None,
        }
    }

    pub fn limits(&self) -> Option<&Limits> {
        match self.answers.get(&Request::new("limits", Value::Null)) {
            Some(Answer::Limits(limits)) => Some(limits),
            _ => None,
        }
    }

    pub fn import_offer(&self) -> Option<&ImportOffer> {
        match self.answers.get(&Request::new("import/offer", Value::Null)) {
            Some(Answer::ImportOffer(offer)) => offer.as_deref(),
            _ => None,
        }
    }

    /// The price table and how it agrees with Claude Code, without any model's prices.
    pub fn price_check(&self) -> Option<&Prices> {
        self.prices(&prices_request(&[]))
    }

    pub fn prices(&self, request: &Request) -> Option<&Prices> {
        match self.answers.get(request) {
            Some(Answer::Prices(prices)) => Some(prices),
            _ => None,
        }
    }

    pub fn overview(&self, request: &Request) -> Option<&Overview> {
        match self.answers.get(request) {
            Some(Answer::Overview(overview)) => Some(overview),
            _ => None,
        }
    }

    pub fn sessions_page(&self) -> Option<&SessionsPage> {
        match self.answers.get(&self.sessions_request()?) {
            Some(Answer::Sessions(page)) => Some(page),
            _ => None,
        }
    }

    pub fn session_detail(&self) -> Option<&SessionDetail> {
        match self.answers.get(&self.session_request()?) {
            Some(Answer::Session(detail)) => Some(detail),
            _ => None,
        }
    }

    /// Why the open session cannot be shown.
    pub fn session_error(&self) -> Option<&str> {
        self.answers.error(&self.session_request()?)
    }

    // The requests behind each part of the screen.

    /// The days the chosen range covers.
    pub fn days(&self) -> Option<Days> {
        Some(self.range.days(self.status()?))
    }

    pub fn zone(&self) -> TimeZone {
        self.status()
            .and_then(|status| TimeZone::get(&status.time_zone).ok())
            .unwrap_or(TimeZone::UTC)
    }

    pub fn week_start(&self) -> Weekday {
        self.status()
            .and_then(|status| weekday(&status.week_start))
            .unwrap_or(Weekday::Monday)
    }

    /// The days and filters chosen, added up.
    pub fn overview_request(&self) -> Option<Request> {
        Some(overview_request(
            &self.days()?,
            &filters(self.project.as_deref(), self.model.as_deref()),
        ))
    }

    /// Today, this week and this month, with the days before each to compare them with.
    pub fn tiles_request(&self) -> Option<Request> {
        let status = self.status()?;
        let today = day(&status.today);
        let week = Bucket::Week.start(today, self.week_start());
        let first = [
            today.checked_sub(29.days()).unwrap_or(today),
            today.first_of_month(),
            week.checked_sub(7.days()).unwrap_or(week),
        ]
        .into_iter()
        .min()
        .unwrap_or(today);
        Some(overview_request(
            &Days {
                first: first.to_string(),
                last: today.to_string(),
            },
            &filters(self.project.as_deref(), self.model.as_deref()),
        ))
    }

    /// Every project, for the Projects table.
    pub fn projects_request(&self) -> Option<Request> {
        Some(overview_request(
            &self.days()?,
            &filters(None, self.model.as_deref()),
        ))
    }

    /// Every model, for the Models section.
    pub fn models_request(&self) -> Option<Request> {
        Some(overview_request(
            &self.days()?,
            &filters(self.project.as_deref(), None),
        ))
    }

    /// The selected project's days, for the chart under the Projects table.
    pub fn project_chart_request(&self) -> Option<Request> {
        let id = self.selected_project()?;
        Some(overview_request(
            &self.days()?,
            &filters(Some(&id), self.model.as_deref()),
        ))
    }

    /// The prices of the models the Models section lists.
    pub fn models_prices_request(&self) -> Option<Request> {
        let overview = self.overview(&self.models_request()?)?;
        let models: Vec<&str> = overview
            .models
            .iter()
            .filter(|row| row.model.name != row.model.id)
            .map(|row| row.model.id.as_str())
            .collect();
        Some(prices_request(&models))
    }

    pub fn sessions_request(&self) -> Option<Request> {
        Some(Request::new(
            "sessions",
            json!({
                "days": self.days()?,
                "filters": filters(self.project.as_deref(), self.model.as_deref()),
                "search": (!self.search.applied.is_empty()).then_some(&self.search.applied),
                "offset": self.sessions_offset(),
                "limit": SESSIONS_PAGE,
            }),
        ))
    }

    /// The first session the sessions answer holds: the list's top, to the half page below it,
    /// so that the answer holds every row shown.
    pub fn sessions_offset(&self) -> usize {
        self.sessions.top / (SESSIONS_PAGE / 2) * (SESSIONS_PAGE / 2)
    }

    /// Every session in the chosen days and filters, whatever the search, for the count the
    /// search narrows.
    fn sessions_count_request(&self) -> Option<Request> {
        Some(Request::new(
            "sessions",
            json!({
                "days": self.days()?,
                "filters": filters(self.project.as_deref(), self.model.as_deref()),
                "search": null,
                "offset": 0,
                "limit": 1,
            }),
        ))
    }

    /// How many sessions there are in the chosen days and filters, whatever the search.
    pub fn sessions_count(&self) -> Option<u64> {
        match self.answers.get(&self.sessions_count_request()?) {
            Some(Answer::Sessions(page)) => Some(page.total),
            _ => None,
        }
    }

    pub fn session_request(&self) -> Option<Request> {
        Some(Request::new(
            "session",
            json!({ "id": self.session.as_deref()? }),
        ))
    }

    /// Everything the screen needs, in the order to ask for it.
    fn needed(&self) -> Vec<Request> {
        let mut needed = vec![
            status_request(),
            Request::new("health", Value::Null),
            Request::new("choices", Value::Null),
        ];
        if self.status().is_none() {
            return needed;
        }
        let mut add = |request: Option<Request>| needed.extend(request);
        match self.section {
            Section::Overview => {
                add(self.overview_request());
                add(self.tiles_request());
                add(Some(Request::new("limits", Value::Null)));
                add(Some(prices_request(&[])));
                add(Some(Request::new("import/offer", Value::Null)));
            }
            Section::Sessions => {
                if self.session.is_some() {
                    add(self.session_request());
                } else {
                    add(self.sessions_request());
                    if !self.search.applied.is_empty() {
                        add(self.sessions_count_request());
                    }
                }
            }
            Section::Projects => {
                add(self.projects_request());
                add(self.project_chart_request());
            }
            Section::Models => {
                add(self.models_request());
                add(self.models_prices_request());
                add(Some(prices_request(&[])));
            }
            Section::Limits => add(Some(Request::new("limits", Value::Null))),
        }
        needed
    }

    /// The requests to send now: what the screen needs and has no current answer to, and any
    /// command, such as reading new transcripts now.
    pub fn asks(&mut self) -> Vec<Ask> {
        let needed = self.needed();
        let mut asks = self.answers.due(&needed);
        asks.append(&mut self.commands);
        asks
    }

    /// When the search is next due to be asked.
    pub fn deadline(&self) -> Option<Instant> {
        self.search.due
    }

    /// Asks the search typed so far once typing has paused long enough.
    pub fn at(&mut self, now: Instant) {
        if self.search.due.is_some_and(|due| due <= now) {
            self.search.due = None;
            self.apply_search();
        }
    }

    fn apply_search(&mut self) {
        if self.search.applied != self.search.text {
            self.search.applied.clone_from(&self.search.text);
            self.sessions = List::default();
        }
    }

    pub fn update(&mut self, event: Event) -> Flow {
        match event {
            Event::Key(key) => return self.key(key),
            Event::Resize(width, height) => {
                self.width = width;
                self.height = height;
                let rows = self.session_rows();
                self.sessions.show(rows);
            }
            Event::Answer { id, result } => self.answers.answered(id, result),
            Event::Notification { method, params } => self.notified(&method, &params),
            Event::Tick(now) => self.now = now,
            Event::Quit | Event::Failed(_) => return Flow::Quit,
        }
        Flow::Continue
    }

    fn notified(&mut self, method: &str, params: &Value) {
        match method {
            "history/changed" => self.answers.outdate(|_| true),
            "limits/changed" => self
                .answers
                .outdate(|method| matches!(method, "status" | "limits")),
            "ingest/progress" => {
                self.progress = serde_json::from_value(params.clone()).ok();
                // A read that ends changes when the history was last read.
                if self
                    .progress
                    .is_none_or(|progress| progress.done >= progress.total)
                {
                    self.answers
                        .outdate(|method| matches!(method, "status" | "health"));
                }
            }
            _ => {}
        }
    }

    // Keys.

    fn key(&mut self, key: KeyEvent) -> Flow {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        if control && !alt && matches!(key.code, KeyCode::Char('c' | 'C')) {
            return Flow::Quit;
        }
        // Windows reports a character typed with AltGr as Ctrl and Alt together, so that pair
        // counts as text where text is typed.
        let text = match key.code {
            KeyCode::Char(c) if !control || alt => Some(c),
            _ => None,
        };
        if self.too_small() {
            return if text == Some('q') {
                Flow::Quit
            } else {
                Flow::Continue
            };
        }
        if let Some(overlay) = self.overlay.take() {
            self.overlay_key(overlay, key.code, text);
            return Flow::Continue;
        }
        if self.section == Section::Sessions && self.search.typing {
            self.search_key(key.code, text);
            return Flow::Continue;
        }
        if let Some(c) = text {
            return self.command_key(c);
        }
        if control || alt {
            return Flow::Continue;
        }
        let last = Section::ALL.len() - 1;
        match key.code {
            KeyCode::Left | KeyCode::BackTab => self.go(self.section.index() + last),
            KeyCode::Right | KeyCode::Tab => self.go(self.section.index() + 1),
            KeyCode::Esc => self.back(),
            KeyCode::Enter => self.open(),
            KeyCode::Up => self.step(-1),
            KeyCode::Down => self.step(1),
            KeyCode::PageUp => self.step(-self.page()),
            KeyCode::PageDown => self.step(self.page()),
            KeyCode::Home => self.step(isize::MIN / 2),
            KeyCode::End => self.step(isize::MAX / 2),
            _ => {}
        }
        Flow::Continue
    }

    /// A character typed outside the search and the pickers: a command.
    fn command_key(&mut self, c: char) -> Flow {
        match c {
            'q' => return Flow::Quit,
            '?' => self.overlay = Some(Overlay::Help),
            '1'..='5' => {
                let index = c
                    .to_digit(10)
                    .and_then(|digit| usize::try_from(digit).ok())
                    .map_or(0, |digit| digit - 1);
                self.go(index);
            }
            'r' => {
                let hover = Range::ALL
                    .iter()
                    .position(|range| *range == self.range)
                    .unwrap_or(0);
                self.overlay = Some(Overlay::Range { hover });
            }
            'p' => {
                let hover = self
                    .project_choices("")
                    .iter()
                    .position(|(id, _)| *id == self.project)
                    .unwrap_or(0);
                self.overlay = Some(Overlay::Project {
                    hover,
                    filter: String::new(),
                });
            }
            'm' => {
                let hover = self
                    .model_choices("")
                    .iter()
                    .position(|(id, _)| *id == self.model)
                    .unwrap_or(0);
                self.overlay = Some(Overlay::Model {
                    hover,
                    filter: String::new(),
                });
            }
            't' => {
                self.tables = !self.tables;
                self.table_top = 0;
            }
            'R' => {
                let refresh = self.answers.command("refresh", Value::Null);
                self.commands.push(refresh);
                // What could not be answered, such as a history that could not be opened, is
                // asked again too.
                self.answers.outdate(|_| true);
            }
            '/' => {
                self.section = Section::Sessions;
                self.session = None;
                self.search.typing = true;
            }
            'k' => self.step(-1),
            'j' => self.step(1),
            _ => {}
        }
        Flow::Continue
    }

    /// Goes to the section at `index`, counting round.
    fn go(&mut self, index: usize) {
        let section = Section::ALL[index % Section::ALL.len()];
        if section != self.section {
            self.section = section;
            self.scroll = 0;
            self.table_top = 0;
        }
    }

    /// How far a page moves at a time.
    fn page(&self) -> isize {
        isize::try_from(self.session_rows()).unwrap_or(1).max(1)
    }

    fn step(&mut self, by: isize) {
        match self.section {
            Section::Sessions if self.session.is_some() => self.scroll_by(by),
            Section::Sessions => {
                let len = self
                    .sessions_page()
                    .map_or(0, |page| usize::try_from(page.total).unwrap_or(usize::MAX));
                let rows = self.session_rows();
                self.sessions.step(by, len, rows);
            }
            Section::Projects => {
                let len = super::view::projects::rows(self).len();
                let rows = super::view::projects::visible_rows(self);
                self.projects.step(by, len, rows);
            }
            Section::Overview | Section::Models if self.tables => {
                self.table_top = self.table_top.saturating_add_signed(by);
            }
            Section::Overview | Section::Models | Section::Limits => self.scroll_by(by),
        }
    }

    fn scroll_by(&mut self, by: isize) {
        let most = super::view::page_overflow(self);
        let by = i32::try_from(by.clamp(-100_000, 100_000)).unwrap_or(0);
        self.scroll = self.scroll.saturating_add(by).clamp(0, most.max(0));
    }

    fn back(&mut self) {
        if self.section == Section::Sessions && self.session.is_some() {
            self.session = None;
            self.scroll = 0;
        } else if self.section == Section::Sessions && !self.search.applied.is_empty() {
            self.search = Search::default();
            self.sessions = List::default();
        }
    }

    fn open(&mut self) {
        match self.section {
            Section::Sessions if self.session.is_none() => {
                if let Some(id) = self.selected_session() {
                    self.session = Some(id);
                    self.scroll = 0;
                }
            }
            Section::Projects => {
                match super::view::projects::rows(self).get(self.projects.selected) {
                    Some(super::view::projects::Row::Project(row)) => {
                        self.project = Some(row.project.id.clone());
                        self.section = Section::Overview;
                        self.filtered();
                    }
                    Some(super::view::projects::Row::Others(_)) => {
                        self.overlay = Some(Overlay::Project {
                            hover: 0,
                            filter: String::new(),
                        });
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }

    /// The id of the session the list selects.
    pub fn selected_session(&self) -> Option<String> {
        let page = self.sessions_page()?;
        let index = self.sessions.selected.checked_sub(self.sessions_offset())?;
        Some(page.rows.get(index)?.id.clone())
    }

    /// The id of the project the Projects table selects, if it is one project.
    pub fn selected_project(&self) -> Option<String> {
        match super::view::projects::rows(self).get(self.projects.selected)? {
            super::view::projects::Row::Project(row) => Some(row.project.id.clone()),
            super::view::projects::Row::Others(_) => None,
        }
    }

    /// A new range or filter: lists start from the top.
    fn filtered(&mut self) {
        self.sessions = List::default();
        self.projects = List::default();
        self.scroll = 0;
        self.table_top = 0;
    }

    fn search_key(&mut self, code: KeyCode, text: Option<char>) {
        match (code, text) {
            (_, Some(c)) => {
                self.search.text.push(c);
                self.search.due = Some(Instant::now() + SEARCH_PAUSE);
            }
            (KeyCode::Backspace, _) => {
                self.search.text.pop();
                self.search.due = Some(Instant::now() + SEARCH_PAUSE);
            }
            (KeyCode::Esc, _) => {
                self.search = Search::default();
                self.sessions = List::default();
            }
            (KeyCode::Enter, _) => {
                self.search.typing = false;
                self.search.due = None;
                let changed = self.search.applied != self.search.text;
                self.apply_search();
                if !changed {
                    self.open();
                }
            }
            (KeyCode::Up, _) => self.step(-1),
            (KeyCode::Down, _) => self.step(1),
            (KeyCode::PageUp, _) => self.step(-self.page()),
            (KeyCode::PageDown, _) => self.step(self.page()),
            _ => {}
        }
    }

    /// The projects to choose from, `None` standing for every project, with their names, those
    /// whose name or folder contains `filter`.
    pub fn project_choices(&self, filter: &str) -> Vec<(Option<String>, String)> {
        let filter = filter.to_lowercase();
        let mut choices = vec![(None, "All projects".to_owned())];
        if let Some(found) = self.choices() {
            choices.extend(
                found
                    .projects
                    .iter()
                    .filter(|project| {
                        filter.is_empty()
                            || project.name.to_lowercase().contains(&filter)
                            || project
                                .path
                                .as_deref()
                                .is_some_and(|path| path.to_lowercase().contains(&filter))
                    })
                    .map(|project| (Some(project.id.clone()), project.name.clone())),
            );
        }
        choices
    }

    /// The models to choose from, `None` standing for every model, with their names.
    pub fn model_choices(&self, filter: &str) -> Vec<(Option<String>, String)> {
        let filter = filter.to_lowercase();
        let mut choices = vec![(None, "All models".to_owned())];
        if let Some(found) = self.choices() {
            choices.extend(
                found
                    .models
                    .iter()
                    .filter(|model| {
                        filter.is_empty()
                            || model.name.to_lowercase().contains(&filter)
                            || model.id.to_lowercase().contains(&filter)
                    })
                    .map(|model| (Some(model.id.clone()), model.name.clone())),
            );
        }
        choices
    }

    fn overlay_key(&mut self, overlay: Overlay, code: KeyCode, text: Option<char>) {
        match overlay {
            Overlay::Help => {
                if !matches!(code, KeyCode::Esc | KeyCode::Char('?' | 'q')) {
                    self.overlay = Some(Overlay::Help);
                }
            }
            Overlay::Range { hover } => {
                let len = Range::ALL.len();
                match code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.overlay = Some(Overlay::Range {
                            hover: hover.saturating_sub(1),
                        });
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.overlay = Some(Overlay::Range {
                            hover: (hover + 1).min(len - 1),
                        });
                    }
                    KeyCode::Enter => {
                        self.range = Range::ALL[hover.min(len - 1)];
                        self.filtered();
                    }
                    KeyCode::Esc | KeyCode::Char('r') => {}
                    _ => self.overlay = Some(Overlay::Range { hover }),
                }
            }
            Overlay::Project { hover, filter } => {
                let choices = self.project_choices(&filter);
                if let Some((hover, filter)) = picker_key(code, text, hover, filter, choices.len())
                {
                    self.overlay = Some(Overlay::Project { hover, filter });
                } else if code == KeyCode::Enter
                    && let Some((id, _)) = choices.get(hover)
                {
                    self.project.clone_from(id);
                    self.filtered();
                }
            }
            Overlay::Model { hover, filter } => {
                let choices = self.model_choices(&filter);
                if let Some((hover, filter)) = picker_key(code, text, hover, filter, choices.len())
                {
                    self.overlay = Some(Overlay::Model { hover, filter });
                } else if code == KeyCode::Enter
                    && let Some((id, _)) = choices.get(hover)
                {
                    self.model.clone_from(id);
                    self.filtered();
                }
            }
        }
    }
}

/// A key in a picker with a filter: the picker as it is after the key, or `None` once it closes,
/// as Enter and Esc close it.
fn picker_key(
    code: KeyCode,
    text: Option<char>,
    hover: usize,
    mut filter: String,
    len: usize,
) -> Option<(usize, String)> {
    match (code, text) {
        (KeyCode::Enter | KeyCode::Esc, _) => None,
        (_, Some(c)) => {
            filter.push(c);
            Some((0, filter))
        }
        (KeyCode::Backspace, _) => {
            filter.pop();
            Some((0, filter))
        }
        (KeyCode::Up, _) => Some((hover.saturating_sub(1), filter)),
        (KeyCode::Down, _) => Some(((hover + 1).min(len.saturating_sub(1)), filter)),
        (KeyCode::PageUp, _) => Some((hover.saturating_sub(8), filter)),
        (KeyCode::PageDown, _) => Some(((hover + 8).min(len.saturating_sub(1)), filter)),
        (KeyCode::Home, _) => Some((0, filter)),
        (KeyCode::End, _) => Some((len.saturating_sub(1), filter)),
        _ => Some((hover, filter)),
    }
}

pub fn status_request() -> Request {
    Request::new("status", Value::Null)
}

fn filters(project: Option<&str>, model: Option<&str>) -> Value {
    json!({ "project": project, "model": model })
}

pub fn overview_request(days: &Days, filters: &Value) -> Request {
    Request::new("overview", json!({ "days": days, "filters": filters }))
}

pub fn prices_request(models: &[&str]) -> Request {
    Request::new("prices", json!({ "models": models }))
}
