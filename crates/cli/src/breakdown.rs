//! `models`, `sessions` and `projects`: where the requests of a stretch of days went.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::process::ExitCode;

use claude_dashboard_core::history::{History, ProjectKey, SessionKey};
use claude_dashboard_core::report::{self, Range};
use jiff::civil::Date;
use jiff::{Span, Timestamp};
use serde::Serialize;

use crate::app::App;
use crate::fmt::short_date;
use crate::report::{
    CostJson, Output, Row, TokensJson, cells, cost_json, model_name, print_json, ready_history,
    series_colour, sum, table, tokens_json, total_row,
};
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

fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
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
    let models = match report::by_model(&history, app.prices, Range::days(since, until, &app.zone))
    {
        Ok(models) => models,
        Err(error) => {
            eprintln!("Cannot read the history: {error}");
            return ExitCode::FAILURE;
        }
    };
    match output {
        Output::Table => {
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
                                paint.paint("■", series_colour(&model.model), false)
                            ),
                            label_len: name.chars().count() + 2,
                            values: cells(&model.totals),
                        }
                    })
                    .collect();
                out.push_str(&table(paint, "Model", &rows, &total_row(&sum(&models))));
            }
            print!("{out}");
        }
        Output::Json => {
            #[derive(Serialize)]
            struct ModelJson {
                model: String,
                name: Option<String>,
                requests: u64,
                tokens: TokensJson,
                cost: CostJson,
            }
            let models: Vec<ModelJson> = models
                .iter()
                .map(|model| ModelJson {
                    model: model.model.clone(),
                    name: app.prices.model_name(&model.model).map(str::to_owned),
                    requests: model.totals.requests,
                    tokens: tokens_json(&model.totals),
                    cost: cost_json(app, &model.totals),
                })
                .collect();
            print_json(&models);
        }
        Output::Csv => {
            let mut out = String::from(
                "model,requests,input_tokens,output_tokens,cache_write_5m_tokens,cache_write_1h_tokens,cache_read_tokens,cost_usd,unpriced_requests,price_table\n",
            );
            for model in &models {
                let t = model.totals.tokens;
                let _ = writeln!(
                    out,
                    "{},{},{},{},{},{},{},{:.2},{},{}",
                    csv_field(&model.model),
                    model.totals.requests,
                    t.input,
                    t.output,
                    t.cache_write_5m,
                    t.cache_write_1h,
                    t.cache_read,
                    model.totals.cost.total(),
                    model.totals.unpriced_requests,
                    app.prices.checked()
                );
            }
            print!("{out}");
        }
    }
    ExitCode::SUCCESS
}

/// What reports call a project: the last part of its working folder, or the last two parts when
/// another project ends in the same folder name, or else its transcript folder.
fn project_names(history: &History) -> HashMap<ProjectKey, (String, Option<String>, String)> {
    let projects = history.projects().unwrap_or_default();
    let tail = |path: &str, parts: usize| {
        let parts: Vec<&str> = path
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .take(parts)
            .collect();
        parts.into_iter().rev().collect::<Vec<_>>().join("/")
    };
    let mut seen: HashMap<String, usize> = HashMap::new();
    for project in &projects {
        if let Some(path) = &project.path {
            *seen.entry(tail(path, 1)).or_default() += 1;
        }
    }
    projects
        .into_iter()
        .map(|project| {
            let name = project.path.as_deref().map_or_else(
                || project.transcript_folder.clone(),
                |path| {
                    let short = tail(path, 1);
                    if seen.get(&short).copied().unwrap_or(0) > 1 {
                        tail(path, 2)
                    } else {
                        short
                    }
                },
            );
            (project.key, (name, project.path, project.transcript_folder))
        })
        .collect()
}

/// A session in a report.
struct SessionView {
    session_id: String,
    title: Option<String>,
    project: String,
    first_ms: i64,
    last_ms: i64,
    totals: report::Totals,
    subagent_requests: u64,
}

pub fn sessions(app: &App, since: Option<Date>, until: Option<Date>, output: Output) -> ExitCode {
    let history = match open(app) {
        Ok(history) => history,
        Err(code) => return code,
    };
    let (since, until) = days(app, since, until, 7);
    let sessions =
        match report::by_session(&history, app.prices, Range::days(since, until, &app.zone)) {
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
    let views: Vec<SessionView> = sessions
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
                last_ms: session.last_ms,
                totals: session.totals,
                subagent_requests: session.subagents.requests,
            }
        })
        .collect();
    match output {
        Output::Table => print!("{}", sessions_table(app, since, until, &views)),
        Output::Json => sessions_json(app, &views),
        Output::Csv => print!("{}", sessions_csv(&views)),
    }
    ExitCode::SUCCESS
}

