// Sessions: every session in the chosen days, most recent first, a page at a time; each opens its
// detail.

import type { MouseEvent } from "react";

import type { Model, SessionRow, Totals } from "@claude-dashboard/protocol";

import { useStatus } from "../context";
import { useRequest } from "../data";
import { compact, dayAndTime, duration, grouped, money } from "../format";
import { seriesFill } from "../models";
import { SearchField } from "../ui/controls";
import { Check, ChevronDown, Warning } from "../ui/icons";
import { Pill, TableScroll } from "../ui/panels";
import { useNow } from "../ui/time";
import { useView } from "../view";
import {
  costText,
  ModelMenu,
  ProjectMenu,
  RangeMenu,
  SectionHead,
  tokensOf,
  useDays,
  useFilters,
  useRangeText,
  useSeries,
} from "./common";
import { SessionDetail } from "./SessionDetail";

/** Sessions on one page. */
const PAGE = 12;

/** How recently a session without Claude Code's own figure made a request, to count as running. */
const RUNNING_MS = 15 * 60_000;

/** How far the estimate may be from Claude Code's figure before prices may be out of date. */
const DRIFT_LIMIT = 0.02;

export function Sessions() {
  const { view } = useView();
  if (view.session !== null) return <SessionDetail id={view.session} />;
  return <SessionList />;
}

function SessionList() {
  const { view, change } = useView();
  const days = useDays();
  const filters = useFilters();
  const rangeText = useRangeText();
  const page = useRequest("sessions", {
    days,
    filters,
    search: view.search === "" ? null : view.search,
    offset: view.offset,
    limit: PAGE,
  }).data;
  const overview = useRequest("overview", { days, filters }).data;
  const summary =
    page && overview && view.search === ""
      ? `${grouped(page.total)} ${page.total === 1 ? "session" : "sessions"} ${
          rangeText === "today" ? "today" : `in the ${rangeText}`
        } · ${money(overview.totals.cost.total)} API-equivalent`
      : page
        ? `${grouped(page.total)} ${page.total === 1 ? "session matches" : "sessions match"}`
        : undefined;
  return (
    <div className="section">
      <SectionHead title="Sessions" summary={summary}>
        <SearchField
          label="Search sessions"
          placeholder="Search titles"
          value={view.search}
          onChange={(search) => {
            change({ search });
          }}
        />
        <RangeMenu />
        <ProjectMenu />
        <ModelMenu />
      </SectionHead>
      <section aria-label="Sessions" className="panel list">
        <TableScroll label="Sessions">
          <table className="data sessions">
            <thead>
              <tr>
                <th scope="col">Session</th>
                <th scope="col" aria-sort="descending">
                  <span className="sorted">
                    Started
                    <ChevronDown size={12} stroke={2.25} />
                  </span>
                </th>
                <th scope="col">Duration</th>
                <th scope="col" className="left">
                  Models
                </th>
                <th scope="col">Requests</th>
                <th scope="col">Subagents</th>
                <th scope="col">Tokens</th>
                <th scope="col">API-equivalent cost</th>
                <th scope="col" className="left">
                  Claude Code's own figure
                </th>
              </tr>
            </thead>
            <tbody>
              {page?.rows.map((row) => (
                <Row key={row.id} row={row} />
              ))}
            </tbody>
          </table>
        </TableScroll>
        {page?.total === 0 && (
          <p className="note empty-list">
            {view.search === "" ? "No sessions in these days." : "No session matches the search."}
          </p>
        )}
        {page && page.total > 0 && (
          <div className="pages">
            <span>
              Showing {grouped(view.offset + 1)} to{" "}
              {grouped(Math.min(page.total, view.offset + page.rows.length))} of{" "}
              {grouped(page.total)} sessions
            </span>
            <div className="actions">
              <button
                type="button"
                className="button small quiet"
                disabled={view.offset === 0}
                onClick={() => {
                  change({ offset: Math.max(0, view.offset - PAGE) });
                }}
              >
                Previous
              </button>
              <button
                type="button"
                className="button small raised"
                disabled={view.offset + PAGE >= page.total}
                onClick={() => {
                  change({ offset: view.offset + PAGE });
                }}
              >
                Next
              </button>
            </div>
          </div>
        )}
      </section>
      <p className="footnote">
        "Claude Code's own figure" compares our estimate with the session cost Claude Code records
        itself. A session gets one when it ends.
      </p>
    </div>
  );
}

