// What every section shares: its heading and filters, the days it covers, and how totals read.

import { type ReactNode, useMemo } from "react";

import type { Days, Totals } from "@claude-dashboard/protocol";

import { useHost, useStatus } from "../context";
import { useRequest } from "../data";
import { money } from "../format";
import { type Series, seriesOf } from "../models";
import { type Choice, Picker, Segmented } from "../ui/controls";
import { type Range, RANGE_NAMES, RANGES, rangeDays, useView } from "../view";

/** The days the chosen range covers. */
export function useDays(): Days {
  const status = useStatus();
  const { view } = useView();
  return rangeDays(view.range, status);
}

/** The range in a subtitle: `last 30 days`. */
export function useRangeText(): string {
  const { view } = useView();
  return RANGE_NAMES[view.range].long.toLowerCase();
}

/** A section's heading, with a line about what it shows and its filters. In VS Code the tab
 * names the section, so the heading is for screen readers only. */
export function SectionHead({
  title,
  summary,
  children,
}: {
  title: string;
  summary?: ReactNode;
  children?: ReactNode;
}) {
  const host = useHost();
  if (host.surface === "vscode" && !summary && !children) {
    return <h1 className="visually-hidden">{title}</h1>;
  }
  return (
    <div className="section-head">
      <div className="section-titles">
        <h1 className={host.surface === "desktop" ? undefined : "visually-hidden"}>{title}</h1>
        {summary && <span className="subtitle">{summary}</span>}
      </div>
      <div className="filters">{children}</div>
    </div>
  );
}

const RANGE_CHOICES: Choice<Range>[] = RANGES.map((range) => ({
  value: range,
  label: RANGE_NAMES[range].short,
}));

const RANGE_MENU: Choice<Range>[] = RANGES.map((range) => ({
  value: range,
  label: RANGE_NAMES[range].long,
}));

/** The date range as a row of buttons. */
export function RangeButtons() {
  const { view, change } = useView();
  return (
    <Segmented
      label="Date range"
      value={view.range}
      choices={RANGE_CHOICES}
      onChange={(range) => {
        change({ range });
      }}
    />
  );
}

/** The date range as a menu. */
export function RangeMenu() {
  const { view, change } = useView();
  return (
    <Picker
      label="Date range"
      value={view.range}
      choices={RANGE_MENU}
      onChange={(range) => {
        change({ range });
      }}
    />
  );
}

export function ProjectMenu() {
  const { view, change } = useView();
  const choices = useRequest("choices", null).data;
  const projects: Choice<string | null>[] = [
    { value: null, label: "All projects" },
    ...(choices?.projects ?? []).map((project) => ({ value: project.id, label: project.name })),
  ];
  if (view.project !== null && !projects.some((choice) => choice.value === view.project)) {
    projects.push({ value: view.project, label: view.project });
  }
  return (
    <Picker
      label="Project"
      value={view.project}
      choices={projects}
      onChange={(project) => {
        change({ project });
      }}
      mono
    />
  );
}

export function ModelMenu() {
  const { view, change } = useView();
  const choices = useRequest("choices", null).data;
  const models: Choice<string | null>[] = [
    { value: null, label: "All models" },
    ...(choices?.models ?? []).map((model) => ({ value: model.id, label: model.name })),
  ];
  if (view.model !== null && !models.some((choice) => choice.value === view.model)) {
    models.push({ value: view.model, label: view.model });
  }
  return (
    <Picker
      label="Model"
      value={view.model}
      choices={models}
      onChange={(model) => {
        change({ model });
      }}
    />
  );
}

/** The filters the view holds, as the server takes them. */
export function useFilters(): { project: string | null; model: string | null } {
  const { view } = useView();
  return { project: view.project, model: view.model };
}

/** Every token, of every kind. */
export function tokensOf(totals: Totals): number {
  const { input, output, cacheRead, cacheWrite5m, cacheWrite1h } = totals.tokens;
  return input + output + cacheRead + cacheWrite5m + cacheWrite1h;
}

/** The share of input tokens read from the prompt cache. */
export function cacheShare(totals: Totals): number {
  const { input, cacheRead, cacheWrite5m, cacheWrite1h } = totals.tokens;
  const all = input + cacheRead + cacheWrite5m + cacheWrite1h;
  return all > 0 ? cacheRead / all : 0;
}

/** API-equivalent cost, saying when some requests could not be priced: `$5.05 + unpriced`. */
export function costText(totals: Totals): string {
  if (totals.requests > 0 && totals.unpricedRequests === totals.requests) return "Unpriced";
  return totals.unpricedRequests > 0
    ? `${money(totals.cost.total)} + unpriced`
    : money(totals.cost.total);
}

/** A change as a signed whole percentage, with a real minus sign: `+12%`, `−8%`. */
export function signedPercent(now: number, before: number): string | undefined {
  if (before <= 0) return undefined;
  const change = Math.round(((now - before) / before) * 100);
  return `${change >= 0 ? "+" : "−"}${String(Math.abs(change))}%`;
}

/** The colour series of every model, the same whatever the filters: each family's newest model in
 * the history gets the family's colour. */
export function useSeries(): Map<string, Series> {
  const choices = useRequest("choices", null).data;
  return useMemo(() => seriesOf(choices?.models.map((model) => model.id) ?? []), [choices]);
}
