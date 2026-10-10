# Changelog

Every release of claude-dashboard, newest first.
A release's notes on GitHub come from its section here.

## Version 0.1.0-alpha.1 (2026-10-11)

The first pre-release: the `claude-dashboard` command for Windows, macOS and Linux, on x64 and arm64.
The binaries are not signed yet.
The shell and PowerShell installers below download them without the mark a browser adds, so Windows SmartScreen and macOS Gatekeeper do not warn about them; an archive downloaded in a browser gets that warning.
Windows Smart App Control, where it is on, blocks unsigned programs however they arrive.

### What it does

- Reads Claude Code's transcripts into a history of its own, which keeps every request after Claude Code deletes its transcripts; a first read of 3 GB takes a few seconds.
- Reports requests, tokens and API-equivalent cost by day, model, session and project: `today`, `daily`, `models`, `sessions` and `projects`, each with `--json` and `--csv`.
- Prices every request from a dated table of Anthropic's API prices, and checks the result against the costs Claude Code reports for each session.
- Records your plan limits from Claude Code's status line, once `claude-dashboard statusline connect` makes it the status line command.
- Fills the gaps from Anthropic's undocumented usage endpoint once `claude-dashboard limits endpoint on` turns it on; it is experimental, and what it asks you to agree to includes what Anthropic's terms say about Claude Code's login.
- Brings in the history of the claude-usage dashboard with `claude-dashboard import claude-usage`.
- `claude-dashboard doctor` checks what it can find and read.

### Checked on real data

- Daily tokens match ccusage 20.0.24 exactly on 25 of the last 29 days.
  On the other four, a few thousand tokens differ: a model fallback and a response cut off mid-stream, whose tokens claude-dashboard counts and ccusage does not, and a response streamed across midnight, which claude-dashboard counts on the day it started.
- API-equivalent costs agree with Claude Code's own session costs within 0.2%.
- The days brought in from claude-usage match its database exactly.

### Not in this release

- The desktop app, the VS Code extension and the terminal UI.
- Reading the usage endpoint's login from the macOS Keychain has not been tried on a Mac yet.
