// Limits: each plan window with its use so far and where it is heading, where the readings come
// from, and recent five-hour windows.

import { useState } from "react";

import type {
  Endpoint,
  FiveHourWindow,
  Limits as LimitsAnswer,
  StatusLine,
  WindowView,
} from "@claude-dashboard/protocol";

import { LineChart } from "../charts/lines";
import { Meter, ShareBar } from "../charts/small";
import { useStatus } from "../context";
import { useRequest, useStore } from "../data";
import { ago, dayAndTime, dayLabel, duration, local, money, percent } from "../format";
import { Warning } from "../ui/icons";
import { Panel, Pill } from "../ui/panels";
import { useNow } from "../ui/time";
import { costText, SectionHead } from "./common";
import { ConsentDialog, StatusLineDialog } from "./dialogs";
import { inOrder, LEVEL_TEXT, LevelIcon, meterName, Provenance, windowName } from "./windows";

const HOUR = 3_600_000;
const DAY = 24 * HOUR;

export function Limits() {
  const status = useStatus();
  const now = useNow(30_000);
  const limits = useRequest("limits", null).data;
  const views = limits ? orderedViews(limits.windows) : [];
  return (
    <div className="section">
      <SectionHead title="Limits" summary="Limits are shared between Claude Code and claude.ai">
        <Provenance readings={status.limits} nowMs={now} />
      </SectionHead>
      {limits && views.length > 0 && (
        <div className="window-cards">
          {views.map((view) => (
            <WindowCard key={view.reading.window} view={view} views={views} />
          ))}
        </div>
      )}
      {limits && (
        <div className="pair">
          <Sources limits={limits} />
          <RecentWindows recent={limits.recent} />
        </div>
      )}
    </div>
  );
}

function orderedViews(views: readonly WindowView[]): WindowView[] {
  const order = inOrder(views.map((view) => view.reading));
  return order.flatMap((reading) => views.filter((view) => view.reading === reading));
}

/** A window's span: `09:20 to 14:20`, or `Thu 8 Oct 09:00 to Thu 15 Oct 09:00`. */
function span(view: WindowView, timeZone: string): string | undefined {
  const { startMs } = view;
  const resetsAtMs = view.reading.resetsAtMs;
  if (startMs === null || startMs === undefined || !resetsAtMs) return undefined;
  const start = local(startMs, timeZone);
  const end = local(resetsAtMs, timeZone);
  return start.day === end.day
    ? `${start.time} to ${end.time}`
    : `${dayLabel(start.day)} ${start.time} to ${dayLabel(end.day)} ${end.time}`;
}

function WindowCard({ view, views }: { view: WindowView; views: readonly WindowView[] }) {
  const status = useStatus();
  const now = useNow(30_000);
  const { reading } = view;
  const name = windowName(reading);
  const used = reading.expired ? 0 : reading.usedPercentage;
  const resetsIn = reading.resetsAtMs ? reading.resetsAtMs - now : undefined;
  const allModels = views.find((other) => other.reading.window === "seven_day");
  const sameWeek =
    reading.model !== null &&
    reading.model !== undefined &&
    allModels?.reading.resetsAtMs === reading.resetsAtMs;
  const level = LEVEL_TEXT[reading.level];
  return (
    <Panel
      className="window-card"
      title={name}
      actions={
        <span className="subtitle">{sameWeek ? "Same week" : span(view, status.timeZone)}</span>
      }
    >
      <div className="window-figure">
        <span className={reading.stale || reading.expired ? "big-figure stale" : "big-figure"}>
          {percent(used)}
        </span>
        <span className={level && !reading.expired ? "window-state strong-text" : "window-state"}>
          {level && !reading.expired && <LevelIcon level={reading.level} size={14} />}
          {[
            level && !reading.expired ? level : "used",
            resetsIn !== undefined && resetsIn > 0 ? `resets in ${duration(resetsIn)}` : undefined,
          ]
            .filter(Boolean)
            .join(" · ")}
        </span>
      </div>
      <Meter
        used={used}
        level={reading.level}
        stale={reading.stale}
        label={`${meterName(reading)}, ${percent(used)} used`}
      />
      <PaceLine view={view} />
      {reading.model ? null : <UseChart view={view} />}
    </Panel>
  );
}

