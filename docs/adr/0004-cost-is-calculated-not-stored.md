# API-equivalent cost is calculated when queried, not stored

Prices change and a price table can be wrong, so the history stores token counts by type, and API-equivalent cost is calculated when it is queried from a dated price table kept in this repository.
Correcting the price table therefore corrects the whole history.
Each session's API-equivalent cost is reconciled with its reported session cost, so a stale or missing price shows up as a warning instead of a silently wrong number, and a model missing from the table is unpriced, never free.
