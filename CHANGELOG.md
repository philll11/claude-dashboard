# Changelog

Every release of claude-dashboard, newest first.
A release's notes on GitHub come from its section here.

## Version 0.3.0 (2026-10-12)

The dashboard in the terminal, and reports by week, by month and of the plan limits.
Every surface has the same version number.
The binaries are not signed.

### What is new

- **The terminal UI**: `claude-dashboard` on its own, in a terminal, opens the dashboard there:
  - Overview, Sessions with search and each session's detail, Projects, Models and Limits, with the same numbers as VS Code and the desktop app;
  - the keyboard alone, from 80 by 24 up, on Windows, macOS and Linux and over SSH, with `?` listing the keys;
  - the range, project and model filters, and every chart as a table;
  - new transcripts read every minute, and at once with `R`, with the numbers updating in place;
  - colours for a light or a dark terminal, which it asks the terminal about or `CLAUDE_DASHBOARD_THEME` chooses, and under `NO_COLOR` none at all, with the models told apart by pattern.
- `claude-dashboard weekly` and `claude-dashboard monthly`: requests, tokens and API-equivalent cost per week and per calendar month, with `--json` and `--csv`.
- `claude-dashboard limits`: each plan window's use, reset, source and age, and the recent five-hour windows, with `--json` and `--csv`.
- The reports and `doctor` use colours made for a light background in a light terminal.
- The Models section of the terminal UI lists each model's fast-mode prices.

### Changed

- On a first run, every surface shows the newest days within seconds, while older history is still read.
- Quitting the desktop app or the terminal UI during a read takes a moment, and VS Code's server ends without being stopped by force; the next read carries on where the last one stopped.
- A transcript whose read stopped partway is read on to its end the next time.
- Projects whose folders end in the same names are told apart by as many folders before them as it takes, such as `smoke/app/workspace` and `matrix/app/workspace`.
- On Windows, the reports and `doctor` write in 24-bit colour in every console, the console host included.
- On Windows, a status line that wraps your own runs it in Git Bash, as Claude Code does, and never in WSL's `bash`.

## Version 0.2.0 (2026-10-12)

The desktop app: the dashboard in a window of its own, on Windows, macOS and Linux.
Every surface has the same version number.
The binaries are not signed: Windows SmartScreen asks once before a downloaded installer runs, and macOS once before the app opens.

### What is new

- **Dashboard for Claude Code**, the desktop app, for Windows, macOS and Linux, on x64 and arm64:
  - the dashboard's sections in a sidebar, with where the history stands at its foot, in IBM Plex, the design's typeface;
  - a window that is light or dark as the system is, keeps its size and place, and comes forward when the app is opened again;
  - updates from later releases, signed and checked before installing, either by themselves or when you choose;
  - a per-user installer for Windows, a disk image for macOS, and an AppImage and a `.deb` package for Linux, attached to this release.
- `claude-dashboard app` opens the desktop app, at a section when you name one.
- A history that a newer version has upgraded says where that version is: in VS Code, the releases page; in the desktop app, its update.

### Changed

- The project filter shows "All projects" in the dashboard's own typeface, as the design does; project names stay in the monospace one.

### Not in this release

- The terminal UI, and the desktop app's tray and menu bar item.

## Version 0.1.1 (2026-10-11)

The dashboard in VS Code, and the server behind it.
Every surface has the same version number.
The binaries are not signed.

### What is new

- **Dashboard for Claude Code**, the VS Code extension, for VS Code 1.126 or later, Cursor, Windsurf and VSCodium:
  - the five-hour window and when it resets in the status bar, or today's API-equivalent cost before plan limits are recorded, with every window in its tooltip;
  - plan limits, today by model, this week and the week's top projects in its sidebar, each opening its section of the dashboard;
  - the dashboard in an editor tab: Overview, Sessions with each session's detail, Projects, Models, Limits and Settings, in your colour theme, high contrast included;
  - a walkthrough to record plan limits, bring in claude-usage history and choose data folders;
  - a package for each platform with its `claude-dashboard` program, attached to this release: Windows, macOS, Linux and Alpine, on x64 and arm64; install one with **Extensions: Install from VSIX…** or `code --install-extension`.
- `claude-dashboard serve --stdio`, which the extension runs to read Claude Code's transcripts and answer the dashboard.
- Settings that every surface shares, kept in the history: the data folders you add, your time zone and the first day of the week.
- Each subagent's agent type, such as `Explore`, read from what Claude Code records beside its transcript.
- Cache writes priced by how long they are kept, the long-prompt price tier, and the pace of each plan window.

### Changed

- The history moves to schema 5 the first time this version opens it; 0.1.0-alpha.1 then opens it read-only and asks to be updated.
- A status line connected from VS Code runs a copy of `claude-dashboard` kept with the history, so that updates to the extension never break it.
- `statusline connect` names a program whose path has spaces: from your home folder (`~/…`) on Windows, and in quotes on macOS and Linux.

### Not in this release

- The desktop app and the terminal UI.
- Reading the usage endpoint's login from the macOS Keychain has not been tried on a Mac yet.

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
