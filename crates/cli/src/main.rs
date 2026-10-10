//! The `claude-dashboard` command.

mod app;
mod breakdown;
mod doctor;
mod fmt;
mod import;
mod limits;
mod prompt;
mod report;
mod statusline;
mod style;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use jiff::civil::Date;

use crate::app::App;
use crate::report::Output;

/// See how you use Claude Code: plan limits, usage, API-equivalent cost and where it goes.
#[derive(Debug, Parser)]
#[command(
    name = "claude-dashboard",
    version,
    about,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
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
    /// Choose where plan limits come from, besides Claude Code's status line.
    Limits {
        #[command(subcommand)]
        action: LimitsAction,
    },
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
    let app = App::new();
    match cli.command {
        Command::Today { format } => report::today(&app, format.output()),
        Command::Daily {
            since,
            until,
            format,
        } => report::daily(&app, since, until, format.output()),
        Command::Models { days, format } => {
            breakdown::models(&app, days.since, days.until, format.output())
        }
        Command::Sessions { days, format } => {
            breakdown::sessions(&app, days.since, days.until, format.output())
        }
        Command::Projects { days, format } => {
            breakdown::projects(&app, days.since, days.until, format.output())
        }
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
            action: LimitsAction::Endpoint { switch, yes },
        } => limits::endpoint(&app, switch == Switch::On, yes),
    }
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
