//! `models`, `sessions` and `projects`: where the requests of a stretch of days went.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::process::ExitCode;

use claude_dashboard_core::history::{History, ProjectKey, SessionKey};
use claude_dashboard_core::report::{self, Range};
use jiff::Span;
use jiff::civil::Date;

use claude_dashboard_server::app::App;
use claude_dashboard_server::fmt::short_date;
use claude_dashboard_server::names::{model_name, project_names};
use claude_dashboard_server::protocol::Dataset;

use crate::report::{Output, Row, cells, ready_history, sum, table, total_row};
use crate::style::Paint;

/// The longest label a session or project gets in a table.
const LABEL_LIMIT: usize = 40;

/// The days to report on: `days` days up to today unless given.
fn days(app: &App, since: Option<Date>, until: Option<Date>, days: i64) -> (Date, Date) {
    let until = until.unwrap_or_else(|| app.now_zoned().date());
    let since = since.unwrap_or_else(|| {
        until
            .checked_sub(Span::new().days(days - 1))
            .unwrap_or(until)
    });
    (since, until)
}

fn heading(app: &App, since: Date, until: Date) -> String {
    format!(
        "{} to {} · API-equivalent estimate · prices of {}",
        short_date(since),
        short_date(until),
        short_date(app.prices.checked())
    )
}

/// `text` cut to `limit` characters, ending in an ellipsis when cut.
fn shorten(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_owned()
    } else {
        let mut short: String = text.chars().take(limit - 1).collect();
        short.push('…');
        short
    }
}

fn open(app: &App) -> Result<History, ExitCode> {
    ready_history(app)
}

