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

1. In a pull request, set the version in the workspace `Cargo.toml`, and add a section to `CHANGELOG.md` with a heading such as `## Version 0.1.0-alpha.2 (2026-10-20)`, which becomes the release's title and notes.
   A version with a pre-release part, such as `-alpha.2`, makes a GitHub pre-release.
2. Once it is merged, tag the merge commit on `main` with `v` and the version, and push the tag.
3. The release workflow builds and publishes the release, then installs it on every operating system and runs it (`.github/workflows/install-check.yml`).
   To check an existing release again, run that workflow by hand with its tag.

## Web dashboard

`web/app` is the dashboard that VS Code and the desktop app show, and `web/protocol` describes the messages it exchanges with `claude-dashboard serve`.
The Rust types in `crates/cli/src/serve/protocol.rs` generate `web/protocol/src/generated/protocol.ts` whenever the CLI's tests run, and CI fails when that file is out of date.
The web workspace needs Node 24 and pnpm, which `corepack enable` provides.

To preview the dashboard as you change it:

```sh
pnpm install
cargo build -p claude-dashboard
pnpm --filter @claude-dashboard/app dev
```

The preview runs the real server over a generated Claude Code folder, at a fixed moment, and never reads or changes your own.
Add `?theme=vscode-light` to the address for another theme (`vscode-dark`, `vscode-high-contrast`, `vscode-high-contrast-light`, `desktop-dark` or `desktop-light`), and `&scenario=stale-limits` for another state, named in `web/app/tests/bridge/scenarios.ts`.

`pnpm lint`, `pnpm format:check`, `pnpm typecheck`, `pnpm test` and `pnpm check-licences` run the checks CI runs.
The browser tests (`pnpm --filter @claude-dashboard/app browser`) compare every section and state with a screenshot, in each of VS Code's default themes, and check each for accessibility.
Their screenshots are made in the Playwright container that CI runs them in, so that fonts and rendering match; after a change to how the dashboard looks, make them again there and look at each one before you commit it:

```sh
docker run --rm -v "$PWD":/work -w /work -e CARGO_TARGET_DIR=/work/target/linux \
  rust:1.99.0-bookworm cargo build --locked -p claude-dashboard
docker run --rm --ipc=host -v "$PWD":/work -w /work/web/app \
  -v claude-dashboard-modules:/work/node_modules \
  -v claude-dashboard-app-modules:/work/web/app/node_modules \
  -v claude-dashboard-protocol-modules:/work/web/protocol/node_modules \
  -v claude-dashboard-pnpm-store:/pnpm-store \
  -e CLAUDE_DASHBOARD_BIN=/work/target/linux/debug/claude-dashboard \
  mcr.microsoft.com/playwright:v1.63.0-noble \
  bash -c "corepack enable && pnpm install --frozen-lockfile --store-dir /pnpm-store && pnpm browser --update-snapshots=changed"
```

The volumes keep the container's Linux packages and its package store apart from your own.

## Price table

`pricing/prices.json` holds the API prices that API-equivalent cost is calculated from, with the page they come from and the date they were last checked against it.
When Anthropic's [pricing page](https://platform.claude.com/docs/en/about-claude/pricing) changes:

1. For a new model, add an entry with the ids the Claude API gives it, without any snapshot date.
2. For a new price, add a period with `from` set to the first day, in UTC, that the price applies, after the model's current period.
   Never edit a period that is already in a release, because earlier requests keep the prices they were billed at.
3. Set `checked` to the date you compared the table with the page.
4. Update the copy of the page's table in `crates/core/tests/pricing.rs` from the page itself, not from `prices.json`, so the test catches a slip in either.

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
