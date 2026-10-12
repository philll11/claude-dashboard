# The desktop app runs the dashboard's server in its own process

The desktop app links the server library, `crates/server`, that `claude-dashboard serve --stdio` is built on.
The dashboard in its window sends every request through one Tauri command, `request`, and receives every notification through one event, `notification`, with the same methods and messages as the JSON-RPC protocol of ADR 0006.
The library knows no transport: `serve --stdio` carries it over `lsp-server` for VS Code, and the desktop app over Tauri's IPC.

## Considered options

- **Running `claude-dashboard serve --stdio` as a child process**, as the VS Code extension does.
  The transport would be reused as it is, but the app would need a JSON-RPC client of its own, restarts after a crash, and a second process for as long as it runs.
  In the app's own process, a request is a function call, and nothing outlives the app.

## Consequences

- The app still brings the `claude-dashboard` program: Claude Code's status line runs a copy of it kept next to the history, as for VS Code, so updating or removing the app never breaks the status line.
- The server answers one request at a time off the main thread, as the stdio loop does, so a slow answer never freezes the window.
- A fault in the server's code is a fault in the app.