fn at(ms: i64) -> String {
    Timestamp::from_millisecond(ms)
        .map(|at| at.to_string())
        .unwrap_or_default()
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

fn sessions_json(app: &App, sessions: &[SessionView]) {
    #[derive(Serialize)]
    struct SessionJson<'a> {
        session_id: &'a str,
        title: Option<&'a str>,
        project: &'a str,
        first_request_at: String,
        last_request_at: String,
        requests: u64,
        subagent_requests: u64,
        tokens: TokensJson,
        cost: CostJson,
    }
    let sessions: Vec<SessionJson<'_>> = sessions
        .iter()
        .map(|session| SessionJson {
            session_id: &session.session_id,
            title: session.title.as_deref(),
            project: &session.project,
            first_request_at: at(session.first_ms),
            last_request_at: at(session.last_ms),
            requests: session.totals.requests,
            subagent_requests: session.subagent_requests,
            tokens: tokens_json(&session.totals),
            cost: cost_json(app, &session.totals),
        })
        .collect();
    print_json(&sessions);
}

fn sessions_csv(sessions: &[SessionView]) -> String {
    let mut out = String::from(
        "session_id,title,project,first_request_at,last_request_at,requests,subagent_requests,output_tokens,cache_read_tokens,cost_usd,unpriced_requests\n",
    );
    for session in sessions {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{:.2},{}",
            csv_field(&session.session_id),
            csv_field(session.title.as_deref().unwrap_or_default()),
            csv_field(&session.project),
            at(session.first_ms),
            at(session.last_ms),
            session.totals.requests,
            session.subagent_requests,
            session.totals.tokens.output,
            session.totals.tokens.cache_read,
            session.totals.cost.total(),
            session.totals.unpriced_requests
        );
    }
    out
}

pub fn projects(app: &App, since: Option<Date>, until: Option<Date>, output: Output) -> ExitCode {
    let history = match open(app) {
        Ok(history) => history,
        Err(code) => return code,
    };
    let (since, until) = days(app, since, until, 30);
    let totals =
        match report::by_project(&history, app.prices, Range::days(since, until, &app.zone)) {
            Ok(projects) => projects,
            Err(error) => {
                eprintln!("Cannot read the history: {error}");
                return ExitCode::FAILURE;
            }
        };
    let names = project_names(&history);
    let name = |key: ProjectKey| names.get(&key).cloned().unwrap_or_default();
    match output {
        Output::Table => {
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
        }
        Output::Json => {
            #[derive(Serialize)]
            struct ProjectJson {
                name: String,
                path: Option<String>,
                transcript_folder: String,
                sessions: u64,
                requests: u64,
                tokens: TokensJson,
                cost: CostJson,
            }
            let projects: Vec<ProjectJson> = totals
                .iter()
                .map(|project| {
                    let (name, path, transcript_folder) = name(project.project);
                    ProjectJson {
                        name,
                        path,
                        transcript_folder,
                        sessions: project.sessions,
                        requests: project.totals.requests,
                        tokens: tokens_json(&project.totals),
                        cost: cost_json(app, &project.totals),
                    }
                })
                .collect();
            print_json(&projects);
        }
        Output::Csv => {
            let mut out = String::from(
                "name,path,sessions,requests,output_tokens,cache_read_tokens,cost_usd,unpriced_requests\n",
            );
            for project in &totals {
                let (name, path, _) = name(project.project);
                let _ = writeln!(
                    out,
                    "{},{},{},{},{},{},{:.2},{}",
                    csv_field(&name),
                    csv_field(&path.unwrap_or_default()),
                    project.sessions,
                    project.totals.requests,
                    project.totals.tokens.output,
                    project.totals.tokens.cache_read,
                    project.totals.cost.total(),
                    project.totals.unpriced_requests
                );
            }
            print!("{out}");
        }
    }
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
    use super::{csv_field, shorten};

    #[test]
    fn long_labels_end_in_an_ellipsis() {
        assert_eq!(shorten("Fix the importer", 40), "Fix the importer");
        assert_eq!(shorten("abcdefghij", 6), "abcde…");
    }

    #[test]
    fn csv_fields_with_commas_or_quotes_are_quoted() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a, b"), "\"a, b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }
}