function PaceLine({ view }: { view: WindowView }) {
  const status = useStatus();
  const now = useNow(60_000);
  const { reading, pace } = view;
  if (reading.stale) {
    return (
      <p className="window-note">
        This reading is from{" "}
        {dayAndTime(reading.observedMs, now, status.timeZone).replace(/^Today /, "")},{" "}
        {ago(reading.observedMs, now)}.
      </p>
    );
  }
  if (reading.model) {
    return <p className="window-note">A separate weekly limit for {reading.model}.</p>;
  }
  if (!pace) return null;
  const since =
    Math.abs(reading.observedMs - pace.sinceMs - DAY) < 2 * HOUR
      ? "the last 24 hours"
      : reading.window === "five_hour"
        ? "this window so far"
        : `since ${dayAndTime(pace.sinceMs, now, status.timeZone)}`;
  if (pace.limitReachedMs) {
    const at = local(pace.limitReachedMs, status.timeZone);
    const today = local(now, status.timeZone).day;
    const when =
      at.day === today
        ? `around ${roundedTime(pace.limitReachedMs, status.timeZone)}`
        : `on ${weekdayName(at.day)} around ${roundedTime(pace.limitReachedMs, status.timeZone)}`;
    return (
      <p className="window-note">
        At the pace of {since}, you reach the limit {when} <span className="ink-2">(estimate)</span>
      </p>
    );
  }
  return (
    <p className="window-note">
      At the pace of {since}, about {percent(pace.atReset)} when it resets{" "}
      <span className="ink-2">(estimate)</span>
    </p>
  );
}

const WEEKDAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

function weekdayName(day: string): string {
  return WEEKDAYS[new Date(`${day}T00:00:00Z`).getUTCDay()] ?? "";
}

/** A time rounded to the nearest quarter hour, as estimates say it: `16:00`. */
function roundedTime(ms: number, timeZone: string): string {
  const quarter = 15 * 60_000;
  return local(Math.round(ms / quarter) * quarter, timeZone).time;
}

const LEVEL_COLOUR = {
  normal: "var(--meter)",
  warning: "var(--status-warning)",
  critical: "var(--status-critical)",
} as const;

/** A window's use from its start to now, and where its pace takes it by the reset. */
function UseChart({ view }: { view: WindowView }) {
  const status = useStatus();
  const { reading, pace, startMs } = view;
  const resetsAtMs = reading.resetsAtMs;
  if (startMs === null || startMs === undefined || !resetsAtMs || view.series.length === 0) {
    return null;
  }
  const colour = LEVEL_COLOUR[reading.level];
  const points = [
    { x: startMs, y: 0 },
    ...view.series.map((point) => ({ x: point.ms, y: point.used })),
  ];
  const last = points.at(-1) ?? { x: startMs, y: 0 };
  const projection =
    pace && !reading.stale
      ? pace.limitReachedMs
        ? [last, { x: pace.limitReachedMs, y: 100 }]
        : [last, { x: resetsAtMs, y: pace.atReset }]
      : undefined;
  const fiveHour = reading.window === "five_hour";
  const xLabels = fiveHour
    ? [startMs, (startMs + resetsAtMs) / 2, resetsAtMs].map((x) => ({
        x,
        text: local(x, status.timeZone).time,
      }))
    : Array.from({ length: 5 }, (_, index) => {
        const x = startMs + ((resetsAtMs - startMs) * index) / 4;
        return { x, text: dayLabel(local(x, status.timeZone).day).slice(0, 3) };
      });
  const label = `${windowName(reading)} use rises from 0% at ${dayAndTime(startMs, last.x, status.timeZone)} to ${percent(
    last.y,
  )} at ${dayAndTime(last.x, last.x, status.timeZone)}.${
    projection
      ? pace?.limitReachedMs
        ? ` At the same pace it reaches 100% at ${dayAndTime(pace.limitReachedMs, last.x, status.timeZone)}.`
        : ` At the same pace it reaches about ${percent(pace?.atReset ?? 0)} when it resets.`
      : ""
  }`;
  return (
    <LineChart
      lines={[
        ...(projection
          ? [{ id: "pace", colour: "var(--spark)", points: projection, projected: true }]
          : []),
        { id: "use", colour, points },
      ]}
      x={[startMs, resetsAtMs]}
      yTop={100}
      height={120}
      ticks={[{ y: 0 }, { y: 50 }, { y: 100 }]}
      guides={[{ y: 70, colour: "var(--status-warning)", label: "70%" }]}
      dots={[{ x: last.x, y: last.y, colour }]}
      xLabels={xLabels}
      label={label}
    />
  );
}

