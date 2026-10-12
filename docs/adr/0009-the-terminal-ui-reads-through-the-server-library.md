# The terminal UI reads through the server library

`claude-dashboard` without a command, in a terminal, runs the dashboard's server library, `crates/server`, in its own process, as the desktop app does (ADR 0008).
The interface asks it the dashboard's requests, such as `status`, `overview` and `sessions`, on a thread of its own, and the server's background work reads new transcripts and says when the history or the plan limits change.
The interface keeps no numbers of its own: it shows the server's answers, and asks again when a notification says they changed.

## Considered options

- **Querying the history directly, as the reports do.**
  The reports run once and print, so the core's queries are all they need.
  An interface that stays open needs what the server already does: reading new transcripts every minute and when asked, noticing what other processes record, such as a status line's plan limits, and giving the usage endpoint its turn.
  It would also have to build the dashboard's answers again, such as the session details, the prices and how they compare with Claude Code's own figures, and keep them the same as the desktop app's and VS Code's.

## Consequences

- Every surface shows the same numbers and states for the same history, from the same code.
- The interface draws its first frame before the server has answered anything, and a slow answer never holds up the keys, because the server answers on the data thread while another thread reads the keys.
- A fix to the server reaches the terminal UI, the desktop app and VS Code at once, as the cancellable read and the history changing during a read did.
