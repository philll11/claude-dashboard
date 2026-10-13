//! The `claude-dashboard` command.

mod breakdown;
mod desktop;
mod doctor;
mod export;
mod import;
mod limits;
mod prices;
mod progress;
mod prompt;
mod report;
mod serve;
mod statusline;
mod style;
mod tui;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use claude_dashboard_core::report::Bucket;
use claude_dashboard_server::app::App;
use claude_dashboard_server::program;
use claude_dashboard_server::protocol::ExportFormat;
use jiff::civil::Date;

use crate::report::Output;

/// See how you use Claude Code: plan limits, usage, API-equivalent cost and where it goes.
///
/// Without a command, in a terminal, it opens the dashboard there.
#[derive(Debug, Parser)]
#[command(name = "claude-dashboard", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Today's requests and API-equivalent cost by model, and the plan limits.
    Today {
        #[command(flatten)]
        format: Format,
    },
    /// Requests, tokens and API-equivalent cost per day, for the last 30 days unless told otherwise.
    Daily {
        /// The first day, such as 2026-10-05.
        #[arg(long, value_name = "DATE")]
        since: Option<Date>,
        /// The last day; today unless given.
        #[arg(long, value_name = "DATE")]
        until: Option<Date>,
        #[command(flatten)]
        format: Format,
    },
    /// Requests, tokens and API-equivalent cost per week, for the last 12 weeks unless told
    /// otherwise.
    Weekly {
        /// A day in the first week, such as 2026-08-03.
        #[arg(long, value_name = "DATE")]
        since: Option<Date>,
        /// A day in the last week; this week unless given.
        #[arg(long, value_name = "DATE")]
        until: Option<Date>,
        #[command(flatten)]
        format: Format,
    },
    /// Requests, tokens and API-equivalent cost per calendar month, for the last 12 months unless
    /// told otherwise.
    Monthly {
        /// A day in the first month, such as 2026-01-01.
        #[arg(long, value_name = "DATE")]
        since: Option<Date>,
        /// A day in the last month; this month unless given.
        #[arg(long, value_name = "DATE")]
        until: Option<Date>,
        #[command(flatten)]
        format: Format,
    },
    /// Requests and API-equivalent cost by model, for the last 30 days unless told otherwise.
    Models {
        #[command(flatten)]
        days: Days,
        #[command(flatten)]
        format: Format,
    },
    /// Requests and API-equivalent cost by session, for the last 7 days unless told otherwise.
    Sessions {
        #[command(flatten)]
        days: Days,
        #[command(flatten)]
        format: Format,
    },
    /// Requests and API-equivalent cost by project, for the last 30 days unless told otherwise.
    Projects {
        #[command(flatten)]
        days: Days,
        #[command(flatten)]
        format: Format,
    },
    /// Write every request, or those of the days given, as CSV or JSON: to standard output, or to
    /// a file made for spreadsheets.
    Export {
        #[command(flatten)]
        days: Days,
        #[command(flatten)]
        format: Format,
        /// The file to write, made for spreadsheets: CSV gets a byte order mark, and text that a
        /// spreadsheet would take for a formula gets an apostrophe before it.
        #[arg(long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// The price table in use; switch the daily check for a newer one on or off, or check now.
    Prices {
        #[command(subcommand)]
        action: Option<PricesAction>,
    },
    /// Check what claude-dashboard can find and read.
    Doctor,
    /// Record the plan limits that Claude Code passes to its status line, and print a compact
    /// line.
    #[command(args_conflicts_with_subcommands = true)]
    Statusline {
        #[command(subcommand)]
        action: Option<StatuslineAction>,
        /// Your own status line command, which claude-dashboard runs after recording the plan
        /// limits and shows instead of its own line.
        #[arg(long, num_args = 1.., allow_hyphen_values = true, value_name = "COMMAND")]
        then: Vec<OsString>,
    },
    /// Bring in history that another tool recorded.
    Import {
        #[command(subcommand)]
        source: ImportSource,
    },
    /// Answer a client such as the VS Code extension in JSON-RPC, until its input closes.
    Serve {
        /// Talk over standard input and output, the only way there is.
        #[arg(long, required = true)]
        stdio: bool,
        /// Connect Claude Code's status line to a copy of this program kept next to the history,
        /// and keep that copy up to date, for a client whose updates move this program.
        #[arg(long)]
        status_line_copy: bool,
    },
    /// Each plan window with its use, reset and source, and the recent five-hour windows; or
    /// choose where plan limits come from, besides Claude Code's status line.
    #[command(args_conflicts_with_subcommands = true)]
    Limits {
        #[command(subcommand)]
        action: Option<LimitsAction>,
        #[command(flatten)]
        format: Format,
    },
    /// Open the desktop app, at a section when given.
    App {
        #[arg(value_enum)]
        section: Option<Section>,
    },
}

/// A section of the dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Section {
    Overview,
    Sessions,
    Projects,
    Models,
    Limits,
    Settings,
}

