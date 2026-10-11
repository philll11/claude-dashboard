// What the person is looking at: the section, the days, the filters and the open session. It is
// remembered between visits, so the dashboard reopens where it was.

import { createContext, useContext } from "react";

import type { Days, Show, Status } from "@claude-dashboard/protocol";

import { addDays } from "./format";

export const SECTIONS = [
  "overview",
  "sessions",
  "projects",
  "models",
  "limits",
  "settings",
] as const;
export type Section = (typeof SECTIONS)[number];

export const SECTION_NAMES: Record<Section, string> = {
  overview: "Overview",
  sessions: "Sessions",
  projects: "Projects",
  models: "Models",
  limits: "Limits",
  settings: "Settings",
};

export const RANGES = ["today", "7-days", "30-days", "this-month", "all-time"] as const;
export type Range = (typeof RANGES)[number];

/** How each range is named: on its button, in a menu, and in a sentence. */
export const RANGE_NAMES: Record<Range, { short: string; long: string; phrase: string }> = {
  today: { short: "Today", long: "Today", phrase: "today" },
  "7-days": { short: "7 days", long: "Last 7 days", phrase: "in the last 7 days" },
  "30-days": { short: "30 days", long: "Last 30 days", phrase: "in the last 30 days" },
  "this-month": { short: "This month", long: "This month", phrase: "this month" },
  "all-time": { short: "All time", long: "All time", phrase: "in all" },
};

export interface View {
  section: Section;
  range: Range;
  /** A project's id, or `null` for every project. */
  project: string | null;
  /** A model's id, or `null` for every model. */
  model: string | null;
  /** The session shown in Sessions, or `null` for the list. */
  session: string | null;
  search: string;
  /** The first session shown in the list. */
  offset: number;
}

export const FIRST_VIEW: View = {
  section: "overview",
  range: "30-days",
  project: null,
  model: null,
  session: null,
  search: "",
  offset: 0,
};

/** The view remembered from an earlier visit, as far as it is still a view. */
export function rememberedView(state: unknown): View {
  if (state === null || typeof state !== "object") return FIRST_VIEW;
  const saved = state as Partial<Record<keyof View, unknown>>;
  const text = (value: unknown) => (typeof value === "string" ? value : null);
  return {
    section: SECTIONS.find((section) => section === saved.section) ?? FIRST_VIEW.section,
    range: RANGES.find((range) => range === saved.range) ?? FIRST_VIEW.range,
    project: text(saved.project),
    model: text(saved.model),
    session: text(saved.session),
    search: text(saved.search) ?? "",
    offset:
      typeof saved.offset === "number" && Number.isInteger(saved.offset) && saved.offset >= 0
        ? saved.offset
        : 0,
  };
}

/** The view showing what the surface asked for: its section from the top, with the days and
 * filters it names. */
export function shown(view: View, show: Show): View {
  return {
    ...view,
    section: show.section,
    session: null,
    offset: 0,
    range: show.range ?? view.range,
    project: show.project === undefined ? view.project : show.project,
    model: show.model === undefined ? view.model : show.model,
  };
}

/** The days a range covers, as of the server's today. */
export function rangeDays(range: Range, status: Pick<Status, "today" | "firstDay">): Days {
  const last = status.today;
  switch (range) {
    case "today":
      return { first: last, last };
    case "7-days":
      return { first: addDays(last, -6), last };
    case "30-days":
      return { first: addDays(last, -29), last };
    case "this-month":
      return { first: `${last.slice(0, 8)}01`, last };
    case "all-time": {
      const first = status.firstDay ?? last;
      return { first: first < last ? first : last, last };
    }
  }
}

export interface ViewState {
  view: View;
  /** Changes part of the view; a new section, range or filter starts the list from the top. */
  change: (change: Partial<View>) => void;
}

export const ViewContext = createContext<ViewState | null>(null);

export function useView(): ViewState {
  const state = useContext(ViewContext);
  if (!state) throw new Error("useView needs a ViewContext");
  return state;
}
