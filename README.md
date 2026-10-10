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

## In the terminal

Until the first release, install from source with Rust 1.99 or later:

```sh
cargo install --locked --git https://github.com/philll11/claude-dashboard claude-dashboard
```

| Command | What it shows |
|---|---|
| `claude-dashboard today` | Today's requests and API-equivalent cost by model, and your plan limits |
| `claude-dashboard daily` | Requests, tokens and cost per day |
| `claude-dashboard models` | Requests and cost by model |
| `claude-dashboard sessions` | Requests and cost by session |
| `claude-dashboard projects` | Requests and cost by project |
| `claude-dashboard doctor` | What claude-dashboard can find and read, and anything that needs your attention |

`daily`, `models` and `projects` cover the last 30 days and `sessions` the last 7, unless you give `--since` and `--until` with dates such as `2026-10-05`.
Every report takes `--json` or `--csv` for scripts.
Each command first reads what Claude Code has added to its transcripts; the first time, that takes a few seconds.

### Plan limits

Claude Code passes your plan limits to its status line command while it runs in a terminal.
`claude-dashboard statusline connect` makes claude-dashboard that command, after showing you the change to Claude Code's `settings.json` and keeping a backup.
If you already have a status line, claude-dashboard records the plan limits and then shows yours.
`claude-dashboard statusline disconnect` puts back the status line you had before.

When no Claude Code session runs in a terminal, Anthropic's undocumented usage endpoint can fill the gaps.
`claude-dashboard limits endpoint on` turns it on once you agree, and `claude-dashboard limits endpoint off` turns it off.
It reads the login Claude Code keeps on your computer, never changes or refreshes it, and sends it only to `api.anthropic.com`, at most once every 5 minutes.
Requests go through the proxy that `HTTPS_PROXY` or `ALL_PROXY` names, or on Windows the system proxy, and trust the operating system's certificates.
Anthropic's terms say Claude Code's login is for Claude Code and Anthropic's own apps, and that Anthropic may act on other uses without notice, so decide for yourself whether to turn it on.

### History from claude-usage

If you used the [claude-usage](https://github.com/phuryn/claude-usage) dashboard, `claude-dashboard import claude-usage` brings in the requests from its database that the history does not have, such as those from transcripts Claude Code has since deleted.

### Where the data lives

claude-dashboard reads what Claude Code keeps in `~/.claude`, or in the folders that `CLAUDE_CONFIG_DIR` names, separated by commas.
It keeps its own history in `history.sqlite`, in `%LOCALAPPDATA%\claude-dashboard` on Windows, `~/Library/Application Support/claude-dashboard` on macOS and `~/.local/share/claude-dashboard` on Linux.
Set `CLAUDE_DASHBOARD_HISTORY` to keep it in another file, and `NO_COLOR` to turn colours off.

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
