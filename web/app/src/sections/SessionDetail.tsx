// One session: what it cost, against Claude Code's own figure, its cost as it went, its tokens by
// type, its models and subagents, and the skills and MCP servers its requests are credited to.

import { type MouseEvent, useState } from "react";

import type { Credit, SessionDetail as Detail, Totals, WhoRow } from "@claude-dashboard/protocol";

import { LineChart } from "../charts/lines";
import { moneyTick, niceScale } from "../charts/scale";
import { useHost, useStatus } from "../context";
import { useRequest } from "../data";
import { compact, dayLabel, duration, grouped, local, money, percent } from "../format";
import { seriesFill } from "../models";
import { ChartIcon, Copy, Folder, TableIcon } from "../ui/icons";
import { Panel, Problem, Tile, TableScroll } from "../ui/panels";
import { useNow } from "../ui/time";
import { useView } from "../view";
import { ExportMenu } from "../exports";
import { cacheShare, costText, tokensOf, useSeries } from "./common";
import { OwnFigure, RUNNING_MS, unpriced } from "./Sessions";

/** How Claude Code was started, as the session's heading says it. */
const ENTRYPOINTS: Record<string, string> = {
  cli: "terminal",
  "claude-vscode": "VS Code",
  "claude-desktop": "desktop app",
  "sdk-cli": "Agent SDK",
  "sdk-ts": "Agent SDK",
  "sdk-py": "Agent SDK",
};

export function SessionDetail({ id }: { id: string }) {
  const { change } = useView();
  const answer = useRequest("session", { id });
  const back = (event: MouseEvent) => {
    event.preventDefault();
    change({ session: null });
  };
  const detail = answer.data;
  return (
    <div className="section">
      <nav aria-label="Breadcrumb" className="breadcrumb">
        <ol>
          <li>
            <a href="#sessions" onClick={back}>
              Sessions
            </a>
          </li>
          <li aria-hidden="true">/</li>
          <li aria-current="page">{detail?.title ?? (detail ? "Untitled session" : "")}</li>
        </ol>
      </nav>
      {answer.error && !detail ? (
        <Problem title="This session cannot be shown" detail={answer.error.message} />
      ) : detail ? (
        <Body detail={detail} />
      ) : null}
    </div>
  );
}

function Body({ detail }: { detail: Detail }) {
  const status = useStatus();
  const now = useNow(60_000);
  const subagents = new Set(detail.who.flatMap((row) => (row.subagent ? [row.subagent] : [])));
  const subagentRequests = detail.totals.requests - detail.own.requests;
  const running =
    (detail.drift === null || detail.drift === undefined) && now - detail.lastMs < RUNNING_MS;
  const unpricedModels = new Set(
    detail.who.filter((row) => unpriced(row.model)).map((row) => row.model.id),
  ).size;
  return (
    <>
      <Heading detail={detail} />
      <div className="tiles four">
        <Tile
          label="API-equivalent cost"
          value={costText(detail.totals)}
          note="Estimate at list prices"
        />
        <Tile
          label="Claude Code's own figure"
          value={
            detail.reportedUsd === null || detail.reportedUsd === undefined
              ? "None yet"
              : money(detail.reportedUsd)
          }
          note={
            <OwnFigure
              reported={detail.reportedUsd}
              drift={detail.drift}
              running={running}
              unpricedModels={unpricedModels}
              long
            />
          }
        />
        <Tile
          label="Requests"
          value={grouped(detail.totals.requests)}
          note={
            subagents.size === 0
              ? "All in the session"
              : `${grouped(detail.own.requests)} in the session · ${grouped(subagentRequests)} by ${
                  subagents.size === 1 ? "1 subagent" : `${String(subagents.size)} subagents`
                }`
          }
        />
        <Tile
          label="Tokens"
          value={compact(tokensOf(detail.totals))}
          note={`${percent(cacheShare(detail.totals) * 100)} read from the prompt cache`}
        />
      </div>
      <CostOverSession detail={detail} timeZone={status.timeZone} />
      <div className="pair">
        <TokensByType totals={detail.totals} />
        <WhoAndCredits detail={detail} timeZone={status.timeZone} />
      </div>
    </>
  );
}

