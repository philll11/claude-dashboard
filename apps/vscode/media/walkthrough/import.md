## History from claude-usage

Claude Code deletes its transcripts after 30 days unless you change `cleanupPeriodDays`.
The claude-usage dashboard kept its own copy of what it read, including requests whose transcripts are gone.

Bringing them in adds only the requests your history does not have, and leaves claude-usage's database as it is.
