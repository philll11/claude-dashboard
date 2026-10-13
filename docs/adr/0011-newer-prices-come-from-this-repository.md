# Newer prices come from this repository's price table

When the person switches the price-table refresh on, the server checks `pricing/prices.json` on this repository's `main` branch, from raw.githubusercontent.com, at most once a day per machine.
Every surface then prices with that table when it is newer than the one the version ships with.

A downloaded table is used only when:

- this version reads its `format`, which the table names first and which changes only when a change to the table's shape would make an older version misread it;
- it parses under the same rules as the bundled table;
- it was checked against Anthropic's pricing page no earlier than the bundled table;
- it keeps every model and price period the bundled table has, adding only new ones.

The history keeps the table, its entity tag and when it was last checked in the settings every surface shares, so every surface prices the same and the machine checks once a day whichever surface runs.

## Considered options

- **LiteLLM's or models.dev's catalogues**, which other usage tools fetch.
  They cover many providers, but only in part what ADR 0004 prices by: cache writes by their duration, each model's own cache-read multiplier, Haiku 5.5's long-prompt tier and fast mode.
  The price check that holds the estimate to Claude Code's own figures would also check someone else's table.
- **Anthropic's Models API**, which lists models without prices.
- **Signed price tables**, as the desktop app's updates are.
  A table changes figures, never code, and comes over HTTPS from the same repository and account as the app, so a second signing key would add a secret to keep without protecting anything new.

## Consequences

- `main` is how new prices reach installed versions, so a change to `pricing/prices.json` keeps every period a release has, as CONTRIBUTING says.
- A table in a newer format says it needs a newer claude-dashboard, rather than failing as a broken file.
- Only the server's background work and `claude-dashboard prices update` reach the network for prices; reports, exports and the status line read the stored table.
- The refresh is off until switched on, as the product brief asks of every use of the network.
