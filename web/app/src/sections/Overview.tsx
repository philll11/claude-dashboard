// Overview: plan limits, today, this week and this month, cost per day by model, top projects and
// models, for the chosen days and filters.

import { useState } from "react";

import type {
  Limits,
  ModelRow,
  Overview as OverviewAnswer,
  ProjectRow,
  Status,
} from "@claude-dashboard/protocol";

import { StackedColumns } from "../charts/columns";
import { Meter, ShareBar, Sparkline } from "../charts/small";
import { useStatus } from "../context";
import { useRequest } from "../data";
import { addDays, compact, fullDate, grouped, money, percent, resets } from "../format";
import { seriesFill } from "../models";
import { ChartIcon, TableIcon, Warning } from "../ui/icons";
import { Panel, Tile } from "../ui/panels";
import { useNow } from "../ui/time";
import { useView } from "../view";
import {
  cacheShare,
  costText,
  ModelMenu,
  ProjectMenu,
  RangeButtons,
  SectionHead,
  signedPercent,
  tokensOf,
  useDays,
  useFilters,
  useRangeText,
  useSeries,
} from "./common";
import { StatusLineDialog } from "./dialogs";
import { Notices } from "./notices";
import { costColumns, costSeries, dailyCost, startOfWeek } from "./series";
import { inOrder, LEVEL_TEXT, LevelIcon, meterName, Provenance, windowName } from "./windows";

export function Overview() {
  const days = useDays();
  const filters = useFilters();
  const overview = useRequest("overview", { days, filters });
  return (
    <div className="section">
      <SectionHead title="Overview">
        <RangeButtons />
        <ProjectMenu />
        <ModelMenu />
      </SectionHead>
      <Notices />
      <PlanLimits />
      <Tiles />
      <div className="row">
        <DailyCost overview={overview.data} />
        <TopProjects projects={overview.data?.projects} />
      </div>
      <ModelsPanel models={overview.data?.models} totals={overview.data?.totals} />
    </div>
  );
}

function PlanLimits() {
  const status = useStatus();
  const now = useNow(30_000);
  const limits = useRequest("limits", null).data;
  const readings = inOrder(status.limits);
  if (readings.length === 0) return <NoLimits limits={limits} />;
  const stale = readings.every((reading) => reading.stale || reading.expired);
  return (
    <Panel title="Plan limits" actions={<Provenance readings={readings} nowMs={now} />}>
      <div className="meters">
        {readings.map((reading) => (
          <div key={reading.window} className="meter-block">
            <div className="meter-head">
              <span className="meter-name">{windowName(reading)}</span>
              <span
                className={reading.stale || reading.expired ? "meter-figure stale" : "meter-figure"}
              >
                {reading.expired ? "Reset" : percent(reading.usedPercentage)}
              </span>
            </div>
            <Meter
              used={reading.expired ? 0 : reading.usedPercentage}
              level={reading.level}
              stale={reading.stale}
              label={`${meterName(reading)}, ${percent(reading.expired ? 0 : reading.usedPercentage)} used`}
            />
            <div className="meter-foot">
              {!reading.expired && LEVEL_TEXT[reading.level] && (
                <>
                  <span className="level">
                    <LevelIcon level={reading.level} />
                    {LEVEL_TEXT[reading.level]}
                  </span>
                  <span aria-hidden="true">·</span>
                </>
              )}
              <span>
                {reading.expired
                  ? "No reading since it reset"
                  : reading.resetsAtMs
                    ? resets(reading.resetsAtMs, now, status.timeZone)
                    : "No reset time"}
              </span>
            </div>
          </div>
        ))}
      </div>
      {stale && <StaleNote limits={limits} status={status} />}
    </Panel>
  );
}

