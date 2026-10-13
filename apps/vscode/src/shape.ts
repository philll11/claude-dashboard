// What the status bar item and the sidebar tree show, worked out from the server's answers. This
// module knows nothing of VS Code, so that its tests run without it.

import type {
  Choices,
  Level,
  Overview,
  Show,
  Status,
  Totals,
  WindowReading,
} from "@claude-dashboard/protocol";
import { ago, money, percent, when } from "@claude-dashboard/app/format";
import { type Series, seriesOf } from "@claude-dashboard/app/models";
import { inOrder, windowName } from "@claude-dashboard/app/windows";

/** The server's answers that the status bar item and the tree are made from. */
export interface Summary {
  status: Status;
  choices: Choices;
  /** Today's requests. */
  today: Overview;
  /** This week's requests, from its first day to today. */
  week: Overview;
  /** Whether Claude Code's status line records plan limits, as far as the settings tell. */
  limitsConnected: boolean;
}

/** A line of the status bar item's tooltip. */
export interface Line {
  label: string;
  value: string;
}

export interface StatusItem {
  /** The text after the gauge: `5h 42% · 14:20`, or `$5.05 today`. */
  text: string;
  /** What a screen reader says for the item. */
  label: string;
  /** How close the window the item shows is to its limit. */
  level: Level;
  lines: Line[];
  /** Where the numbers come from, under the lines. */
  note: string;
}

/** The icon of a tree item. */
export type Icon =
  | { kind: "gauge" }
  | { kind: "level"; level: "warning" | "critical" }
  | { kind: "folder" }
  | { kind: "model"; series: Series }
  | { kind: "connect" };

export interface Node {
  id: string;
  label: string;
  description: string;
  icon?: Icon;
  /** The section of the dashboard the item opens. */
  show: Show;
  children: Node[];
  /** Whether its children show at first. */
  expanded: boolean;
}

/** The most projects the tree lists. */
const TOP_PROJECTS = 5;

/** The newest reading among `readings`. */
function newest(readings: readonly WindowReading[]): WindowReading | undefined {
  return readings.reduce<WindowReading | undefined>(
    (best, reading) => (best && best.observedMs >= reading.observedMs ? best : reading),
    undefined,
  );
}

/** A cost as the dashboard writes it, saying when some requests have no price. */
function cost(totals: Totals): string {
  return totals.unpricedRequests > 0
    ? `${money(totals.cost.total)} + unpriced`
    : money(totals.cost.total);
}

/** Whether a window has reset since its reading, so that the reading no longer tells its use. The
 * server says so when it answers; between answers, the reset time does. */
function expired(window: WindowReading, nowMs: number): boolean {
  return window.expired || (window.resetsAtMs != null && window.resetsAtMs <= nowMs);
}

/** How much of a window is used and when it resets: `42% · resets 14:20`. With `marked`, a window
 * over 70% has `▲` after its percentage, as the tooltip shows it: `71% ▲ · resets Thu 09:00`. */
function reading(window: WindowReading, nowMs: number, timeZone: string, marked = false): string {
  const resetsAt = window.resetsAtMs;
  if (expired(window, nowMs)) {
    return resetsAt == null ? "reset" : `reset at ${when(resetsAt, nowMs, timeZone)}`;
  }
  const share = percent(window.usedPercentage);
  const used = marked && window.level !== "normal" ? `${share} ▲` : share;
  return resetsAt == null ? used : `${used} · resets ${when(resetsAt, nowMs, timeZone)}`;
}

/** Where the newest reading comes from, for a sentence: `official, status line`. */
function source(window: WindowReading): string {
  return window.provenance === "official"
    ? "official, status line"
    : "experimental, usage endpoint";
}

/** The status bar item: the five-hour window when it has a current reading, and otherwise today's
 * API-equivalent cost. */
