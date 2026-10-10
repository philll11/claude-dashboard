//! The `claude-dashboard` command.

use clap::Parser;

/// See how you use Claude Code: plan limits, usage, API-equivalent cost and where it goes.
#[derive(Debug, Parser)]
#[command(
    name = "claude-dashboard",
    version,
    about,
    arg_required_else_help = true
)]
struct Cli {}

fn main() {
    Cli::parse();
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
