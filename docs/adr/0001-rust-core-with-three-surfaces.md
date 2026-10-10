# A Rust core shared by the terminal, the desktop app and VS Code

The product runs on Windows, macOS and Linux in three surfaces, inside VS Code, as a desktop app and in a terminal, with nothing extra to install.
One Rust core does all the work; the terminal UI (ratatui) lives in the `claude-dashboard` binary, the desktop app is a Tauri 2 app, and the VS Code extension bundles the same binary for each platform.
The desktop app and the VS Code dashboard share one web UI.

## Considered options

- **TypeScript everywhere.**
  Native to VS Code, but the terminal and desktop surfaces would need Node.js on every machine or a bundled runtime of 60 MB or more, and heavy ingest would compete with every other extension in VS Code's extension host.
- **Python.**
  Needs an interpreter on every machine, which is exactly what Windows users tend not to have.

## Consequences

- The VS Code extension ships one package per platform with the matching binary, as rust-analyzer and Ruff do, plus a fallback package that explains an unsupported platform.
- The desktop app and the CLI are separate binaries, because a Windows GUI app cannot write to the terminal that started it.
