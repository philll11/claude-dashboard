# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub: [open a security advisory](https://github.com/philll11/claude-dashboard/security/advisories/new).
Do not open a public issue for a security problem.

Include what you found, how to reproduce it, and which version and operating system you used.
You will get a reply as soon as possible, and credit in the release notes if you would like it.

## Supported versions

Security fixes go into the latest release only.

## Areas that matter most

- Handling of Claude Code's login token by the opt-in plan-limits feature.
- The contents and location of the local database.
- The content security policy of the VS Code dashboard and the desktop app.
- Signing and verification of desktop app updates.
- The export writer, which writes files where the person chooses, and keeps text from transcripts from being read as spreadsheet formulas.
- The price-table refresh, whose download changes every API-equivalent cost.
- Opening the desktop app at login, which leaves a command that the system runs each time the person logs in.
