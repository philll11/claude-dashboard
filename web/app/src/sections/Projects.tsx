// Projects: each project's cost per day, sessions, requests, tokens and share of the cost, for the
// chosen days and model.

import type { MouseEvent } from "react";

import type { ProjectRow } from "@claude-dashboard/protocol";

import { Sparkline } from "../charts/small";
import { useRequest } from "../data";
import { compact, grouped, money, percent } from "../format";
import { ChevronDown } from "../ui/icons";
import { TableScroll } from "../ui/panels";
import { useView } from "../view";
import {
  costText,
  ModelMenu,
  RangeMenu,
  SectionHead,
  tokensOf,
  useDays,
  useFilters,
  useExportScope,
  useRangeText,
} from "./common";
import { ExportMenu } from "../exports";
import { costPerDay } from "./series";

/** Projects with a smaller share of the cost than this are grouped together. */
const SMALL_SHARE = 0.02;

export function Projects() {
  const days = useDays();
  const filters = useFilters();
  const rangeText = useRangeText();
  const overview = useRequest("overview", { days, filters: { ...filters, project: null } }).data;
  const rows = overview?.projects ?? [];
  const total = overview?.totals.cost.total ?? 0;
  const small = rows.filter((row) => total > 0 && row.totals.cost.total / total < SMALL_SHARE);
  const shown = small.length > 1 ? rows.filter((row) => !small.includes(row)) : rows;
  const summary = overview
    ? `${grouped(rows.length)} ${rows.length === 1 ? "project" : "projects"} ${
        rangeText === "today" ? "today" : `in the ${rangeText}`
      } · ${money(total)} API-equivalent`
    : undefined;
  const largest = Math.max(...shown.map((row) => row.totals.cost.total), 0);
  const exportScope = useExportScope(rows.length, "project", "projects");
  return (
    <div className="section">
      <SectionHead title="Projects" summary={summary}>
        <RangeMenu />
        <ModelMenu />
        <ExportMenu
          what={{ dataset: "projects", days, filters: { ...filters, project: null } }}
          scope={exportScope}
          className="button export-button"
        />
      </SectionHead>
      <section aria-label="Projects" className="panel list">
        <TableScroll label="Projects">
          <table className="data projects">
            <thead>
              <tr>
                <th scope="col">Project</th>
                <th scope="col" className="left">
                  {overview && overview.days.first !== overview.days.last
                    ? `Daily cost, ${grouped(daysIn(overview.days.first, overview.days.last))} days`
                    : "Daily cost"}
                </th>
                <th scope="col">Sessions</th>
                <th scope="col">Requests</th>
                <th scope="col">Tokens</th>
                <th scope="col" aria-sort="descending">
                  <span className="sorted">
                    API-equivalent cost
                    <ChevronDown size={12} stroke={2.25} />
                  </span>
                </th>
                <th scope="col" className="left share-column-narrow">
                  Share
                </th>
              </tr>
            </thead>
            <tbody>
              {overview &&
                shown.map((row) => (
                  <Project
                    key={row.project.id}
                    row={row}
                    first={overview.days.first}
                    last={overview.days.last}
                    total={total}
                    largest={largest}
                  />
                ))}
              {overview && small.length > 1 && (
                <Others
                  rows={small}
                  first={overview.days.first}
                  last={overview.days.last}
                  total={total}
                  largest={largest}
                />
              )}
            </tbody>
          </table>
        </TableScroll>
        {overview && rows.length === 0 && (
          <p className="note empty-list">No projects in these days.</p>
        )}
      </section>
      <p className="footnote">
        A project is the folder a session ran in.
        {small.length > 1 &&
          ` "Other projects" groups the ${String(small.length)} projects with less than 2% each; choose one in Sessions to see them.`}
      </p>
    </div>
  );
}

function Project({
  row,
  first,
  last,
  total,
  largest,
}: {
  row: ProjectRow;
  first: string;
  last: string;
  total: number;
  largest: number;
}) {
  const { change } = useView();
  const open = (event: MouseEvent) => {
    event.preventDefault();
    change({ section: "sessions", project: row.project.id, session: null });
    window.scrollTo(0, 0);
  };
  const costs = costPerDay(
    row.dailyCost.map((day) => [day.date, day.usd] as const),
    first,
    last,
  );
  return (
    <tr>
      <th scope="row">
        <a href={`#project-${row.project.id}`} onClick={open} className="project-link">
          <span className="mono project-title">{row.project.name}</span>
          {row.project.path && (
            <span className="mono project-path">{shortPath(row.project.path)}</span>
          )}
        </a>
      </th>
      <td className="left">
        <Sparkline
          values={costs}
          width={140}
          height={28}
          stroke="var(--bar)"
          label={`Daily cost for ${row.project.name} over ${grouped(costs.length)} days`}
          format={money}
        />
      </td>
      <td>{grouped(row.sessions)}</td>
      <td>{grouped(row.totals.requests)}</td>
      <td>{compact(tokensOf(row.totals))}</td>
      <td className="strong">{costText(row.totals)}</td>
      <td className="left">
        <span className="share-cell">
          <span
            className="bar"
            aria-hidden="true"
            style={{
              width: `${String(largest > 0 ? (row.totals.cost.total / largest) * 100 : 0)}%`,
            }}
          />
          <span className="share-text ink-2">
            {percent(total > 0 ? (row.totals.cost.total / total) * 100 : 0)}
          </span>
        </span>
      </td>
    </tr>
  );
}

function Others({
  rows,
  first,
  last,
  total,
  largest,
}: {
  rows: readonly ProjectRow[];
  first: string;
  last: string;
  total: number;
  largest: number;
}) {
  const cost = rows.reduce((sum, row) => sum + row.totals.cost.total, 0);
  const byDay = new Map<string, number>();
  for (const row of rows) {
    for (const day of row.dailyCost) byDay.set(day.date, (byDay.get(day.date) ?? 0) + day.usd);
  }
  const costs = costPerDay(byDay, first, last);
  const sum = (pick: (row: ProjectRow) => number) =>
    rows.reduce((total, row) => total + pick(row), 0);
  return (
    <tr>
      <th scope="row">
        <span className="project-link">
          <span className="project-title">Other projects</span>
          <span className="mono project-path">{grouped(rows.length)} projects</span>
        </span>
      </th>
      <td className="left">
        <Sparkline
          values={costs}
          width={140}
          height={28}
          stroke="var(--bar)"
          label={`Daily cost for the other projects over ${grouped(costs.length)} days`}
          format={money}
        />
      </td>
      <td>{grouped(sum((row) => row.sessions))}</td>
      <td>{grouped(sum((row) => row.totals.requests))}</td>
      <td>{compact(sum((row) => tokensOf(row.totals)))}</td>
      <td className="strong">{money(cost)}</td>
      <td className="left">
        <span className="share-cell">
          <span
            className="bar"
            aria-hidden="true"
            style={{ width: `${String(largest > 0 ? (cost / largest) * 100 : 0)}%` }}
          />
          <span className="share-text ink-2">{percent(total > 0 ? (cost / total) * 100 : 0)}</span>
        </span>
      </td>
    </tr>
  );
}

function daysIn(first: string, last: string): number {
  return Math.round((Date.parse(last) - Date.parse(first)) / 86_400_000) + 1;
}

/** A working folder as the design shows it: the home folder as `~`. */
function shortPath(path: string): string {
  return path
    .replace(/^\/(home|Users)\/[^/]+(?=\/|$)/, "~")
    .replace(/^[A-Za-z]:\\Users\\[^\\]+(?=\\|$)/, "~");
}
