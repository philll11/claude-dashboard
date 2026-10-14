import { describe, expect, it } from "vitest";

import type { SessionRow, Totals } from "@claude-dashboard/protocol";

import { isRunning, latestSession, relation, RUNNING_MS } from "./sessions";

const NOW = Date.parse("2026-10-09T23:15:00Z");
const MINUTE = 60_000;

const TOTALS: Totals = {
  requests: 1,
  tokens: { input: 1, output: 1, cacheRead: 0, cacheWrite5m: 0, cacheWrite1h: 0 },
  cost: {
    total: 0,
    input: 0,
    output: 0,
    cacheRead: 0,
    cacheWrite5m: 0,
    cacheWrite1h: 0,
    webSearch: 0,
  },
  unpricedRequests: 0,
  webSearchRequests: 0,
  fastRequests: 0,
};

function row(id: string, path: string | null, lastMs: number): SessionRow {
  return {
    id,
    title: id,
    project: { id: `project-${id}`, name: id, path },
    firstMs: lastMs - 30 * MINUTE,
    lastMs,
    models: [],
    subagents: 0,
    totals: TOTALS,
  };
}

const POSIX = { ignoreCase: false };
const WINDOWS = { ignoreCase: true };

describe("a running session", () => {
  it("made a request in the last 15 minutes, with no figure from Claude Code that covers it", () => {
    expect(isRunning({ lastMs: NOW - MINUTE }, NOW)).toBe(true);
    expect(isRunning({ lastMs: NOW - MINUTE, drift: null }, NOW)).toBe(true);
    expect(isRunning({ lastMs: NOW - RUNNING_MS }, NOW)).toBe(false);
    expect(isRunning({ lastMs: NOW - MINUTE, drift: 0.001 }, NOW)).toBe(false);
  });
});

describe("how a project's folder stands to the folders VS Code has open", () => {
  it("is inside when it is one of them or inside one", () => {
    const open = { folders: ["/home/you/code/app"], ...POSIX };
    expect(relation("/home/you/code/app", open)).toBe("inside");
    expect(relation("/home/you/code/app/", open)).toBe("inside");
    expect(relation("/home/you/code/app/web", open)).toBe("inside");
  });

  it("is around when it holds one of them", () => {
    const open = { folders: ["/home/you/code/app/web"], ...POSIX };
    expect(relation("/home/you/code/app", open)).toBe("around");
  });

  it("is neither for a folder that only starts with the same letters", () => {
    const open = { folders: ["/home/you/code/app"], ...POSIX };
    expect(relation("/home/you/code/app-old", open)).toBeUndefined();
    expect(relation("/home/you/code/ap", open)).toBeUndefined();
    expect(relation(null, open)).toBeUndefined();
  });

  it("minds case only where file names do", () => {
    expect(relation("/Home/you/app", { folders: ["/home/you/app"], ...POSIX })).toBeUndefined();
    expect(relation("C:\\Users\\you\\app", { folders: ["c:\\users\\you\\app"], ...WINDOWS })).toBe(
      "inside",
    );
  });

  it("reads either slash, and a drive's or the system's top folder", () => {
    expect(relation("C:\\code\\app", { folders: ["C:/code"], ...WINDOWS })).toBe("inside");
    expect(relation("C:\\code\\app", { folders: ["C:\\"], ...WINDOWS })).toBe("inside");
    expect(relation("/home/you", { folders: ["/"], ...POSIX })).toBe("inside");
  });
});

describe("the latest session in the project VS Code has open", () => {
  const app = row("app", "/home/you/code/app", NOW - 20 * MINUTE);
  const appWeb = row("app-web", "/home/you/code/app/web", NOW - 40 * MINUTE);
  const home = row("home", "/home/you", NOW - 5 * MINUTE);
  const other = row("other", "/home/you/code/other", NOW - MINUTE);

  it("is the one whose last request came last, inside an open folder", () => {
    const open = { folders: ["/home/you/code/app"], ...POSIX };
    expect(latestSession([appWeb, other, app, home], open)).toEqual({ row: app, here: true });
  });

  it("is in the nearest project holding an open folder when none is inside one", () => {
    const open = { folders: ["/home/you/code/app/web/src"], ...POSIX };
    expect(latestSession([app, other, home], open)).toEqual({ row: app, here: true });
    expect(latestSession([other, home], open)).toEqual({ row: home, here: true });
  });

  it("is the latest in any project when none is open", () => {
    expect(latestSession([app, other], { folders: [], ...POSIX })).toEqual({
      row: other,
      here: false,
    });
    expect(latestSession([], { folders: ["/home/you/code/app"], ...POSIX })).toBeUndefined();
  });

  it("goes by the last request, not the start", () => {
    const long = {
      ...row("long", "/home/you/code/app", NOW - 2 * MINUTE),
      firstMs: NOW - 600 * MINUTE,
    };
    const open = { folders: ["/home/you/code/app"], ...POSIX };
    expect(latestSession([app, long], open)?.row).toBe(long);
  });
});