function Heading({ detail }: { detail: Detail }) {
  const status = useStatus();
  const host = useHost();
  const [copied, setCopied] = useState(false);
  const first = local(detail.firstMs, status.timeZone);
  const last = local(detail.lastMs, status.timeZone);
  const span =
    first.day === last.day
      ? `${dayLabel(first.day)}, ${first.time} to ${last.time}`
      : `${dayLabel(first.day)}, ${first.time} to ${dayLabel(last.day)}, ${last.time}`;
  const started = detail.entrypoint
    ? (ENTRYPOINTS[detail.entrypoint] ?? detail.entrypoint)
    : undefined;
  const version = [
    detail.claudeCodeVersion ? `Claude Code ${detail.claudeCodeVersion}` : undefined,
    started,
  ]
    .filter(Boolean)
    .join(" · ");
  const copy = () => {
    void navigator.clipboard.writeText(detail.id).then(() => {
      setCopied(true);
      setTimeout(() => {
        setCopied(false);
      }, 2000);
    });
  };
  return (
    <div className="detail-head">
      <div className="detail-titles">
        <h1>{detail.title ?? "Untitled session"}</h1>
        <span className="detail-meta">
          <span className="mono project-name">{detail.project.name}</span>
          <span>{span}</span>
          <span>{duration(detail.lastMs - detail.firstMs)}</span>
          {version && <span>{version}</span>}
        </span>
      </div>
      <div className="actions">
        <button type="button" className="button" onClick={copy}>
          <Copy />
          {copied ? "Copied" : "Copy session ID"}
        </button>
        {detail.transcript && host.reveal && (
          <button
            type="button"
            className="button"
            onClick={() => {
              if (detail.transcript) host.reveal?.(detail.transcript);
            }}
          >
            <Folder />
            Show transcript folder
          </button>
        )}
        <ExportMenu
          what={{ dataset: "requests", filters: {}, session: detail.id }}
          scope={`All ${grouped(detail.totals.requests)} ${
            detail.totals.requests === 1 ? "request" : "requests"
          } of this session, its subagents' included`}
          className="button export-button"
        />
      </div>
    </div>
  );
}

/** Six times spread evenly from `from` to `to`. */
function times(from: number, to: number, timeZone: string): { x: number; text: string }[] {
  return Array.from({ length: 6 }, (_, index) => {
    const at = from + ((to - from) * index) / 5;
    return { x: at, text: local(at, timeZone).time };
  });
}

function CostOverSession({ detail, timeZone }: { detail: Detail; timeZone: string }) {
  const series = useSeries();
  const [asTable, setAsTable] = useState(false);
  const points = detail.cumulativeCost;
  const total = points.at(-1)?.usd ?? 0;
  const scale = niceScale(total, 2);
  const from = detail.firstMs;
  const to = Math.max(detail.lastMs, from + 60_000);
  const costAt = (ms: number) => points.reduce((so, point) => (point.ms <= ms ? point.usd : so), 0);
  const starts = new Map<string, WhoRow>();
  for (const row of detail.who) {
    if (row.subagent && !starts.has(row.subagent)) starts.set(row.subagent, row);
  }
  const marks = [...starts.values()];
  const label = `Cumulative cost rises from $0 at ${local(from, timeZone).time} to ${money(total)} at ${
    local(to, timeZone).time
  }${
    marks.length > 0
      ? `, with ${marks.length === 1 ? "a subagent" : "subagents"} starting at ${marks
          .map((row) => local(row.firstMs, timeZone).time)
          .join(", ")}`
      : ""
  }.`;
  return (
    <Panel
      title="Cost over the session"
      subtitle={
        marks.length > 0
          ? "Cumulative API-equivalent cost · dots mark where subagents started"
          : "Cumulative API-equivalent cost"
      }
      actions={
        <button
          type="button"
          className="button small quiet"
          onClick={() => {
            setAsTable(!asTable);
          }}
        >
          {asTable ? <ChartIcon /> : <TableIcon />}
          {asTable ? "Show as chart" : "Show requests as a table"}
        </button>
      }
    >
      {asTable ? (
        <TableScroll label="Requests" tall>
          <table className="data">
            <thead>
              <tr>
                <th scope="col">Time</th>
                <th scope="col">Request</th>
                <th scope="col">So far</th>
              </tr>
            </thead>
            <tbody>
              {points.map((point, index) => (
                <tr key={`${String(point.ms)}-${String(index)}`}>
                  <th scope="row">{local(point.ms, timeZone).time}</th>
                  <td>{money(point.usd - (points[index - 1]?.usd ?? 0))}</td>
                  <td className="strong">{money(point.usd)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </TableScroll>
      ) : (
        <LineChart
          lines={[
            {
              id: "cost",
              colour: "var(--meter)",
              area: true,
              points: [
                { x: from, y: 0 },
                ...points.map((point) => ({ x: point.ms, y: point.usd })),
              ],
            },
          ]}
          x={[from, to]}
          yTop={scale.top}
          height={176}
          ticks={scale.ticks.map((tick) => ({ y: tick, text: moneyTick(tick, scale.ticks) }))}
          dots={marks.map((row) => ({
            x: row.firstMs,
            y: costAt(row.firstMs),
            colour: seriesFill(series.get(row.model.id) ?? "other"),
            label: `${row.agentType ?? "A"} subagent`,
          }))}
          xLabels={times(from, to, timeZone)}
          label={label}
          probe={(at) => {
            const so = costAt(at);
            const time = local(at, timeZone).time;
            return {
              x: at,
              title: time,
              rows: (
                <div className="tooltip-row">
                  <span className="swatch-name">Cost so far</span>
                  <span className="figure">{money(so)}</span>
                </div>
              ),
              summary: `${time}: ${money(so)} so far.`,
            };
          }}
        />
      )}
    </Panel>
  );
}

function TokensByType({ totals }: { totals: Totals }) {
  const { tokens, cost } = totals;
  const rows = [
    { name: "Cache reads", tokens: tokens.cacheRead, cost: cost.cacheRead },
    { name: "Cache writes, 1 hour", tokens: tokens.cacheWrite1h, cost: cost.cacheWrite1h },
    { name: "Cache writes, 5 minutes", tokens: tokens.cacheWrite5m, cost: cost.cacheWrite5m },
    { name: "Output", tokens: tokens.output, cost: cost.output },
    { name: "Input", tokens: tokens.input, cost: cost.input },
  ].filter((row) => row.tokens > 0);
  const all = tokensOf(totals);
  const largest = Math.max(...rows.map((row) => row.tokens), 0);
  return (
    <Panel title="Tokens by type">
      <table className="data">
        <thead>
          <tr>
            <th scope="col">Type</th>
            <th scope="col" className="left share-column">
              Share of tokens
            </th>
            <th scope="col">Tokens</th>
            <th scope="col">Cost</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.name}>
              <th scope="row">{row.name}</th>
              <td className="left">
                <span className="share-cell">
                  <span
                    className="bar"
                    aria-hidden="true"
                    style={{ width: `${String(largest > 0 ? (row.tokens / largest) * 100 : 0)}%` }}
                  />
                  <span className="share-text">
                    {percent(all > 0 ? (row.tokens / all) * 100 : 0)}
                  </span>
                </span>
              </td>
              <td>{compact(row.tokens)}</td>
              <td className="strong">{money(row.cost)}</td>
            </tr>
          ))}
          {totals.webSearchRequests > 0 && (
            <tr>
              <th scope="row">Web searches</th>
              <td className="left" />
              <td>{grouped(totals.webSearchRequests)}</td>
              <td className="strong">{money(cost.webSearch)}</td>
            </tr>
          )}
        </tbody>
      </table>
    </Panel>
  );
}

