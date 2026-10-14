// The situations the dashboard is previewed and tested in, each a synthetic Claude Code folder
// read by the real server. A few states only last a moment in real use, such as a first read
// under way; those scenarios hold them still by changing the server's answers.

import type { Scenario } from "./server.ts";

function withStatus(
  change: (status: Record<string, unknown>) => void,
): NonNullable<Scenario["patch"]> {
  return (method, result) => {
    if (method !== "status" || result === null || typeof result !== "object") return result;
    const status = { ...(result as Record<string, unknown>) };
    change(status);
    return status;
  };
}

/** The prices as after a daily check that found a newer table this morning, at 08:00 in Auckland. */
const pricesRefreshed: NonNullable<Scenario["patch"]> = (method, result) => {
  if (method !== "prices" || result === null || typeof result !== "object") return result;
  const morning = Date.parse("2026-10-09T19:00:00Z");
  return {
    ...(result as Record<string, unknown>),
    downloadedMs: morning,
    refresh: { enabled: true, checkedMs: morning, outcome: "up-to-date", detail: null },
  };
};

/** The gateway session as though Claude Code's figure for it counted other requests, such as an
 * earlier process's while a later one still runs. */
const otherRequests: NonNullable<Scenario["patch"]> = (method, result) => {
  const gateway = (row: Record<string, unknown>) =>
    row.title === "Rate limiter for the gateway" ? { ...row, drift: null } : row;
  if (result === null || typeof result !== "object") return result;
  if (method === "session") return gateway(result as Record<string, unknown>);
  if (method !== "sessions") return result;
  const page = result as { rows: Record<string, unknown>[] };
  return { ...page, rows: page.rows.map(gateway) };
};

/** The running session as though it used only a model the price table does not list. */
const unpricedSession: NonNullable<Scenario["patch"]> = (method, result) => {
  const unpriced = (row: Record<string, unknown>) => {
    if (row.title !== "Review the runbook") return row;
    const totals = row.totals as { requests: number; cost: Record<string, number> };
    const cost = Object.fromEntries(Object.keys(totals.cost).map((key) => [key, 0]));
    return {
      ...row,
      models: [{ id: "glm-4.6", name: "glm-4.6" }],
      totals: { ...totals, cost, unpricedRequests: totals.requests },
    };
  };
  if (result === null || typeof result !== "object") return result;
  if (method === "session") return unpriced(result as Record<string, unknown>);
  if (method !== "sessions") return result;
  const page = result as { rows: Record<string, unknown>[] };
  return { ...page, rows: page.rows.map(unpriced) };
};

export const SCENARIOS: Record<string, Scenario> = {
  /** Ten weeks of history, plan limits from the status line, the usage endpoint on. */
  default: {},
  /** No Claude Code folder at all. */
  empty: { empty: true },
  /** The status line is not connected and nothing has recorded plan limits. */
  "no-limits": { options: { noLimits: true } },
  /** The last plan limit reading is more than three hours old. */
  "stale-limits": { options: { staleLimits: true } },
  /** Claude Code's own figure for every Opus session is 6% above the estimate. */
  "prices-drift": { options: { opusDrift: true } },
  /** A first read of the history is under way. */
  "first-run": {
    patch: withStatus((status) => {
      status.reading = { done: 41, total: 254 };
      status.readMs = null;
    }),
  },
  /** The usage endpoint needs Claude Code's login renewed, and the readings are old. */
  "login-expired": { options: { staleLimits: true, loginExpired: true } },
  /** A data folder the person added cannot be read. */
  "unreadable-folder": { options: { unreadableFolder: true } },
  /** A newer version of claude-dashboard has upgraded the history. */
  "newer-schema": { options: { newerSchema: true } },
  /** A newer version of claude-dashboard has upgraded the history beyond what this one reads. */
  "too-new-schema": { options: { tooNewSchema: true }, unreadable: true },
  /** The claude-usage dashboard's database holds July, which no transcript does. */
  "claude-usage": { options: { claudeUsage: true } },
  /** Servers of their own for the browser tests that change something. */
  "change-status-line": { options: { noLimits: true } },
  "change-endpoint": { options: { noLimits: true } },
  "change-folders": {},
  "change-settings": {},
  /** Claude Code's figure for a session counts other requests than the history holds. */
  "other-requests": { patch: otherRequests },
  /** The daily check for a newer price table is on, and found one this morning. */
  "prices-refreshed": { patch: pricesRefreshed },
  /** The running session used only a model the price table does not list. */
  "unpriced-session": { patch: unpricedSession },
  /** A server of its own for the browser tests that save exports. */
  exports: {},
  /** New activity is being read. */
  reading: {
    patch: withStatus((status) => {
      status.reading = { done: 0, total: 2 };
    }),
  },
};
