# One SQLite database in our own data folder, shared by every surface

The history must outlive Claude Code's transcript clean-up and look the same in every surface.
It lives in one SQLite database in the operating system's app data folder, never inside Claude Code's data root, and every surface opens it at the same time in WAL mode.
One process ingests at a time under a lock file while the others read, and every write is an idempotent upsert, so nothing is counted twice.

## Consequences

- The database records its schema version, and a binary older than that version opens it read-only and asks to be updated, because the extension, the desktop app and the CLI update independently.
- The database must stay on a local disk: SQLite's WAL mode does not work on network file systems.
- The database holds numbers, names and titles, never prompt or code content.