export function statusItem(summary: Summary, nowMs: number): StatusItem {
  const { status } = summary;
  const windows = inOrder(status.limits);
  const lines = windows.map((window) => ({
    label: windowName(window),
    value: reading(window, nowMs, status.timeZone, true),
  }));
  lines.push({ label: "Today", value: cost(status.todayTotals) });
  const latest = newest(windows);
  const note = latest
    ? `Limits: ${source(latest)}, ${ago(latest.observedMs, nowMs)}. Cost: API-equivalent estimate.`
    : "Cost: API-equivalent estimate. Plan limits are not recorded yet.";
  const fiveHour = windows.find(
    (window) => window.window === "five_hour" && !expired(window, nowMs),
  );
  if (fiveHour) {
    const used = percent(fiveHour.usedPercentage);
    const resetsAt = fiveHour.resetsAtMs;
    const time = resetsAt == null ? undefined : when(resetsAt, nowMs, status.timeZone);
    return {
      text: time ? `5h ${used} · ${time}` : `5h ${used}`,
      label: `Five-hour window ${used} used${time ? `, resets at ${time}` : ""}. Open the usage dashboard.`,
      level: fiveHour.level,
      lines,
      note,
    };
  }
  const today = cost(status.todayTotals);
  return {
    text: `${today} today`,
    label: `API-equivalent cost today ${today}. Open the usage dashboard.`,
    level: "normal",
    lines,
    note,
  };
}

/** The sidebar tree: plan limits, today by model, this week, and this week's top projects. */
export function tree(summary: Summary, nowMs: number): Node[] {
  const { status, today, week } = summary;
  const series = seriesOf(summary.choices.models.map((model) => model.id));
  const windows = inOrder(status.limits);
  const latest = newest(windows);
  const limits: Node = {
    id: "limits",
    label: "Plan limits",
    description: latest
      ? `${latest.provenance} · ${ago(latest.observedMs, nowMs)}`
      : summary.limitsConnected
        ? "waiting for Claude Code"
        : "not recorded",
    show: { section: "limits" },
    expanded: true,
    children: windows.map((window) => ({
      id: `limits/${window.window}`,
      label: windowName(window),
      description: reading(window, nowMs, status.timeZone),
      icon:
        window.level === "normal" || expired(window, nowMs)
          ? { kind: "gauge" }
          : { kind: "level", level: window.level },
      show: { section: "limits" },
      children: [],
      expanded: false,
    })),
  };
  if (windows.length === 0 && !summary.limitsConnected) {
    limits.children.push({
      id: "limits/connect",
      label: "Connect Claude Code's status line",
      description: "",
      icon: { kind: "connect" },
      show: { section: "limits" },
      children: [],
      expanded: false,
    });
  }
  const todayNode: Node = {
    id: "today",
    label: "Today",
    description: `${cost(status.todayTotals)} estimate`,
    show: { section: "overview", range: "today" },
    expanded: true,
    children: today.models.map((row) => ({
      id: `today/${row.model.id}`,
      label: row.model.name,
      description: cost(row.totals),
      icon: { kind: "model", series: series.get(row.model.id) ?? "other" },
      show: { section: "models", range: "today" },
      children: [],
      expanded: false,
    })),
  };
  const weekNode: Node = {
    id: "week",
    label: "This week",
    description: `${cost(status.weekTotals)} estimate`,
    show: { section: "overview" },
    expanded: false,
    children: week.models.map((row) => ({
      id: `week/${row.model.id}`,
      label: row.model.name,
      description: cost(row.totals),
      icon: { kind: "model", series: series.get(row.model.id) ?? "other" },
      show: { section: "models" },
      children: [],
      expanded: false,
    })),
  };
  const projects: Node = {
    id: "projects",
    label: "Top projects",
    description: "this week",
    show: { section: "projects" },
    expanded: true,
    children: week.projects.slice(0, TOP_PROJECTS).map((row) => ({
      id: `projects/${row.project.id}`,
      label: row.project.name,
      description: cost(row.totals),
      icon: { kind: "folder" },
      show: { section: "sessions", project: row.project.id },
      children: [],
      expanded: false,
    })),
  };
  return [limits, todayNode, weekNode, projects];
}
