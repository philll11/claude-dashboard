//! The `claude-dashboard` command.

mod doctor;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

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
    /// Check what claude-dashboard can find and read.
    Doctor,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Doctor => doctor::run(),
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
