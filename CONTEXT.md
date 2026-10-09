# Claude Dashboard

Claude Dashboard shows a person how they use Claude Code: what it would cost at API prices, where the usage goes, and how close they are to their plan limits.
It keeps that history after Claude Code deletes its own transcripts.

## Language

### Claude Code's data

**Data root**:
A Claude Code configuration folder whose transcripts and statistics we read, `~/.claude` unless the person has moved it.
_Avoid_: config dir, Claude folder

**Transcript**:
The file in which Claude Code records one session, or one subagent's part of a session.
_Avoid_: log, JSONL file, conversation file

**Session**:
One Claude Code conversation, identified by its session id.
_Avoid_: conversation, chat, thread

**Subagent**:
A helper agent that a session starts; its requests belong to the session that started it.
_Avoid_: sidechain, task, agent (on its own)

**Project**:
The working folder a session ran in.
_Avoid_: workspace, repo, cwd

**Request**:
One call from Claude Code to a Claude model, with the tokens it used.
_Avoid_: turn, message, entry, API call

**Attribution**:
The skill, MCP server or MCP tool that Claude Code credits a request to.
_Avoid_: source, origin

### Our history

**History**:
Every request and session we have recorded, kept after Claude Code deletes the transcripts they came from.
_Avoid_: archive, warehouse, cache

**Ingest**:
Reading new transcript content into the history.
_Avoid_: scan, sync, import

**Import**:
A one-time transfer into the history from another source, such as the claude-usage dashboard's database.
_Avoid_: ingest, migration

**Daily totals**:
Tokens per model per day without request detail, used only for days that no transcript covers.
_Avoid_: summary, rollup

### Cost

**API-equivalent cost**:
What requests would cost at Anthropic's published API prices; an estimate, never a bill.
_Avoid_: cost (on its own), spend, bill, price

**Price table**:
The dated list of API prices per model from which API-equivalent cost is calculated.
_Avoid_: pricing, rates, rate card

**Unpriced model**:
A model with no entry in the price table, whose requests have no API-equivalent cost.
_Avoid_: unknown model, free model

**Cache write**:
Tokens that Claude Code stores in Anthropic's prompt cache, priced by how long they are kept: five minutes or one hour.
_Avoid_: cache creation

**Cache read**:
Tokens served from Anthropic's prompt cache instead of being processed again.
_Avoid_: cache hit

**Reported session cost**:
The cost Claude Code records for a session itself, also calculated at list price.
_Avoid_: actual cost, real cost, true cost

**Reconciliation**:
Comparing a session's API-equivalent cost with its reported session cost.
_Avoid_: validation, verification

### Plan limits

**Plan**:
The Claude subscription, such as Pro or Max, whose usage limits apply to a person.
_Avoid_: tier, account

**Plan window**:
A period over which a plan limit is measured: the five-hour window or the weekly window.
_Avoid_: block, session limit, rate limit

**Limit snapshot**:
How much of a plan window was used and when it resets, as reported at one moment.
_Avoid_: quota, usage bar

**Provenance**:
Where a number comes from: official (Claude Code's status line), estimate (our price table) or experimental (Anthropic's undocumented usage endpoint).
_Avoid_: source, confidence

### Product

**Surface**:
One of the three ways the product is shown: the terminal, the desktop app or VS Code.
_Avoid_: client, front end, app (on its own)