function Sources({ limits }: { limits: LimitsAnswer }) {
  const [dialog, setDialog] = useState<"connect" | "disconnect" | "consent" | null>(null);
  const store = useStore();
  const [problem, setProblem] = useState<string | null>(null);
  const turnOff = () => {
    setProblem(null);
    store.send("endpoint/set", { enabled: false }).then(
      () => {
        store.refresh((entry) => entry.method === "limits" || entry.method === "status");
      },
      (error: unknown) => {
        setProblem(error instanceof Error ? error.message : String(error));
      },
    );
  };
  return (
    <Panel title="Where the limits come from" className="sources">
      <div className="source">
        <div className="source-head">
          <span className="source-name">
            Claude Code's status line
            <StatusLinePill state={limits.statusLine} />
          </span>
          {limits.statusLine.state === "connected" ? (
            <button
              type="button"
              className="button small quiet"
              onClick={() => {
                setDialog("disconnect");
              }}
            >
              Disconnect
            </button>
          ) : limits.statusLine.state === "notConnected" ? (
            <button
              type="button"
              className="button small raised"
              onClick={() => {
                setDialog("connect");
              }}
            >
              Connect…
            </button>
          ) : null}
        </div>
        <StatusLineText limits={limits} />
      </div>
      <div className="source">
        <div className="source-head">
          <span className="source-name">
            Anthropic's usage endpoint
            <Pill tone={limits.endpoint.enabled ? "accent" : undefined}>
              {limits.endpoint.enabled ? "On" : "Off"}
            </Pill>
            <Pill tone="caution">Experimental</Pill>
          </span>
          {limits.endpoint.enabled ? (
            <button type="button" className="button small quiet" onClick={turnOff}>
              Turn off
            </button>
          ) : (
            <button
              type="button"
              className="button small raised"
              onClick={() => {
                setDialog("consent");
              }}
            >
              Turn on…
            </button>
          )}
        </div>
        <EndpointText endpoint={limits.endpoint} />
        {problem && <p className="note problem">{problem}</p>}
      </div>
      {dialog === "consent" && (
        <ConsentDialog
          consent={limits.endpoint.consent}
          onClose={() => {
            setDialog(null);
          }}
        />
      )}
      {(dialog === "connect" || dialog === "disconnect") && (
        <StatusLineDialog
          connect={dialog === "connect"}
          onClose={() => {
            setDialog(null);
          }}
        />
      )}
    </Panel>
  );
}

function StatusLinePill({ state }: { state: StatusLine }) {
  if (state.state === "connected") return <Pill tone="accent">Connected</Pill>;
  if (state.state === "notConnected") return <Pill>Not connected</Pill>;
  return <Pill tone="caution">Unknown</Pill>;
}

function StatusLineText({ limits }: { limits: LimitsAnswer }) {
  const status = useStatus();
  const now = useNow(30_000);
  const official = status.limits.filter((reading) => reading.provenance === "official");
  const newest = Math.max(0, ...official.map((reading) => reading.observedMs));
  const state = limits.statusLine;
  if (state.state === "unknown") {
    return (
      <p className="note">claude-dashboard can't tell whether it is connected: {state.reason}.</p>
    );
  }
  if (state.state === "notConnected") {
    return (
      <p className="note">
        Official. Claude Code passes it your five-hour and weekly windows while it runs in a
        terminal. Connecting changes one setting in Claude Code's settings.json, which you see
        before it is made.
      </p>
    );
  }
  return (
    <p className="note">
      Official. Updates while Claude Code runs in a terminal
      {newest > 0 ? `; last reading ${ago(newest, now)}` : "; no reading yet"}.
      {state.wrapping
        ? " Your own status line still shows, because ours runs it and passes its output through."
        : ""}
    </p>
  );
}

function EndpointText({ endpoint }: { endpoint: Endpoint }) {
  const now = useNow(30_000);
  return (
    <>
      <p className="note">
        Fills the gaps when no terminal session is running. It reads your Claude Code login without
        changing it and asks at most once every 5 minutes. The endpoint is undocumented, so it may
        stop working without notice.
        {endpoint.enabled && endpoint.askedMs && !endpoint.problem
          ? ` Last asked ${ago(endpoint.askedMs, now)}.`
          : ""}
      </p>
      {endpoint.problem && (
        <p className="note problem">
          <Warning size={14} className="warning-mark" />
          {endpoint.problem}
        </p>
      )}
    </>
  );
}

function RecentWindows({ recent }: { recent: readonly FiveHourWindow[] }) {
  const status = useStatus();
  const now = useNow(60_000);
  return (
    <Panel title="Recent five-hour windows">
      {recent.length === 0 ? (
        <p className="note">No five-hour window has a reading yet.</p>
      ) : (
        <table className="data">
          <thead>
            <tr>
              <th scope="col">Window</th>
              <th scope="col" className="left recent-share">
                Highest use
              </th>
              <th scope="col">API-equivalent cost</th>
            </tr>
          </thead>
          <tbody>
            {recent.map((window) => {
              const start = local(window.startMs, status.timeZone);
              const end = local(window.resetsAtMs, status.timeZone);
              const running = window.startMs <= now && now < window.resetsAtMs;
              const level =
                window.highest >= 90 ? "critical" : window.highest >= 70 ? "warning" : "normal";
              return (
                <tr key={window.startMs}>
                  <th scope="row" className="nowrap">
                    {dayAndTime(window.startMs, now, status.timeZone)} to{" "}
                    {end.day === start.day
                      ? end.time
                      : dayAndTime(window.resetsAtMs, now, status.timeZone)}
                    {running ? ", in progress" : ""}
                  </th>
                  <td className="left">
                    <span className="share-cell">
                      <ShareBar share={window.highest / 100} fill={LEVEL_COLOUR[level]} />
                      <span className="ink-2">{percent(window.highest)}</span>
                      {level === "critical" && (
                        <span role="img" aria-label="Over 90%" className="level-icon critical">
                          <Warning size={13} />
                        </span>
                      )}
                    </span>
                  </td>
                  <td>{window.totals.requests > 0 ? costText(window.totals) : money(0)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </Panel>
  );
}