function Row({ row }: { row: SessionRow }) {
  const status = useStatus();
  const { change } = useView();
  const now = useNow(60_000);
  const series = useSeries();
  const running =
    row.reportedUsd === null || row.reportedUsd === undefined
      ? now - row.lastMs < RUNNING_MS
      : false;
  const open = (event: MouseEvent) => {
    event.preventDefault();
    change({ session: row.id });
    window.scrollTo(0, 0);
  };
  const priced = row.models.filter((model) => !unpriced(model));
  const unpricedCount = row.models.length - priced.length;
  return (
    <tr>
      <th scope="row" className="session-cell">
        <a href={`#session-${row.id}`} onClick={open} className="session-link">
          <span className="session-title">
            <span className="title-text">{row.title ?? "Untitled session"}</span>
            {running && <Pill tone="accent">Running</Pill>}
          </span>
          <span className="mono session-project">{row.project.name}</span>
        </a>
      </th>
      <td className="nowrap">{dayAndTime(row.firstMs, now, status.timeZone)}</td>
      <td className="nowrap">{duration(row.lastMs - row.firstMs)}</td>
      <td className="left">
        <span
          className="model-swatches"
          role="img"
          aria-label={`${priced.map((model) => model.name).join(", ")}${
            unpricedCount > 0
              ? `${priced.length > 0 ? ", and " : ""}${unpricedCount === 1 ? "one unpriced model" : `${String(unpricedCount)} unpriced models`}`
              : ""
          }`}
        >
          {priced.map((model) => (
            <span
              key={model.id}
              className="swatch"
              style={{ background: seriesFill(series.get(model.id) ?? "other") }}
            />
          ))}
        </span>
      </td>
      <td>{grouped(row.totals.requests)}</td>
      <td>{grouped(row.subagents)}</td>
      <td>{compact(tokensOf(row.totals))}</td>
      <td className="strong">{costText(row.totals)}</td>
      <td className="left nowrap">
        <OwnFigure
          totals={row.totals}
          reported={row.reportedUsd}
          running={running}
          unpricedModels={unpricedCount}
        />
      </td>
    </tr>
  );
}

/** A model the price table does not list: the server names it by its id. */
export function unpriced(model: Model): boolean {
  return model.name === model.id;
}

/** How the estimate compares with the cost Claude Code recorded for the session. */
export function OwnFigure({
  totals,
  reported,
  running,
  unpricedModels,
  long = false,
}: {
  totals: Totals;
  reported: number | null | undefined;
  running: boolean;
  unpricedModels: number;
  /** Says `Matches within 0.1%` rather than `Within 0.1%`. */
  long?: boolean;
}) {
  if (unpricedModels > 0) {
    return (
      <span className="figure-check strong-text">
        <Warning size={14} className="warning-mark" />
        {unpricedModels === 1 ? "1 model unpriced" : `${String(unpricedModels)} models unpriced`}
      </span>
    );
  }
  if (reported === null || reported === undefined || reported <= 0) {
    return (
      <span className="figure-check muted">{running ? "Not ended yet" : "None recorded"}</span>
    );
  }
  const drift = (totals.cost.total - reported) / reported;
  if (Math.abs(drift) <= DRIFT_LIMIT) {
    const within = Math.max(0.1, Math.abs(drift) * 100).toFixed(1);
    return (
      <span className="figure-check">
        <Check size={14} />
        {long ? `Matches within ${within}%` : `Within ${within}%`}
      </span>
    );
  }
  return (
    <span className="figure-check strong-text">
      <Warning size={14} className="warning-mark" />
      {String(Math.round(Math.abs(drift) * 100))}% {drift < 0 ? "lower" : "higher"}, prices may be
      out of date
    </span>
  );
}