/** Why the readings are old, and what brings newer ones. */
function StaleNote({ limits, status }: { limits: Limits | undefined; status: Status }) {
  const newest = Math.max(...status.limits.map((reading) => reading.observedMs));
  const at = new Intl.DateTimeFormat("en-GB", {
    timeZone: status.timeZone,
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).format(newest);
  if (limits?.endpoint.enabled && limits.endpoint.outcome === "login-expired") {
    return (
      <p className="note">
        Claude Code's login on this computer has expired, so the usage endpoint can't be asked. Open
        Claude Code once to sign in again; claude-dashboard never refreshes the login itself.
      </p>
    );
  }
  return (
    <p className="note">
      Claude Code hasn't run in a terminal since {at}, so there is no newer reading. Use Claude Code
      in a terminal
      {limits?.endpoint.enabled
        ? "."
        : ", or turn on the experimental usage endpoint to fill the gaps."}
    </p>
  );
}

function NoLimits({ limits }: { limits: Limits | undefined }) {
  const { change } = useView();
  const [connecting, setConnecting] = useState(false);
  if (!limits) return null;
  if (limits.statusLine.state === "connected") {
    return (
      <Panel title="Plan limits">
        <p className="note">
          Claude Code's status line is connected. Your five-hour and weekly windows appear here the
          next time Claude Code runs in a terminal.
        </p>
      </Panel>
    );
  }
  return (
    <Panel title="Plan limits">
      <p className="note">
        Connect Claude Code's status line to see your five-hour and weekly windows. It is the
        official source and updates while Claude Code runs in a terminal.
      </p>
      <div className="actions">
        <button
          type="button"
          className="button primary"
          onClick={() => {
            setConnecting(true);
          }}
        >
          Connect the status line…
        </button>
        <button
          type="button"
          className="button quiet"
          onClick={() => {
            change({ section: "limits" });
          }}
        >
          Other options
        </button>
      </div>
      {connecting && (
        <StatusLineDialog
          connect
          onClose={() => {
            setConnecting(false);
          }}
        />
      )}
    </Panel>
  );
}

/** Today, this week and this month, with the days before each for comparison. */
function Tiles() {
  const status = useStatus();
  const filters = useFilters();
  const today = status.today;
  const weekStart = startOfWeek(today, status.weekStart);
  const monthStart = `${today.slice(0, 8)}01`;
  const first = [addDays(today, -29), monthStart, addDays(weekStart, -7)].sort()[0] ?? today;
  const recent = useRequest("overview", { days: { first, last: today }, filters }).data;
  if (!recent) return <div className="tiles" />;
  const sum = (from: string, to: string) => {
    let cost = 0;
    let tokens = 0;
    for (const day of recent.byDay) {
      if (day.date < from || day.date > to) continue;
      cost += day.totals.cost.total;
      tokens += tokensOf(day.totals);
    }
    return { cost, tokens };
  };
  const todays = sum(today, today);
  const yesterdays = sum(addDays(today, -1), addDays(today, -1));
  const week = sum(weekStart, today);
  const lastWeek = sum(addDays(weekStart, -7), addDays(today, -7));
  const month = sum(monthStart, today);
  const spark = (count: number) => dailyCost(recent.byDay, addDays(today, 1 - count), today);
  const compared = (now: number, before: number, what: string) => {
    const change = signedPercent(now, before);
    return change ? `${change} on ${what}` : `nothing ${what}`;
  };
  return (
    <div className="tiles">
      <Tile
        label="Today"
        value={money(todays.cost)}
        note={`${compact(todays.tokens)} tokens · ${compared(todays.cost, yesterdays.cost, "yesterday")}`}
      >
        <Sparkline values={spark(12)} label="Daily cost over the last 12 days" />
      </Tile>
      <Tile
        label="This week"
        value={money(week.cost)}
        note={`${compact(week.tokens)} tokens · ${compared(week.cost, lastWeek.cost, "last week")}`}
      >
        <Sparkline values={spark(18)} label="Daily cost over the last 18 days" />
      </Tile>
      <Tile
        label="This month"
        value={money(month.cost)}
        note={`${compact(month.tokens)} tokens · month to date`}
      >
        <Sparkline values={spark(30)} label="Daily cost over the last 30 days" />
      </Tile>
    </div>
  );
}

function DailyCost({ overview }: { overview: OverviewAnswer | undefined }) {
  const status = useStatus();
  const series = useSeries();
  const rangeText = useRangeText();
  const [asTable, setAsTable] = useState(false);
  if (!overview) return <section className="panel grow" aria-label="API-equivalent cost per day" />;
  const chart = costSeries(overview.models, series);
  const { bucket, columns } = costColumns(
    overview.byDay,
    chart,
    overview.days.first,
    overview.days.last,
    status.weekStart,
  );
  const per = bucket === "day" ? "day" : bucket === "week" ? "week" : "month";
  const totals = columns.map((column) => [...column.values.values()].reduce((a, b) => a + b, 0));
  const highest = totals.reduce(
    (best, total, index) => (total > (totals[best] ?? 0) ? index : best),
    0,
  );
  const label = `API-equivalent cost per ${per} by model, ${rangeText}: ${money(
    overview.totals.cost.total,
  )} in all${
    columns[highest] && (totals[highest] ?? 0) > 0
      ? `, highest ${money(totals[highest] ?? 0)} on ${columns[highest].title}`
      : ""
  }.`;
  return (
    <Panel
      className="grow"
      title={`API-equivalent cost per ${per}`}
      subtitle={`Estimate at Anthropic's list prices · ${rangeText}`}
      actions={
        <button
          type="button"
          className="button small quiet"
          onClick={() => {
            setAsTable(!asTable);
          }}
        >
          {asTable ? <ChartIcon /> : <TableIcon />}
          {asTable ? "Show as chart" : "Show as table"}
        </button>
      }
    >
      {chart.series.length > 1 && (
        <ul aria-label="Legend" className="legend">
          {chart.series.map((item) => (
            <li key={item.id}>
              <span className="swatch" style={{ background: item.fill }} />
              <span>{item.name}</span>
              <span className="figure strong">{money(item.total)}</span>
            </li>
          ))}
        </ul>
      )}
      {asTable ? (
        <div className="table-scroll">
          <table className="data">
            <thead>
              <tr>
                <th scope="col">{per === "day" ? "Day" : per === "week" ? "Week" : "Month"}</th>
                {chart.series.map((item) => (
                  <th key={item.id} scope="col">
                    {item.name}
                  </th>
                ))}
                <th scope="col">Total</th>
              </tr>
            </thead>
            <tbody>
              {columns.map((column, index) => (
                <tr key={column.key}>
                  <th scope="row">{column.title}</th>
                  {chart.series.map((item) => (
                    <td key={item.id}>{money(column.values.get(item.id) ?? 0)}</td>
                  ))}
                  <td className="strong">{money(totals[index] ?? 0)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <StackedColumns columns={columns} series={chart.series} label={label} format={money} />
      )}
    </Panel>
  );
}

/** The projects that cost the most, and the rest together. */
function TopProjects({ projects }: { projects: readonly ProjectRow[] | undefined }) {
  const rangeText = useRangeText();
  const rows = projects ?? [];
  const shown = rows.length > 6 ? rows.slice(0, 5) : rows;
  const rest = rows.slice(shown.length);
  const items = [
    ...shown.map((row) => ({
      key: row.project.id,
      name: row.project.name,
      cost: row.totals.cost.total,
      mono: true,
    })),
    ...(rest.length > 0
      ? [
          {
            key: "other",
            name: "Other projects",
            cost: rest.reduce((sum, row) => sum + row.totals.cost.total, 0),
            mono: false,
          },
        ]
      : []),
  ];
  const largest = Math.max(...items.map((item) => item.cost), 0);
  return (
    <Panel className="side" title="Top projects" subtitle={`API-equivalent cost · ${rangeText}`}>
      {items.length === 0 ? (
        <p className="note">
          No projects {rangeText === "today" ? "today" : `in the ${rangeText}`}.
        </p>
      ) : (
        <ol className="ranking">
          {items.map((item) => (
            <li key={item.key}>
              <div className="ranking-row">
                <span className={item.mono ? "mono name" : "name"}>{item.name}</span>
                <span className="figure">{money(item.cost)}</span>
              </div>
              <ShareBar share={largest > 0 ? item.cost / largest : 0} />
            </li>
          ))}
        </ol>
      )}
    </Panel>
  );
}

function ModelsPanel({
  models,
  totals,
}: {
  models: readonly ModelRow[] | undefined;
  totals: OverviewAnswer["totals"] | undefined;
}) {
  const rangeText = useRangeText();
  const prices = useRequest("prices", { models: [] }).data;
  const series = useSeries();
  return (
    <Panel
      title="Models"
      aside={`· ${rangeText}`}
      actions={
        totals && tokensOf(totals) > 0 ? (
          <span className="subtitle">
            {percent(cacheShare(totals) * 100)} of input read from cache
          </span>
        ) : undefined
      }
    >
      <div className="table-scroll">
        <table className="data wide">
          <thead>
            <tr>
              <th scope="col">Model</th>
              <th scope="col">Requests</th>
              <th scope="col">Input</th>
              <th scope="col">Output</th>
              <th scope="col">Cache writes</th>
              <th scope="col">Cache reads</th>
              <th scope="col">API-equivalent cost</th>
            </tr>
          </thead>
          <tbody>
            {(models ?? []).map((row) => {
              const unpriced =
                row.totals.requests > 0 && row.totals.unpricedRequests === row.totals.requests;
              return (
                <tr key={row.model.id}>
                  <th scope="row" className="strong">
                    <span className="swatch-name">
                      <span
                        className="swatch"
                        style={{ background: seriesFill(series.get(row.model.id) ?? "other") }}
                      />
                      <span className={unpriced ? "mono plain" : undefined}>{row.model.name}</span>
                    </span>
                  </th>
                  <td>{grouped(row.totals.requests)}</td>
                  <td>{compact(row.totals.tokens.input)}</td>
                  <td>{compact(row.totals.tokens.output)}</td>
                  <td>
                    {compact(row.totals.tokens.cacheWrite5m + row.totals.tokens.cacheWrite1h)}
                  </td>
                  <td>{compact(row.totals.tokens.cacheRead)}</td>
                  <td className="strong">
                    {unpriced ? (
                      <span className="unpriced">
                        <Warning size={13} />
                        Unpriced
                      </span>
                    ) : (
                      costText(row.totals)
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {prices && (
        <div className="estimate">
          <span className="badge">Estimate</span>
          <span>
            Price table of {fullDate(prices.checked)}
            {prices.drift !== null && prices.drift !== undefined && prices.outOfLine.length === 0
              ? ` · within ${Math.max(0.1, Math.abs(prices.drift) * 100).toFixed(1)}% of Claude Code's own session costs`
              : ""}
          </span>
        </div>
      )}
    </Panel>
  );
}
