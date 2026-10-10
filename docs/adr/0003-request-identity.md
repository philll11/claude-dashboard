# How a request is identified

Claude Code writes several transcript records for one request while its response streams, copies subagent records into the parent transcript, and repeats earlier requests when a session is resumed.
A request is identified by its message id and request id; without a request id, by its message id within the session; without a message id, by the record's own id.
Of the records that share an identity, the one with the largest usage wins, because it carries the final tally of a streamed response.

## Considered options

- **Message id alone** merges different requests when a gateway reuses message ids across sessions.
- **Message id and timestamp** counts each streamed content block of one response as a separate request.
