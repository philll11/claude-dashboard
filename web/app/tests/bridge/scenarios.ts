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

export const SCENARIOS: Record<string, Scenario> = {
  /** Ten weeks of history, plan limits from the status line, the usage endpoint on. */
  default: {},
  /** No Claude Code folder at all. */
  empty: { empty: true },
  /** The status line is not connected and nothing has recorded plan limits. */
  "no-limits": { options: { noLimits: true } },
  /** The last plan limit reading is more than three hours old. */
  "stale-limits": { options: { staleLimits: true } },
  /** Claude Code's own figure for every Opus session this week is 6% above the estimate. */
  "prices-drift": { options: { opusDrift: true } },
  /** A first read of the history is under way. */
  "first-run": {
    patch: withStatus((status) => {
      status.reading = { done: 41, total: 254 };
      status.readMs = null;
    }),
  },
  /** New activity is being read. */
  reading: {
    patch: withStatus((status) => {
      status.reading = { done: 0, total: 2 };
    }),
  },
};