impl Section {
    /// The section's name, as the desktop app takes it.
    fn name(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Sessions => "sessions",
            Self::Projects => "projects",
            Self::Models => "models",
            Self::Limits => "limits",
            Self::Settings => "settings",
        }
    }
}

#[derive(Debug, Subcommand)]
enum LimitsAction {
    /// Turn Anthropic's undocumented usage endpoint on or off (experimental). It fills the gaps
    /// in plan limits while no Claude Code session runs in a terminal.
    Endpoint {
        #[arg(value_enum)]
        switch: Switch,
        /// Turn it on without asking.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Switch {
    On,
    Off,
}

#[derive(Debug, Subcommand)]
enum PricesAction {
    /// Turn the daily check for a newer price table in claude-dashboard's repository on GitHub on
    /// or off. A running dashboard checks once a day.
    Refresh {
        #[arg(value_enum)]
        switch: Switch,
    },
    /// Check claude-dashboard's repository for a newer price table now.
    Update,
}

#[derive(Debug, Subcommand)]
enum StatuslineAction {
    /// Make Claude Code run claude-dashboard's status line, keeping yours if you have one.
    Connect {
        /// Make the change without asking.
        #[arg(long)]
        yes: bool,
    },
    /// Undo `connect`.
    Disconnect {
        /// Make the change without asking.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ImportSource {
    /// The claude-usage dashboard's database: usage.db in Claude Code's folder, unless given.
    ClaudeUsage {
        #[arg(long, value_name = "PATH")]
        database: Option<PathBuf>,
    },
}

#[derive(Debug, Args)]
struct Days {
    /// The first day, such as 2026-10-05.
    #[arg(long, value_name = "DATE")]
    since: Option<Date>,
    /// The last day; today unless given.
    #[arg(long, value_name = "DATE")]
    until: Option<Date>,
}

#[derive(Debug, Args)]
#[group(multiple = false)]
struct Format {
    /// Write JSON.
    #[arg(long)]
    json: bool,
    /// Write CSV.
    #[arg(long)]
    csv: bool,
}

impl Format {
    fn output(&self) -> Output {
        if self.json {
            Output::Json
        } else if self.csv {
            Output::Csv
        } else {
            Output::Table
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let app = App::new().with_shared_settings();
    let Some(command) = cli.command else {
        return if tui::can_run() {
            tui::run(app)
        } else {
            help()
        };
    };
    match command {
        Command::Today { format } => report::today(&app, format.output()),
        Command::Daily {
            since,
            until,
            format,
        } => report::daily(&app, since, until, format.output()),
        Command::Weekly {
            since,
            until,
            format,
        } => report::periods(&app, Bucket::Week, since, until, format.output()),
        Command::Monthly {
            since,
            until,
            format,
        } => report::periods(&app, Bucket::Month, since, until, format.output()),
        Command::Models { days, format } => {
            breakdown::models(&app, days.since, days.until, format.output())
        }
        Command::Sessions { days, format } => {
            breakdown::sessions(&app, days.since, days.until, format.output())
        }
        Command::Projects { days, format } => {
            breakdown::projects(&app, days.since, days.until, format.output())
        }
        Command::Export {
            days,
            format,
            output,
        } => export::run(
            &app,
            days.since,
            days.until,
            if format.json {
                ExportFormat::Json
            } else {
                ExportFormat::Csv
            },
            output.as_deref(),
        ),
        Command::Prices { action } => match action {
            None => prices::status(&app),
            Some(PricesAction::Refresh { switch }) => prices::switch(&app, switch == Switch::On),
            Some(PricesAction::Update) => prices::update(&app),
        },
        Command::Doctor => doctor::run(&app),
        Command::Statusline { action, then } => match action {
            None => statusline::run(&app, &then),
            Some(StatuslineAction::Connect { yes }) => statusline::connect(&app, yes),
            Some(StatuslineAction::Disconnect { yes }) => statusline::disconnect(&app, yes),
        },
        Command::Import {
            source: ImportSource::ClaudeUsage { database },
        } => import::claude_usage(&app, database),
        Command::Limits {
            action: None,
            format,
        } => limits::report(&app, format.output()),
        Command::Limits {
            action: Some(LimitsAction::Endpoint { switch, yes }),
            ..
        } => limits::endpoint(&app, switch == Switch::On, yes),
        Command::App { section } => desktop::open(&app, section.map(Section::name)),
        Command::Serve {
            stdio: _,
            status_line_copy,
        } => serve::run(App {
            status_line_copy: status_line_copy.then(program::this).and_then(Result::ok),
            ..app
        }),
    }
}

/// The help, on standard error with the status a missing command gets, for a run without a
/// command that has no terminal to open the dashboard in.
fn help() -> ExitCode {
    let help = Cli::command().render_help();
    anstream::eprint!("{}", help.ansi());
    ExitCode::from(2)
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::Cli;

    #[test]
    fn command_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
