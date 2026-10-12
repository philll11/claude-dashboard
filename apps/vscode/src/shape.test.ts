import { readFileSync } from "node:fs";

import type { ModelRow, Overview, Status, Totals, WindowReading } from "@claude-dashboard/protocol";
import { describe, expect, it } from "vitest";

import { statusItem, type Summary, tree } from "./shape";

// 12:15 on Saturday 10 October 2026 in Auckland, the design's moment.
const NOW = Date.parse("2026-10-09T23:15:00Z");
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;

function totals(usd: number, unpricedRequests = 0): Totals {
  return {
    requests: 10,
    tokens: { input: 1, output: 2, cacheRead: 3, cacheWrite5m: 4, cacheWrite1h: 5 },
    cost: {
      total: usd,
      input: 0,
      output: usd,
      cacheRead: 0,
      cacheWrite5m: 0,
      cacheWrite1h: 0,
      webSearch: 0,
    },
    unpricedRequests,
    webSearchRequests: 0,
    fastRequests: 0,
  };
}

function window(
  name: string,
  usedPercentage: number,
  resetsAtMs: number,
  extra: Partial<WindowReading> = {},
): WindowReading {
  return {
    window: name,
    usedPercentage,
    resetsAtMs,
    observedMs: NOW - 2 * MINUTE,
    provenance: "official",
    level: usedPercentage >= 90 ? "critical" : usedPercentage >= 70 ? "warning" : "normal",
    stale: false,
    expired: false,
    ...extra,
  };
}

function model(id: string, name: string, usd: number): ModelRow {
  return { model: { id, name }, totals: totals(usd) };
}

function overview(models: ModelRow[], projects: Overview["projects"] = []): Overview {
  return {
    days: { first: "2026-10-05", last: "2026-10-10" },
    totals: totals(0),
    byDay: [],
    models,
    projects,
  };
}

const LIMITS = [
  window("seven_day:claude-fable", 18, Date.parse("2026-10-14T20:00:00Z"), { model: "Fable" }),
  window("five_hour", 42, Date.parse("2026-10-10T01:20:00Z")),
  window("seven_day", 71, Date.parse("2026-10-14T20:00:00Z")),
];

function summary(change: Partial<Status> = {}, limitsConnected = true): Summary {
  const status: Status = {
    version: "0.1.1",
    timeZone: "Pacific/Auckland",
    today: "2026-10-10",
    firstDay: "2026-07-31",
    readMs: NOW,
    access: "readWrite",
    limits: LIMITS,
    todayTotals: totals(5.1),
    weekTotals: totals(48.32),
    weekStart: "monday",
    ...change,
  };
  const project = (id: string, name: string, usd: number) => ({
    project: { id, name },
    sessions: 3,
    lastMs: NOW,
    totals: totals(usd),
    dailyCost: [],
  });
  return {
    status,
    choices: {
      projects: [],
      models: [
        { id: "claude-opus-5-5", name: "Opus 5.5" },
        { id: "claude-opus-4-8", name: "Opus 4.8" },
        { id: "claude-fable-5-1", name: "Fable 5.1" },
      ],
    },
    today: overview([
      model("claude-opus-5-5", "Opus 5.5", 2.1),
      model("claude-fable-5-1", "Fable 5.1", 1.83),
      model("claude-opus-4-8", "Opus 4.8", 0.04),
    ]),
    week: overview(
      [model("claude-opus-5-5", "Opus 5.5", 30)],
      ["a", "b", "c", "d", "e", "f"].map((name, index) => project(`C--${name}`, name, 10 - index)),
    ),
    limitsConnected,
  };
}

describe("the status bar item", () => {
  it("shows the five-hour window and when it resets", () => {
    const item = statusItem(summary(), NOW);
    expect(item.text).toBe("5h 42% · 14:20");
    expect(item.label).toBe(
      "Five-hour window 42% used, resets at 14:20. Open the usage dashboard.",
    );
    expect(item.level).toBe("normal");
    expect(item.lines).toEqual([
      { label: "Five-hour window", value: "42% · resets 14:20" },
      { label: "Weekly, all models", value: "71% ▲ · resets Thu 09:00" },
      { label: "Weekly, Fable", value: "18% · resets Thu 09:00" },
      { label: "Today", value: "$5.10" },
    ]);
    expect(item.note).toBe(
      "Limits: official, status line, 2 min ago. Cost: API-equivalent estimate.",
    );
  });

  it("takes the five-hour window's level for its colour", () => {
    const limits = [window("five_hour", 93, NOW + HOUR)];
    expect(statusItem(summary({ limits }), NOW).level).toBe("critical");
  });

  it("marks a window over 70% in the tooltip, not in the text, and not once it has reset", () => {
    const over = statusItem(summary({ limits: [window("five_hour", 93, NOW + HOUR)] }), NOW);
    expect(over.text).toBe("5h 93% · 13:15");
    expect(over.lines[0]).toEqual({ label: "Five-hour window", value: "93% ▲ · resets 13:15" });
    const reset = statusItem(summary({ limits: [window("five_hour", 93, NOW - HOUR)] }), NOW);
    expect(reset.lines[0]).toEqual({ label: "Five-hour window", value: "reset at 11:15" });
  });

  it("shows today's cost once the five-hour window has reset", () => {
    const item = statusItem(summary(), Date.parse("2026-10-10T01:21:00Z"));
    expect(item.text).toBe("$5.10 today");
    expect(item.label).toBe("API-equivalent cost today $5.10. Open the usage dashboard.");
    expect(item.lines[0]).toEqual({ label: "Five-hour window", value: "reset at 14:20" });
  });

  it("shows today's cost without plan limits, and says that some requests have no price", () => {
    const item = statusItem(summary({ limits: [], todayTotals: totals(5.1, 2) }), NOW);
    expect(item.text).toBe("$5.10 + unpriced today");
    expect(item.note).toBe("Cost: API-equivalent estimate. Plan limits are not recorded yet.");
  });
});

