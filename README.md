# claude-dashboard

See how you use Claude Code: what it would cost at API prices, where the usage goes, and how close you are to your plan limits.
It runs inside VS Code, as a desktop app and in the terminal, on Windows, macOS and Linux, with nothing else to install.

> It is made for personal use, and installed from the [releases](https://github.com/philll11/claude-dashboard/releases) rather than a marketplace.
> What each version keeps stable is under [Versions](#versions).

## What it is for

- **Plan limits at a glance**: how much of the five-hour and weekly windows you have used, and when they reset.
- **Usage and cost**: tokens and API-equivalent cost today, this week and this month, by model.
- **Where it goes**: projects, sessions, subagents, skills and MCP servers, and how well the prompt cache works for you.
- **History that lasts**: Claude Code deletes its transcripts after 30 days by default; claude-dashboard keeps what it has read.
- **Your data, your way**: CSV and JSON export, and terminal reports you can script.

## Where it runs

| Surface | What you get |
|---|---|
| VS Code | A status bar item, a sidebar summary, and the full dashboard in the panel beside the terminal and in an editor tab. Works over WSL and SSH. |
| Desktop | The full dashboard in its own window on Windows, macOS and Linux. |
| Terminal | An interactive terminal UI, plus reports with `--json` and `--csv` output. |

## In VS Code

Install **Dashboard for Claude Code** from the [latest release](https://github.com/philll11/claude-dashboard/releases/latest): download the package for your platform, such as `claude-dashboard-win32-x64-<version>.vsix` on Windows, and install it with **Extensions: Install from VSIX…** or `code --install-extension <package>`.
The platforms are `win32`, `darwin`, `linux` and `alpine`, each `x64` or `arm64`; Cursor, Windsurf and VSCodium install the same packages.
It comes with the `claude-dashboard` program for your platform, so nothing else needs installing.
To update, install the package from a newer release.

- The status bar shows the five-hour window and when it resets, or today's API-equivalent cost before plan limits are recorded.
- Its sidebar lists plan limits, today by model, this week and the week's top projects.
- The **Usage dashboard** tab in the panel, beside the terminal, holds the whole dashboard under a strip that keeps the five-hour and weekly windows and the latest session in the project you have open in sight, whichever section you look at.
  Drag it beside the terminal or into a sidebar, and it fits itself to the room.
- The dashboard also opens in an editor tab, in your colour theme: Overview, Sessions, Projects, Models, Limits and Settings.
- Its walkthrough records your plan limits, brings in claude-usage history and chooses data folders.

In a window connected to WSL, over SSH or in a container, it runs on that machine and reads its Claude Code folder.
Install it there from that window, with **Extensions: Install from VSIX…** and the package for that machine, such as `claude-dashboard-linux-x64-<version>.vsix` for WSL on an x64 computer: an extension installed in a local window does not run in remote ones until it is installed there too.
`claudeDashboard.server.path` runs another `claude-dashboard` instead of the one that comes with the extension, such as a build of your own.

## On the desktop

Install **Dashboard for Claude Code** from the [latest release](https://github.com/philll11/claude-dashboard/releases/latest) with the installer for your system, such as `claude-dashboard-desktop-<version>-windows-x64-setup.exe`:

- Windows: the `-setup.exe`, which installs for you alone and needs no administrator rights.
- macOS: the `.dmg`.
- Linux: the `.AppImage`, or the `.deb` package.

Each comes for x64 and arm64.
It installs updates from later releases by itself, signed and checked before installing; Settings can leave that to you instead.

The installers are not signed.
Windows SmartScreen asks once before the downloaded installer runs: choose **More info**, then **Run anyway**.
macOS asks once before the app opens: in **System Settings**, **Privacy & Security**, choose **Open Anyway**.

- The sidebar holds the sections: Overview, Sessions, Projects, Models, Limits and Settings.
- The window is light or dark as your system is, and remembers its size and place.
- Its item in the notification area, or the menu bar on macOS, shows how much of the five-hour window you have used, and its menu lists every plan window and today's API-equivalent cost.
  On Windows and macOS, closing the window keeps the app running there, and **Quit** in the item's menu ends it; on Linux, closing the window quits unless you switch keeping it running on in Settings.
  Settings can also hide the item and open the app at login, in the item without the window.
- Each section's **Export** button saves its table as CSV or JSON, and Settings saves the whole history.
- `claude-dashboard app` opens it from a terminal, and `claude-dashboard app limits` at a section.

It comes with the `claude-dashboard` program, which Claude Code's status line runs a copy of once you connect it, so updating or removing the app never breaks the status line.
If something goes wrong, the app's log is in `%LOCALAPPDATA%\io.github.philll11.claude-dashboard\logs` on Windows, `~/Library/Logs/io.github.philll11.claude-dashboard` on macOS and `~/.local/share/io.github.philll11.claude-dashboard/logs` on Linux.

## In the terminal

Install it with the installer for your system, and run the installer again to update.
On macOS and Linux:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/philll11/claude-dashboard/releases/latest/download/claude-dashboard-installer.sh | sh
```

On Windows:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/philll11/claude-dashboard/releases/latest/download/claude-dashboard-installer.ps1 | iex"
```

Both installers also put `cld` beside it: a short name for the same program, which takes every command and option, so `cld` opens the dashboard and `cld today` prints today's report.
The binaries are not signed.
Installed this way they carry no browser download mark, so Windows SmartScreen and macOS Gatekeeper do not warn about them; Windows Smart App Control, where it is on, blocks unsigned programs however they arrive.
[Releases](https://github.com/philll11/claude-dashboard/releases) lists every version with its notes and checksums.
To build from source instead, with Rust 1.99 or later, which installs it as `claude-dashboard` alone:

```sh
cargo install --locked --git https://github.com/philll11/claude-dashboard claude-dashboard
```

### The dashboard in the terminal

`claude-dashboard` on its own, or `cld`, opens the dashboard in the terminal: Overview, Sessions with each session's detail, Projects, Models and Limits.
It works from the keyboard alone, in a terminal of 80 by 24 or larger:

| Keys | What they do |
|---|---|
| `←` `→`, `Tab`, `1` to `5` | Change the section |
| `↑` `↓` or `j` `k`, `PgUp` `PgDn`, `Home` `End` | Move in a list, or scroll a page |
| `Enter` | Open a session, or filter the dashboard to a project |
| `Esc` | Go back, or close |
| `/` | Search the sessions |
| `r`, `p`, `m` | Choose the range, the project and the model |
| `t` | Show the charts as tables |
| `e` | Export the section shown, as CSV or JSON, to a file in your Downloads folder unless you change it |
| `R` | Read new transcripts now |
| `?` | Show the keys |
| `q`, `Ctrl-C` | Quit |

It reads what Claude Code adds to its transcripts every minute and updates in place.
Its colours suit your terminal's light or dark background, which it asks the terminal about; `CLAUDE_DASHBOARD_THEME=light` or `dark` chooses instead.
`NO_COLOR` turns colour off, and the charts then tell the models apart by pattern.
In Git Bash's own window, run it in Windows Terminal or through `winpty claude-dashboard`.
In the Windows console host and the Linux console, whose fonts lack the finer block characters, sparklines step in half cells.
For a screen reader, the reports below print plain lines.

### Reports

| Command | What it shows |
|---|---|
| `claude-dashboard today` | Today's requests and API-equivalent cost by model, and your plan limits |
| `claude-dashboard daily` | Requests, tokens and cost per day |
| `claude-dashboard weekly` | Requests, tokens and cost per week |
| `claude-dashboard monthly` | Requests, tokens and cost per calendar month |
| `claude-dashboard models` | Requests and cost by model |
| `claude-dashboard sessions` | Requests and cost by session |
| `claude-dashboard projects` | Requests and cost by project |
| `claude-dashboard limits` | Each plan window's use, reset and source, and the recent five-hour windows |
| `claude-dashboard export` | Every request, or those of the days given, as CSV or JSON |
| `claude-dashboard prices` | Which price table is in use, and the daily check for a newer one |
| `claude-dashboard doctor` | What claude-dashboard can find and read, and anything that needs your attention |
| `claude-dashboard app` | Opens the desktop app, at a section when you name one |

`daily`, `models` and `projects` cover the last 30 days and `sessions` the last 7, unless you give `--since` and `--until` with dates such as `2026-10-05`.
`weekly` and `monthly` cover the last 12 weeks and months, and with `--since` and `--until` every week or month those days fall in, whole; weeks start on the day the dashboard's settings name, Monday unless you choose another.
Every report takes `--json` or `--csv` for scripts.
Each command first reads what Claude Code has added to its transcripts; the first time, that takes a few seconds.

### Export

`claude-dashboard export` writes one row per request, with its session, project, model, tokens, tools and API-equivalent cost: as CSV unless you give `--json`, for every day unless you give `--since` and `--until`.
It writes to standard output for scripts, or with `--output requests.csv` to a file made for spreadsheets: a CSV file starts with UTF-8's byte order mark, so that Excel reads it as UTF-8, and text that a spreadsheet would take for a formula, such as a session title starting with `=`, gets an apostrophe before it.
Use `--output` in Windows PowerShell 5.1, whose `>` writes another encoding.
The dashboard's exports are the same files.
Days are written in your time zone, moments in UTC, and costs in US dollars without a symbol.

### Prices

API-equivalent cost comes from the price table in this repository's `pricing/prices.json`, which each release brings.
`claude-dashboard prices refresh on`, or the switch in the dashboard's Settings, has a running dashboard check the repository for a newer table once a day, and every surface prices with it; `claude-dashboard prices refresh off` stops the checks.
`claude-dashboard prices update` checks once, at once, and `claude-dashboard prices` says which table is in use and how the last check went.
A newer table is used only when it keeps every price this version has, adding only new models and prices.

### Plan limits

Claude Code passes your plan limits to its status line command while it runs in a terminal.
`claude-dashboard statusline connect` makes claude-dashboard that command, after showing you the change to Claude Code's `settings.json` and keeping a backup.
Its line starts with the session's title, which tells apart sessions running side by side, then shows the model, how much of each plan window is used and today's API-equivalent cost: `Fix the importer · Opus 5.5 · 5h 42% · week 71% · $5.05 today`.
If you already have a status line, claude-dashboard records the plan limits and then shows yours.
`claude-dashboard statusline disconnect` puts back the status line you had before.

When no Claude Code session runs in a terminal, Anthropic's undocumented usage endpoint can fill the gaps.
`claude-dashboard limits endpoint on` turns it on once you agree, and `claude-dashboard limits endpoint off` turns it off.
It reads the login Claude Code keeps on your computer, never changes or refreshes it, and sends it only to `api.anthropic.com`, at most once every 5 minutes.
Requests go through the proxy that `ALL_PROXY`, `HTTPS_PROXY` or `HTTP_PROXY` names unless `NO_PROXY` excludes the host, or on Windows the system proxy, and trust the operating system's certificates.
Anthropic's terms say Claude Code's login is for Claude Code and Anthropic's own apps, and that Anthropic may act on other uses without notice, so decide for yourself whether to turn it on.

### History from claude-usage

If you used the [claude-usage](https://github.com/phuryn/claude-usage) dashboard, `claude-dashboard import claude-usage` brings in the requests from its database that the history does not have, such as those from transcripts Claude Code has since deleted.

### Where the data lives

claude-dashboard reads what Claude Code keeps in `~/.claude`, or in the folders that `CLAUDE_CONFIG_DIR` names, separated by commas.
It keeps its own history in `history.sqlite`, in `%LOCALAPPDATA%\claude-dashboard` on Windows, `~/Library/Application Support/claude-dashboard` on macOS and `~/.local/share/claude-dashboard` on Linux.
Set `CLAUDE_DASHBOARD_HISTORY` to keep it in another file, `NO_COLOR` to turn colours off, and `CLAUDE_DASHBOARD_THEME` to `light` or `dark` to choose colours for your terminal's background.

## Privacy

- claude-dashboard reads the files Claude Code keeps on your machine and records numbers, names and titles in a local database; it never stores prompt or code content.
- There is no telemetry.
- It only uses the network for features you switch on: plan limits from Anthropic's undocumented usage endpoint (labelled experimental), update checks for the desktop app, and the daily check for a newer price table in this repository.

## How it works

- [`CONTEXT.md`](CONTEXT.md) defines the words used throughout the project.
- [`docs/adr`](docs/adr) records the architecture decisions and why they were made.

## Versions

Every surface has the same version, and the [changelog](CHANGELOG.md) lists what each release changes.
Within version 1:

- a later version reads the history an earlier one kept, and an earlier version reads a later one's until the later one changes something it cannot read, when it says to update;
- reports keep their JSON fields and CSV columns, adding new ones at the end;
- commands and their options stay as they are.

## Contributing and security

See [`CONTRIBUTING.md`](CONTRIBUTING.md) to get involved, and [`SECURITY.md`](SECURITY.md) to report a vulnerability privately.

## Licence

[MIT](LICENSE).

claude-dashboard is an independent project and is not affiliated with or endorsed by Anthropic.
