# VS Code talks to the bundled binary with JSON-RPC over stdio

The VS Code extension starts the bundled binary with `claude-dashboard serve --stdio` and exchanges JSON-RPC messages with it, framed like the Language Server Protocol, instead of running a local web server.
There are no ports to allocate, no firewall prompts, and nothing another local process can connect to.
The Rust types are the single source of truth for this API, and the TypeScript types used by the extension and the web UI are generated from them.

## Considered options

- **A web server on 127.0.0.1** needs a free port, can clash with other software, and exposes usage data to any local process.
