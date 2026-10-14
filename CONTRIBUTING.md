# Contributing

Thank you for helping.
Start with an issue, so we can agree on the change before you spend time on it.

## Workflow

1. Branch from an up-to-date `main`; use one branch per change.
2. Open a pull request that links its issue and explains what changed and why.
3. Every check must pass on Windows, macOS and Linux.
4. The maintainer reviews and squash-merges; the branch is deleted on merge.

## Style

- Words: use the terms defined in [`CONTEXT.md`](CONTEXT.md), and avoid the alternatives it lists.
- Decisions that are hard to reverse get a short record in [`docs/adr`](docs/adr).
- Documentation and UI text use British English.
- In Markdown, each sentence goes on its own line, so diffs stay readable.
- Commit messages are short, plain sentences that say what changed.
- Comments explain constraints a future maintainer needs, not the history of how the code came about.

## Release workflow

[dist](https://github.com/axodotdev/cargo-dist) generates `.github/workflows/release.yml` from `dist-workspace.toml`.
Release settings, and the versions of the actions the release uses, change in `dist-workspace.toml`, never in `release.yml`.
After changing it:

1. Install the dist version that `cargo-dist-version` names, with `cargo-dist-installer.sh` or `cargo-dist-installer.ps1` from that version's [release](https://github.com/axodotdev/cargo-dist/releases).
2. Run `dist generate`, and commit both files.

The release workflow's plan job fails when `release.yml` differs from what dist generates.

To cut a release:

1. In a pull request, set the version in the workspace `Cargo.toml` and in `apps/vscode/package.json`, and add a section to `CHANGELOG.md` with a heading such as `## Version 1.0.1 (2026-11-02)`, which becomes the release's title and notes.
   Every surface has the same version, as plain `major.minor.patch`, and each release becomes the latest release on GitHub, which the README's install links read.
   A suffix such as `-alpha.1` would make dist publish a pre-release instead, which those links skip.
2. Once it is merged, tag the merge commit on `main` with `v` and the version, and push the tag.
3. The release workflow makes the release as a draft (`.github/workflows/release-draft.yml`) and builds the programs, then:
   - packages the VS Code extension for every platform with them and attaches the packages to the draft (`.github/workflows/vscode-release.yml`);
   - builds the desktop app for every platform around them, signs its update bundles, and attaches its installers, the update bundles and `latest.json`, which the app's updater reads (`.github/workflows/desktop-release.yml`);
   - attaches the programs and publishes the release, only once every job before has succeeded ([ADR 0012](docs/adr/0012-releases-go-public-complete.md)).
4. Once it is published:
   - the install check installs its programs on every operating system and runs them (`.github/workflows/install-check.yml`);
   - the release check verifies the attestation of every file it publishes, and runs the end-to-end tests on its own desktop installers and VS Code packages (`.github/workflows/release-check.yml`).

   To check a published release again, run the install check or the release check by hand with its tag.
   A release that fails before it is published stays a draft; the runbook says what to do then.

## Terminal UI

`crates/cli/src/tui` is the dashboard in the terminal, which `cargo run -p claude-dashboard` opens.
It reads your own Claude Code folder and history, as an installed copy would; set `CLAUDE_DASHBOARD_HISTORY` to a file of its own to keep your history out of it.
The design's terminal boards on the canvas are what each screen is drawn to match.

- The snapshot tests draw every screen and state from made-up answers, through crossterm into memory, read the result back as a terminal shows it, and compare its text and colours with the snapshots in `crates/cli/src/tui/tests/snapshots`.
  After a change that is meant to show, run the tests with `INSTA_UPDATE=always`, or `cargo insta review` with [cargo-insta](https://insta.rs/docs/cli/), and read the changed snapshots before committing them.
- `crates/cli/tests/tui.rs` runs the program in a pseudo-terminal, Windows' pseudo console on Windows, and moves through its screens as a person would.
- `design/tokens.json` holds the terminal palettes, for a dark and a light background; the colour tests hold their text to 4.5:1 and their marks to 3:1, on white, GNOME Terminal's and Solarized Light's backgrounds as well as the dark one.
- `design/formats.json` and `design/buckets.json` hold the examples that the Rust and the TypeScript formats, weeks and months are both tested against, so that every surface writes them the same.
- `design/glance.json` holds what the desktop app's item and VS Code's status bar say, which the server library's and the extension's tests both check.

## Web dashboard

`web/app` is the dashboard that VS Code and the desktop app show, and `web/protocol` describes the messages it exchanges with `claude-dashboard serve`.
The Rust types in `crates/server/src/protocol.rs` generate `web/protocol/src/generated/protocol.ts` whenever the server library's tests run, and CI fails when that file is out of date.
The web workspace needs Node 24 and pnpm, which `corepack enable` provides.

To preview the dashboard as you change it:

```sh
pnpm install
cargo build -p claude-dashboard
pnpm --filter @claude-dashboard/app dev
```

The preview runs the real server over a generated Claude Code folder, at a fixed moment, and never reads or changes your own.
Add `?theme=vscode-light` to the address for another theme (`vscode-dark`, `vscode-high-contrast`, `vscode-high-contrast-light`, `desktop-dark` or `desktop-light`), and `&scenario=stale-limits` for another state, named in `web/app/tests/bridge/scenarios.ts`.
Add `&placement=view&folder=/home/you/code/claude-dashboard` for VS Code's view in the panel, with that project's folder open.

`pnpm lint`, `pnpm format:check`, `pnpm typecheck`, `pnpm test` and `pnpm check-licences` run the checks CI runs.
The browser tests (`pnpm --filter @claude-dashboard/app browser`) compare every section and state with a screenshot, in each of VS Code's default themes and the desktop app's two, and check each for accessibility.
Their screenshots are made in the Playwright container that CI runs them in, so that fonts and rendering match; after a change to how the dashboard looks, make them again there and look at each one before you commit it:

```sh
docker run --rm -v "$PWD":/work -w /work -e CARGO_TARGET_DIR=/work/target/linux \
  rust:1.99.0-bookworm cargo build --locked -p claude-dashboard
docker run --rm --ipc=host -v "$PWD":/work -w /work/web/app \
  -v claude-dashboard-modules:/work/node_modules \
  -v claude-dashboard-app-modules:/work/web/app/node_modules \
  -v claude-dashboard-protocol-modules:/work/web/protocol/node_modules \
  -v claude-dashboard-vscode-modules:/work/apps/vscode/node_modules \
  -v claude-dashboard-desktop-modules:/work/apps/desktop/node_modules \
  -v claude-dashboard-pnpm-store:/pnpm-store \
  -e CLAUDE_DASHBOARD_BIN=/work/target/linux/debug/claude-dashboard \
  mcr.microsoft.com/playwright:v1.63.0-noble \
  bash -c "corepack enable && pnpm install --frozen-lockfile --store-dir /pnpm-store && pnpm browser --update-snapshots=changed"
```

The volumes keep the container's Linux packages and its package store apart from your own.

## VS Code extension

`apps/vscode` is the extension: it runs `claude-dashboard serve`, shows a summary in the status bar and the sidebar, and the dashboard from `web/app` in its view in the panel and in an editor tab.
Its package carries the program for one platform, in `server/`, and runs it with `--status-line-copy`, so that a status line connected from it runs a copy of the program that the next update cannot move.

To try your changes, build everything, point the extension at your build in your user settings (`"claudeDashboard.server.path": "<repository>/target/debug/claude-dashboard"`), and start VS Code with the extension:

```sh
cargo build -p claude-dashboard
pnpm install
pnpm build
code --extensionDevelopmentPath="$PWD/apps/vscode"
```

It reads your own Claude Code folder and history, as an installed copy would.

The tests:

- `pnpm --filter claude-dashboard test` checks what the status bar and the sidebar show, with Vitest.
- `pnpm --filter claude-dashboard test:integration` runs the extension in VS Code over the browser tests' generated Claude Code folder.
  It downloads the latest VS Code; set `VSCODE_VERSION=1.126.0` for the oldest the extension supports, or `VSCODE_PATH` to the program of a VS Code you have installed.
- The end-to-end tests install a package of the extension into VS Code 1.137, the newest that ExTester drives, open the project whose session is running, look at the status bar, the sidebar and inside the dashboard in its tab and in the panel, and export the sessions it lists, leaving screenshots of the tab and the panel in each default theme in `screenshots` in the folder `E2E_STORAGE` names, or in `claude-dashboard-e2e` in your system's temporary folder:

  ```sh
  node apps/vscode/scripts/package.mjs --target win32-x64 --program target/debug/claude-dashboard.exe --out apps/vscode/claude-dashboard.vsix
  node apps/vscode/scripts/e2e.mjs --vsix apps/vscode/claude-dashboard.vsix
  ```

  Name your platform's target, such as `linux-x64` or `darwin-arm64`, and its program.

On Linux, run the integration and end-to-end tests under `xvfb-run -a`, which gives VS Code a display.

## Desktop app

`apps/desktop` is the desktop app: a Tauri 2 window with the dashboard from `web/app`, answered by the server library in the app's own process ([ADR 0008](docs/adr/0008-the-desktop-app-runs-the-server-in-process.md)).
It brings the `claude-dashboard` program next to its own, and a status line connected from the app runs a copy of that program kept next to the history.
On Linux it builds against WebKitGTK; on Debian and Ubuntu, install `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`.
Its item in the notification area loads the AppIndicator library when the app starts, and the app runs without the item where there is none.
The item's icons are drawn in code, and `cargo test -p claude-dashboard-desktop` compares each level and size with its text snapshot in `apps/desktop/src-tauri/src/snapshots`.

To try your changes, with the dashboard reloading as you change it:

```sh
pnpm install
pnpm --filter @claude-dashboard/desktop tauri dev
```

It reads your own Claude Code folder and history, as an installed copy would; set `CLAUDE_DASHBOARD_HISTORY` to a file of its own to keep your history out of it.

To build the installers for your platform, name its target and a build of the program the app brings:

```sh
cargo build --release -p claude-dashboard
node apps/desktop/scripts/bundle.mjs --target x86_64-pc-windows-msvc --bundles nsis --program target/release/claude-dashboard.exe
```

They land in `apps/desktop/out/bundles`, named as the release names them.

The tests:

- The end-to-end tests install the app from its installer into a folder of their own, drive it with [tauri-driver](https://v2.tauri.app/develop/tests/webdriver/) over the browser tests' generated Claude Code folder, export the sessions it lists, and uninstall it again, leaving nothing behind but a screenshot of each section in `apps/desktop/out/screenshots`.
  Install the driver with `cargo install tauri-driver --locked --version 2.1.0`, and close the app if you have it open, quitting it from its item, since a second copy hands over to the first.
  They stop where the app is installed already, since a test copy would replace its entry in Apps & features, its shortcut and its folder, or on Linux its package: uninstall it first, or test on another machine.
  The tests switch keeping the app running off, because they end it by closing its window, and one test on Windows switches it on to close the window and bring it back.
  The native save dialog cannot be driven, so `CLAUDE_DASHBOARD_EXPORT_TO`, which only tests set, names a folder that exports go to without asking; the VS Code extension honours it too, and its end-to-end tests set it.
  Each suite checks that the saved file is what `claude-dashboard sessions` reports for the same days.
  On Windows the tests download the Microsoft Edge WebDriver that matches your WebView2 runtime.
  On Linux they install the `.deb` package with `sudo` and need `webkit2gtk-driver`; without a desktop, run them under `xvfb-run -a dbus-run-session --`, which gives them a display and the D-Bus session bus a desktop has.

  ```sh
  node apps/desktop/scripts/e2e.mjs --installer apps/desktop/out/bundles/claude-dashboard-desktop-<version>-windows-x64-setup.exe
  ```

- Tauri's driver has no macOS support, so there `node apps/desktop/scripts/start.mjs --target aarch64-apple-darwin` starts the built app on the generated folder and checks that it reads the transcripts and shows its window once the dashboard has drawn.

The icons come from the extension's (`apps/vscode/media/icon.svg`), and macOS's from `apps/desktop/icon-macos.svg`, which draws the same icon on Apple's icon grid.
After changing either, from `apps/desktop`, run `pnpm exec tauri icon ../vscode/media/icon.svg -o src-tauri/icons` and remove what it writes for Android, iOS and the Microsoft Store; then run `pnpm exec tauri icon icon-macos.svg -o <a folder of your own>` and copy its `icon.icns` over `src-tauri/icons/icon.icns`.

## Price table

`pricing/prices.json` holds the API prices that API-equivalent cost is calculated from, with the page they come from and the date they were last checked against it.
When Anthropic's [pricing page](https://platform.claude.com/docs/en/about-claude/pricing) changes:

1. For a new model, add an entry with the ids the Claude API gives it, without any snapshot date.
2. For a new price, add a period with `from` set to the first day, in UTC, that the price applies, after the model's current period.
   Never edit a period that is already in a release, because earlier requests keep the prices they were billed at.
3. Set `checked` to the date you compared the table with the page.
4. Update the copy of the page's table in `crates/core/tests/pricing.rs` from the page itself, not from `prices.json`, so the test catches a slip in either.

The table on `main` reaches every installed version whose daily price check is on ([ADR 0011](docs/adr/0011-newer-prices-come-from-this-repository.md)), so a change merged there is a change to everyone's costs.
Those versions use it only if it keeps every model and period their own table has and was checked no earlier, so a correction to a period that is in a release reaches people only with a new release.
`format` names the table's shape: raise it only for a change that an older version would misread, such as a new field, and that version then says it needs updating rather than using the table.
The export goldens in `crates/cli/tests/goldens` carry the table's date and prices; `UPDATE_GOLDENS=1 cargo test -p claude-dashboard --test export` writes them again, and Git keeps them byte for byte.

## Performance

The core library has [criterion](https://github.com/criterion-rs/criterion.rs) benchmarks for reading transcripts, ingesting them and reporting from the history, on made-up transcripts shaped like Claude Code's.
CI runs each benchmark once, as a test, so that they keep working; timings come from your own machine:

```sh
cargo bench -p claude-dashboard-core
```

Set `CLAUDE_DASHBOARD_BENCH_ROOT` to a data root, such as `~/.claude`, to time ingesting its real transcripts as well.
The benchmarks only read that root, and write to temporary histories.
On a typical laptop, the targets for real data are:

| Measure | Target |
|---|---|
| First ingest of about 3 GB of transcripts | under 15 seconds |
| First ingest of the last 7 days, which a first ingest reads first | under 3 seconds |
| Ingest that finds nothing new | under 200 milliseconds |
| Peak memory during a first ingest, whatever the size of the history | under 64 MB |

Peak memory is that of `claude-dashboard today` with `CLAUDE_DASHBOARD_HISTORY` set to a new file, as `/usr/bin/time -v` reports it on Linux, `/usr/bin/time -l` on macOS, or a process's peak working set on Windows.
A pull request that slows a benchmark down says by how much, and why it is worth it.