pub fn models(app: &App, since: Option<Date>, until: Option<Date>, output: Output) -> ExitCode {
    let history = match open(app) {
        Ok(history) => history,
        Err(code) => return code,
    };
    let (since, until) = days(app, since, until, 30);
    if output != Output::Table {
        drop(history);
        return crate::export::print(
            app,
            &crate::export::params(Dataset::Models, output, since, until),
        );
    }
    let models = match report::by_model(&history, &app.prices, Range::days(since, until, &app.zone))
    {
        Ok(models) => models,
        Err(error) => {
            eprintln!("Cannot read the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    let paint = Paint::for_stdout();
    let mut out = format!("{}\n\n", paint.muted(&heading(app, since, until)));
    if models.is_empty() {
        let _ = writeln!(out, "{}", paint.muted("No requests on these days."));
    } else {
        let rows: Vec<Row> = models
            .iter()
            .map(|model| {
                let name = model_name(app, &model.model);
                Row {
                    label: format!(
                        "{} {name}",
                        paint.paint("■", paint.palette().series(&model.model), false)
                    ),
                    label_len: name.chars().count() + 2,
                    values: cells(&model.totals),
                }
            })
            .collect();
        out.push_str(&table(paint, "Model", &rows, &total_row(&sum(&models))));
    }
    print!("{out}");
    ExitCode::SUCCESS
}

/// A session in a report.
struct SessionView {
    session_id: String,
    title: Option<String>,
    project: String,
    first_ms: i64,
    totals: report::Totals,
}

pub fn sessions(app: &App, since: Option<Date>, until: Option<Date>, output: Output) -> ExitCode {
    let history = match open(app) {
        Ok(history) => history,
        Err(code) => return code,
    };
    let (since, until) = days(app, since, until, 7);
    if output != Output::Table {
        drop(history);
        return crate::export::print(
            app,
            &crate::export::params(Dataset::Sessions, output, since, until),
        );
    }
    let sessions =
        match report::by_session(&history, &app.prices, Range::days(since, until, &app.zone)) {
            Ok(sessions) => sessions,
            Err(error) => {
                eprintln!("Cannot read the history: {error}");
                return ExitCode::FAILURE;
            }
        };
    let details: HashMap<SessionKey, (String, Option<String>)> = history
        .sessions()
        .unwrap_or_default()
        .into_iter()
        .map(|session| (session.key, (session.session_id, session.title)))
        .collect();
    let projects = project_names(&history);
    let mut views: Vec<SessionView> = sessions
        .into_iter()
        .map(|session| {
            let (session_id, title) = details.get(&session.session).cloned().unwrap_or_default();
            SessionView {
                session_id,
                title,
                project: projects
                    .get(&session.project)
                    .map(|(name, _, _)| name.clone())
                    .unwrap_or_default(),
                first_ms: session.first_ms,
                totals: session.totals,
            }
        })
        .collect();
    // As the dashboard and the exports list them: the session started last first.
    views.sort_by(|a, b| {
        b.first_ms
            .cmp(&a.first_ms)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });
    print!("{}", sessions_table(app, since, until, &views));
    ExitCode::SUCCESS
}

fn sessions_table(app: &App, since: Date, until: Date, sessions: &[SessionView]) -> String {
    let paint = Paint::for_stdout();
    let mut out = format!("{}\n\n", paint.muted(&heading(app, since, until)));
    if sessions.is_empty() {
        let _ = writeln!(out, "{}", paint.muted("No sessions on these days."));
        return out;
    }
    let mut totals = report::Totals::default();
    let rows: Vec<Row> = sessions
        .iter()
        .map(|session| {
            add(&mut totals, &session.totals);
            let title = session
                .title
                .clone()
                .unwrap_or_else(|| session.session_id.chars().take(8).collect());
            let label = shorten(&format!("{title} · {}", session.project), LABEL_LIMIT);
            Row {
                label_len: label.chars().count(),
                label,
                values: cells(&session.totals),
            }
        })
        .collect();
    out.push_str(&table(paint, "Session", &rows, &total_row(&totals)));
    out
}

pub fn projects(app: &App, since: Option<Date>, until: Option<Date>, output: Output) -> ExitCode {
    let history = match open(app) {
        Ok(history) => history,
        Err(code) => return code,
    };
    let (since, until) = days(app, since, until, 30);
    if output != Output::Table {
        drop(history);
        return crate::export::print(
            app,
            &crate::export::params(Dataset::Projects, output, since, until),
        );
    }
    let totals =
        match report::by_project(&history, &app.prices, Range::days(since, until, &app.zone)) {
            Ok(projects) => projects,
            Err(error) => {
                eprintln!("Cannot read the history: {error}");
                return ExitCode::FAILURE;
            }
        };
    let names = project_names(&history);
    let name = |key: ProjectKey| names.get(&key).cloned().unwrap_or_default();
    let paint = Paint::for_stdout();
    let mut out = format!("{}\n\n", paint.muted(&heading(app, since, until)));
    if totals.is_empty() {
        let _ = writeln!(out, "{}", paint.muted("No requests on these days."));
    } else {
        let mut all = report::Totals::default();
        let rows: Vec<Row> = totals
            .iter()
            .map(|project| {
                add(&mut all, &project.totals);
                let label = shorten(&name(project.project).0, LABEL_LIMIT);
                Row {
                    label_len: label.chars().count(),
                    label,
                    values: cells(&project.totals),
                }
            })
            .collect();
        out.push_str(&table(paint, "Project", &rows, &total_row(&all)));
    }
    print!("{out}");
    ExitCode::SUCCESS
}

fn add(sum: &mut report::Totals, totals: &report::Totals) {
    sum.requests += totals.requests;
    sum.tokens.add(totals.tokens);
    sum.cost.add(totals.cost);
    sum.unpriced_requests += totals.unpriced_requests;
    sum.web_search_requests += totals.web_search_requests;
}

#[cfg(test)]
mod tests {
    use super::shorten;

    #[test]
    fn long_labels_end_in_an_ellipsis() {
        assert_eq!(shorten("Fix the importer", 40), "Fix the importer");
        assert_eq!(shorten("abcdefghij", 6), "abcde…");
    }
}
