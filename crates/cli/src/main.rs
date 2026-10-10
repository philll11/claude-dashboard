//! The `claude-dashboard` command.

mod app;
mod doctor;
mod fmt;
mod import;
mod report;
mod statusline;
mod style;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
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
        Command::Doctor => doctor::run(&app),
        Command::Statusline { action, then } => match action {
            None => statusline::run(&app, &then),
            Some(StatuslineAction::Connect { yes }) => statusline::connect(&app, yes),
            Some(StatuslineAction::Disconnect { yes }) => statusline::disconnect(&app, yes),
        },
        Command::Import {
            source: ImportSource::ClaudeUsage { database },
        } => import::claude_usage(&app, database),
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