function WhoAndCredits({ detail, timeZone }: { detail: Detail; timeZone: string }) {
  const series = useSeries();
  const credits: { kind: string; credit: Credit }[] = [
    ...detail.skills.map((credit) => ({ kind: "Skill", credit })),
    ...detail.mcpTools.map((credit) => ({ kind: "MCP server", credit })),
  ];
  const span = (row: WhoRow) => {
    const first = local(row.firstMs, timeZone).time;
    const last = local(row.lastMs, timeZone).time;
    return first === last ? first : `${first} to ${last}`;
  };
  return (
    <Panel title="Models and subagents">
      <table className="data">
        <thead>
          <tr>
            <th scope="col">Who</th>
            <th scope="col" className="left">
              Model
            </th>
            <th scope="col">When</th>
            <th scope="col">Requests</th>
            <th scope="col">Cost</th>
          </tr>
        </thead>
        <tbody>
          {detail.who.map((row) => (
            <tr key={`${row.subagent ?? "session"}-${row.model.id}`}>
              <th scope="row" className={row.subagent ? undefined : "strong"}>
                {row.subagent ? `${row.agentType ?? "Unnamed"} subagent` : "The session"}
              </th>
              <td className="left">
                <span className="swatch-name">
                  <span
                    className="swatch"
                    style={{ background: seriesFill(series.get(row.model.id) ?? "other") }}
                  />
                  <span className={unpriced(row.model) ? "mono" : undefined}>{row.model.name}</span>
                </span>
              </td>
              <td className="nowrap">{span(row)}</td>
              <td>{grouped(row.totals.requests)}</td>
              <td className="strong">{costText(row.totals)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <h3 className="minor-heading">Credited to skills and MCP servers</h3>
      <ul className="credits">
        {credits.map(({ kind, credit }) => (
          <li key={`${kind}-${credit.name}-${credit.tool ?? ""}`}>
            <span>
              <span className="muted">{kind}</span> {credit.name}
              {credit.tool && <span className="muted"> {credit.tool}</span>}
            </span>
            <span className="credit-figures">
              {requestsText(credit.totals)} · {costText(credit.totals)}
            </span>
          </li>
        ))}
        <li className="muted">
          <span>
            {credits.length === 0
              ? "Not credited to a skill or an MCP server"
              : "Not credited to either"}
          </span>
          <span>
            {requestsText(detail.uncredited)} · {costText(detail.uncredited)}
          </span>
        </li>
      </ul>
    </Panel>
  );
}

function requestsText(totals: Totals): string {
  return `${grouped(totals.requests)} ${totals.requests === 1 ? "request" : "requests"}`;
}
