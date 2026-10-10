# Plan limits come from the status line first, and from the usage endpoint only by opt-in

Claude Code documents plan-limit data in one place: the JSON it passes to a status line command while it runs in a terminal.
Connecting our `statusline` command is therefore the primary source of limit snapshots, and its numbers are labelled official.
Anthropic's undocumented usage endpoint fills the gap when no terminal session runs, but it needs Claude Code's login token, so it is opt-in and labelled experimental.
When enabled it only reads the token, never refreshes or writes it, sends it only to `api.anthropic.com` over HTTPS, and makes at most one request per five minutes per machine, waiting longer each time the endpoint answers that it is asked too often.

Claude Code's [legal and compliance page](https://code.claude.com/docs/en/legal-and-compliance#authentication-and-credential-use) says its OAuth login is designed for Claude Code and Anthropic's own applications, that third-party developers may not collect, store or intermediate Claude.ai credentials, and that Anthropic may enforce this without notice.
The maintainer keeps the endpoint with that risk known: the consent each person gives before turning it on states it, so everyone decides for their own account.

## Considered options

- **The endpoint alone** works without a terminal session, but it is undocumented, can change without notice, and borrows Claude Code's credentials.
- **No endpoint** keeps clear of Anthropic's terms for Claude Code's login, but leaves the limits unknown whenever no terminal session runs.
- **A Claude Code plugin** cannot set the main status line, because plugin settings only accept `agent` and `subagentStatusLine`.