/** An example of design/glance.json, which the server's Rust tests check as well. */
interface GlanceExample {
  name: string;
  nowMs: number;
  timeZone: string;
  limits: WindowReading[];
  today: { usd: number; unpricedRequests: number };
  expect: { lines: [string, string][]; source: string | null; statusText: string };
}

const GLANCE = JSON.parse(
  readFileSync(new URL("../../../design/glance.json", import.meta.url), "utf8"),
) as { examples: GlanceExample[] };

describe("the glance's shared examples", () => {
  it.each(GLANCE.examples)("$name", (example) => {
    const item = statusItem(
      summary({
        limits: example.limits,
        timeZone: example.timeZone,
        todayTotals: totals(example.today.usd, example.today.unpricedRequests),
      }),
      example.nowMs,
    );
    expect(item.lines.map((line) => [line.label, line.value])).toEqual(example.expect.lines);
    expect(item.text).toBe(example.expect.statusText);
    const { source } = example.expect;
    if (source === null) {
      expect(item.note).not.toMatch(/^Limits: /);
    } else {
      const prefix = `Limits: ${source.charAt(0).toLowerCase()}${source.slice(1)}`;
      expect(item.note.slice(0, prefix.length)).toBe(prefix);
    }
  });
});

describe("the sidebar tree", () => {
  const rows = (nodes: ReturnType<typeof tree>) =>
    nodes.flatMap((node) => [
      `${node.label} | ${node.description}`,
      ...node.children.map((child) => `  ${child.label} | ${child.description}`),
    ]);

  it("lists plan limits, today by model, this week and the top five projects", () => {
    expect(rows(tree(summary(), NOW))).toEqual([
      "Plan limits | official · 2 min ago",
      "  Five-hour window | 42% · resets 14:20",
      "  Weekly, all models | 71% · resets Thu 09:00",
      "  Weekly, Fable | 18% · resets Thu 09:00",
      "Today | $5.10 estimate",
      "  Opus 5.5 | $2.10",
      "  Fable 5.1 | $1.83",
      "  Opus 4.8 | $0.04",
      "This week | $48.32 estimate",
      "  Opus 5.5 | $30.00",
      "Top projects | this week",
      "  a | $10.00",
      "  b | $9.00",
      "  c | $8.00",
      "  d | $7.00",
      "  e | $6.00",
    ]);
  });

  it("marks a window over 70% and colours each model by its family's newest", () => {
    const [limits, today] = tree(summary(), NOW);
    expect(limits?.children.map((child) => child.icon)).toEqual([
      { kind: "gauge" },
      { kind: "level", level: "warning" },
      { kind: "gauge" },
    ]);
    expect(today?.children.map((child) => child.icon)).toEqual([
      { kind: "model", series: "opus" },
      { kind: "model", series: "fable" },
      { kind: "model", series: "other" },
    ]);
  });

  it("opens the section each item belongs to", () => {
    const [limits, today, week, projects] = tree(summary(), NOW);
    expect(limits?.show).toEqual({ section: "limits" });
    expect(today?.show).toEqual({ section: "overview", range: "today" });
    expect(today?.children[0]?.show).toEqual({ section: "models", range: "today" });
    expect(week?.show).toEqual({ section: "overview" });
    expect(projects?.show).toEqual({ section: "projects" });
    expect(projects?.children[0]?.show).toEqual({ section: "sessions", project: "C--a" });
  });

  it("offers to connect the status line until plan limits are recorded", () => {
    const [limits] = tree(summary({ limits: [] }, false), NOW);
    expect(limits?.description).toBe("not recorded");
    expect(limits?.children.map((child) => child.label)).toEqual([
      "Connect Claude Code's status line",
    ]);
    const [waiting] = tree(summary({ limits: [] }, true), NOW);
    expect(waiting?.description).toBe("waiting for Claude Code");
    expect(waiting?.children).toEqual([]);
  });
});
