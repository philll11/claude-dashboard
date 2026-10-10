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

## Price table

`pricing/prices.json` holds the API prices that API-equivalent cost is calculated from, with the page they come from and the date they were last checked against it.
When Anthropic's [pricing page](https://platform.claude.com/docs/en/about-claude/pricing) changes:

1. For a new model, add an entry with the ids the Claude API gives it, without any snapshot date.
2. For a new price, add a period with `from` set to the first day, in UTC, that the price applies, after the model's current period.
   Never edit a period that is already in a release, because earlier requests keep the prices they were billed at.
3. Set `checked` to the date you compared the table with the page.
4. Update the copy of the page's table in `crates/core/tests/pricing.rs` from the page itself, not from `prices.json`, so the test catches a slip in either.
