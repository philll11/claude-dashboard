# Dashboard for Claude Code

See how you use Claude Code: how close you are to your plan limits, what your usage would cost at API prices, and where it goes.

> New versions are on the [releases page](https://github.com/philll11/claude-dashboard/releases): install the package for your platform with **Extensions: Install from VSIX…**.

## What you get

- **A status bar item** with the five-hour window and when it resets, or today's API-equivalent cost before plan limits are recorded.
  Its tooltip lists every window; selecting it opens the dashboard.
- **A summary in the sidebar**: plan limits, today by model, this week, and this week's top projects.
  Each item opens the matching section of the dashboard.
- **The dashboard in an editor tab**: Overview, Sessions with each session's detail, Projects, Models, Limits and Settings.
  It follows your colour theme, high contrast included, and reopens where you left it.
- **A walkthrough** to record your plan limits, bring in older history and choose data folders.

## How it works

The extension runs `claude-dashboard`, a small program that comes with it for your platform, which reads Claude Code's transcripts into a history of its own.
Claude Code deletes its transcripts after 30 days by default; the history keeps every request it has read.
The program does the reading in the background, so VS Code's extension host stays idle.

The extension runs where Claude Code does: in a window connected to WSL, over SSH or in a container, it reads that machine's transcripts.

Every surface of claude-dashboard shares the same history: this extension, the command line and the desktop app.

## Plan limits

Claude Code passes your plan limits to its status line command while it runs in a terminal.
Connected from the dashboard's Limits section, claude-dashboard becomes that command and records each reading; if you already have a status line, it records the limits and then shows yours.
The dashboard shows the change to Claude Code's `settings.json` before making it, keeps a backup, and puts your own status line back when you disconnect.
The status line runs a copy of claude-dashboard kept with the history, so updating the extension never breaks it.

Anthropic's undocumented usage endpoint can fill the gaps when no Claude Code session runs in a terminal.
It is off unless you turn it on in Settings, labelled experimental, and reads the login Claude Code keeps on your computer without changing it.
Anthropic's terms say Claude Code's login is for Claude Code and Anthropic's own apps, so decide for yourself whether to turn it on.

## Settings

| Setting | What it does |
|---|---|
| `claudeDashboard.server.path` | The `claude-dashboard` program to run instead of the one that comes with the extension. Only your user or remote settings can set it. |

The dashboard's own Settings section holds the rest: data folders, where plan limits come from, the history, and your time zone and first day of the week.

## Privacy

- claude-dashboard reads the files Claude Code keeps on your machine and records numbers, names and titles in a local database; it never stores prompt or code content.
- There is no telemetry.
- It only uses the network for the usage endpoint, and only once you turn it on.

## Platforms

Windows, macOS and Linux (including Alpine), on x64 and arm64, with VS Code 1.126 or later, Cursor, Windsurf and VSCodium.
On any other platform, set `claudeDashboard.server.path` to a `claude-dashboard` you build or install yourself.

## Licence

[MIT](https://github.com/philll11/claude-dashboard/blob/main/LICENSE).
Dashboard for Claude Code is an independent project and is not affiliated with or endorsed by Anthropic.
