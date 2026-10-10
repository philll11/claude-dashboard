# claude-dashboard

See how you use Claude Code: what it would cost at API prices, where the usage goes, and how close you are to your plan limits.
It runs inside VS Code, as a desktop app and in the terminal, on Windows, macOS and Linux, with nothing else to install.

> **Status: early development.**
> Nothing is released yet.
> Progress is tracked in the [milestones](https://github.com/philll11/claude-dashboard/milestones).

## What it is for

- **Plan limits at a glance**: how much of the five-hour and weekly windows you have used, and when they reset.
- **Usage and cost**: tokens and API-equivalent cost today, this week and this month, by model.
- **Where it goes**: projects, sessions, subagents, skills and MCP servers, and how well the prompt cache works for you.
- **History that lasts**: Claude Code deletes its transcripts after 30 days by default; claude-dashboard keeps what it has read.
- **Your data, your way**: CSV and JSON export, and terminal reports you can script.

## Where it runs

| Surface | What you get |
|---|---|
| VS Code | A status bar item, a sidebar summary and the full dashboard in an editor tab. Works over WSL and SSH. |
| Desktop | The full dashboard in its own window on Windows, macOS and Linux. |
| Terminal | An interactive terminal UI, plus reports with `--json` and `--csv` output. |

## Privacy

- claude-dashboard reads the files Claude Code keeps on your machine and records numbers, names and titles in a local database; it never stores prompt or code content.
- There is no telemetry.
- It only uses the network for features you switch on: plan limits from Anthropic's undocumented usage endpoint (labelled experimental), update checks for the desktop app, and price-table refreshes.

## How it works

- [`CONTEXT.md`](CONTEXT.md) defines the words used throughout the project.
- [`docs/adr`](docs/adr) records the architecture decisions and why they were made.

## Contributing and security

See [`CONTRIBUTING.md`](CONTRIBUTING.md) to get involved, and [`SECURITY.md`](SECURITY.md) to report a vulnerability privately.

## Licence

[MIT](LICENSE).

claude-dashboard is an independent project and is not affiliated with or endorsed by Anthropic.
