# Plan limits come from the status line first, and from the usage endpoint only by opt-in

Claude Code documents plan-limit data in one place: the JSON it passes to a status line command while it runs in a terminal.
Connecting our `statusline` command is therefore the primary source of limit snapshots, and its numbers are labelled official.
Anthropic's undocumented usage endpoint fills the gap when no terminal session runs, but it needs Claude Code's login token, so it is opt-in and labelled experimental.
When enabled it only reads the token, never refreshes or writes it, and makes at most one request per five minutes per machine.

## Considered options

- **The endpoint alone** works without a terminal session, but it is undocumented, can change without notice, and borrows Claude Code's credentials.
- **A Claude Code plugin** cannot set the main status line, because plugin settings only accept `agent` and `subagentStatusLine`.
