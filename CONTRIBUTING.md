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
