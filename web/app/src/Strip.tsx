// The strip at the top of VS Code's view: the five-hour and weekly windows, and the latest session
// in the project VS Code has open, in sight whichever section is shown below. The view does not
// keep running while hidden, so the strip keeps what it shows in the view's state, paints that at
// once when VS Code shows the view again, and brings it up to date as the answers come.

import { type MouseEvent, type ReactNode, useEffect, useState, useSyncExternalStore } from "react";

import type {
  OpenFolders,
  Progress,
  SessionsParams,
  SessionRow,
  StatusLine,
  Totals,
  WindowReading,
} from "@claude-dashboard/protocol";

import { Meter } from "./charts/small";
import { useHost } from "./context";
import { useReadingProgress, useRequest } from "./data";
import { addDays, ago, compact, duration, grouped, local, percent, resets, when } from "./format";
import type { Host } from "./host";
import { costText, tokensOf } from "./sections/common";
import { StatusLineDialog } from "./sections/dialogs";
import { LevelIcon } from "./sections/provenance";
import { isRunning, latestSession } from "./sessions";
import { Clock, Warning } from "./ui/icons";
import { toTop } from "./ui/scroll";
import { useNow } from "./ui/time";
import { useView } from "./view";
import { inOrder, LEVEL_TEXT, meterName, windowName } from "./windows";

/** The sessions the latest is chosen from: enough for two busy days. */
const SESSIONS = 200;

/** How often, at most, the whole session's figures are asked for again while its requests come in:
 * the server adds up every request of a long session to answer. */
const WHOLE_EVERY_MS = 10_000;

const DAY_MS = 24 * 60 * 60_000;

/** What the strip shows, which it keeps in the view's state between showings. */
interface Shown {
  timeZone: string;
  /** The five-hour and weekly windows, as far as they have readings. */
  windows: WindowReading[];
  /** When Claude Code last passed a reading of any window. */
  lastReadingMs?: number | undefined;
  /** The status line's connection, which the strip tells while there are no readings. */
  statusLine?: StatusLine["state"] | undefined;
  /** Whether Claude Code's login has expired, so that the usage endpoint cannot be asked. */
  loginExpired?: boolean | undefined;
  /** A first read of the history under way. */
  firstRead?: Progress | undefined;
  /** The latest session; `null` when there is none. */
  session?: ShownSession | null | undefined;
}

interface ShownSession {
  row: SessionRow;
  /** Whether its project is one VS Code has open. */
  here: boolean;
  /** The figures of the whole session, which the row holds only for the days it covers. */
  whole?: Whole | undefined;
}

interface Whole {
  firstMs: number;
  totals: Totals;
}

/** The shape of `Shown` that the view's state holds; a change to `Shown` needs a new one, so that
 * the strip never paints from a shape it no longer reads. */
const KEPT_AS = 1;

/** What the strip showed when the view was last shown. */
function remembered(state: unknown): Shown | undefined {
  if (state === null || typeof state !== "object" || !("strip" in state)) return undefined;
  const kept = state.strip as { as?: unknown; shown?: unknown } | null | undefined;
  return kept?.as === KEPT_AS ? (kept.shown as Shown) : undefined;
}

/** Keeps `shown` in the view's state, beside what the rest of the dashboard keeps there. */
function remember(host: Host, shown: Shown): void {
  const state = host.memory.read();
  host.memory.write({
    ...(typeof state === "object" ? state : {}),
    strip: { as: KEPT_AS, shown },
  });
}

const NO_FOLDERS = () => undefined;
const NO_CHANGES = () => () => undefined;

export function Strip() {
  const host = useHost();
  const [seed] = useState(() => remembered(host.memory.read()));
  const status = useRequest("status", null).data;
  const progress = useReadingProgress();
  const folders = useSyncExternalStore(
    host.folders?.subscribe ?? NO_CHANGES,
    host.folders?.current ?? NO_FOLDERS,
  );
  // The five-hour and weekly windows, or for a plan without them, the first windows it has.
  const windows = status ? inOrder(status.limits).slice(0, 2) : undefined;
  const explain =
    windows !== undefined &&
    (windows.length === 0 || windows.some((reading) => reading.stale && !reading.expired));
  const limits = useRequest("limits", null, explain).data;
  const page = useRequest("sessions", latestParams(status?.today), status !== undefined).data;
  const latest = page && folders ? (latestSession(page.rows, folders) ?? null) : undefined;
  const whole = useWhole(
    latest?.row.id,
    seed?.session && seed.session.row.id === latest?.row.id ? seed.session.whole : undefined,
  );
  const reading = progress ?? status?.reading;

  const fresh: Partial<Shown> = {};
  if (status && windows) {
    fresh.timeZone = status.timeZone;
    fresh.windows = windows;
    if (status.limits.length > 0) {
      fresh.lastReadingMs = Math.max(...status.limits.map((each) => each.observedMs));
    }
    if (!status.readMs && reading && reading.done < reading.total) fresh.firstRead = reading;
  }
  if (limits) {
    fresh.statusLine = limits.statusLine.state;
    fresh.loginExpired = limits.endpoint.enabled && limits.endpoint.outcome === "login-expired";
  }
  if (latest !== undefined) {
    fresh.session = latest && { ...latest, ...(whole ? { whole } : {}) };
  }
  const shown: Shown | undefined =
    fresh.timeZone !== undefined && fresh.windows
      ? {
          ...fresh,
          timeZone: fresh.timeZone,
          windows: fresh.windows,
          statusLine: fresh.statusLine ?? seed?.statusLine,
          loginExpired: fresh.loginExpired ?? seed?.loginExpired,
          session: fresh.session !== undefined ? fresh.session : seed?.session,
        }
      : seed;

  const kept = status && shown ? JSON.stringify(shown) : undefined;
  useEffect(() => {
    if (kept) remember(host, JSON.parse(kept) as Shown);
  }, [host, kept]);

  if (!shown) return null;
  return <StripCells shown={shown} folders={folders} />;
}

/** The sessions of yesterday and today, so that midnight does not empty the strip. */
function latestParams(today: string | undefined): SessionsParams {
  const last = today ?? "";
  return {
    days: { first: today ? addDays(today, -1) : "", last },
    filters: { project: null, model: null },
    search: null,
    offset: 0,
    limit: SESSIONS,
  };
}

/** The whole session's figures, asked for when the history changes, at most every 10 seconds;
 * `initial` until the first answer. */
function useWhole(id: string | undefined, initial: Whole | undefined): Whole | undefined {
  const host = useHost();
  const [answer, setAnswer] = useState<{ id: string; whole: Whole }>();
  useEffect(() => {
    if (id === undefined) return;
    let stopped = false;
    let asking = false;
    let again = false;
    let lastMs = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const ask = () => {
      if (stopped) return;
      if (asking) {
        again = true;
        return;
      }
      const wait = lastMs + WHOLE_EVERY_MS - Date.now();
      if (wait > 0) {
        timer ??= setTimeout(() => {
          timer = undefined;
          ask();
        }, wait);
        return;
      }
      asking = true;
      lastMs = Date.now();
      host.backend
        .request("session", { id })
        .then(
          (detail) => {
            if (!stopped)
              setAnswer({ id, whole: { firstMs: detail.firstMs, totals: detail.totals } });
          },
          () => {
            // The row's figures stand until the next answer.
          },
        )
        .finally(() => {
          asking = false;
          if (again) {
            again = false;
            ask();
          }
        });
    };
    ask();
    const stop = host.backend.onNotification("history/changed", ask);
    return () => {
      stopped = true;
      stop();
      clearTimeout(timer);
    };
  }, [host, id]);
  return answer !== undefined && answer.id === id ? answer.whole : initial;
}

function StripCells({ shown, folders }: { shown: Shown; folders: OpenFolders | undefined }) {
  const now = useNow(30_000);
  return (
    <section aria-label="Plan windows and the latest session" className="strip">
      {shown.windows.length > 0 ? (
        shown.windows.map((reading, index) => (
          <WindowCell
            key={reading.window}
            reading={reading}
            first={index === 0}
            shown={shown}
            now={now}
          />
        ))
      ) : (
        <LimitsCell statusLine={shown.statusLine} />
      )}
      {shown.firstRead ? (
        <FirstRead progress={shown.firstRead} />
      ) : (
        <SessionCell
          session={shown.session}
          now={now}
          inFolder={(folders?.folders.length ?? 0) > 0}
        />
      )}
    </section>
  );
}

function WindowCell({
  reading,
  first,
  shown,
  now,
}: {
  reading: WindowReading;
  /** The first window, which also says where the readings come from or why they are old. */
  first: boolean;
  shown: Shown;
  now: number;
}) {
  const old = reading.stale && !reading.expired;
  const at = local(reading.observedMs, shown.timeZone).time;
  const level = reading.expired ? undefined : LEVEL_TEXT[reading.level];
  const used = reading.expired ? 0 : reading.usedPercentage;
  const facts: ReactNode[] = [];
  if (level) {
    facts.push(
      <span className="level">
        <LevelIcon level={reading.level} />
        {level}
      </span>,
    );
  }
  if (old) {
    facts.push(
      <span className="level">
        <Clock size={13} className="caution-mark" />
        {duration(now - reading.observedMs)} old
      </span>,
    );
  }
  if (reading.expired) {
    facts.push("No reading since it reset");
  } else if (old && first) {
    const last = local(shown.lastReadingMs ?? reading.observedMs, shown.timeZone).time;
    facts.push(
      shown.loginExpired
        ? "Claude Code's login has expired"
        : `Claude Code hasn't run in a terminal since ${last}`,
    );
  } else {
    facts.push(
      reading.resetsAtMs ? resetsText(reading.resetsAtMs, now, shown.timeZone) : "No reset time",
    );
    if (first && !old) facts.push(`${reading.provenance}, ${ago(reading.observedMs, now)}`);
  }
  return (
    <div className="strip-cell window">
      <div className="strip-head">
        <span className="strip-name">{windowName(reading)}</span>
        <span className="strip-figures">
          {old && <span className="strip-at">at {at}</span>}
          <span
            className={reading.stale || reading.expired ? "strip-figure stale" : "strip-figure"}
          >
            {reading.expired ? "Reset" : percent(reading.usedPercentage)}
          </span>
        </span>
      </div>
      <Meter
        used={used}
        level={reading.level}
        stale={reading.stale}
        label={`${meterName(reading)}, ${percent(used)} used`}
      />
      <Facts facts={facts} />
    </div>
  );
}

/** Facts one after another, each but the last followed by a dot, so that a line never starts with
 * one where the facts wrap. */
function Facts({ facts }: { facts: readonly ReactNode[] }) {
  return (
    <div className="strip-foot">
      {facts.map((fact, index) => (
        // The facts keep their order, so their places name them.
        <span key={index} className="strip-fact">
          {fact}
          {index < facts.length - 1 && (
            <span aria-hidden="true" className="strip-dot">
              ·
            </span>
          )}
        </span>
      ))}
    </div>
  );
}

/** When a window resets: `Resets 14:20, in 2 h 05 min` within a day, `Resets Thu 09:00` within a
 * week. */
function resetsText(atMs: number, nowMs: number, timeZone: string): string {
  return atMs > nowMs && atMs - nowMs < DAY_MS
    ? resets(atMs, nowMs, timeZone)
    : `Resets ${when(atMs, nowMs, timeZone)}`;
}

/** No readings yet: whether the status line is connected, or a way to connect it. */
function LimitsCell({ statusLine }: { statusLine: StatusLine["state"] | undefined }) {
  const [connecting, setConnecting] = useState(false);
  return (
    <div className="strip-cell setup">
      <div className="strip-setup-text">
        <span className="strip-setup-title">Plan limits</span>
        {statusLine === "connected" ? (
          <span className="strip-setup-detail">
            Claude Code's status line is connected. Your five-hour and weekly windows appear here
            the next time Claude Code runs in a terminal.
          </span>
        ) : statusLine ? (
          <span className="strip-setup-detail">
            Connect Claude Code's status line to see your five-hour and weekly windows.
          </span>
        ) : null}
      </div>
      {statusLine && statusLine !== "connected" && (
        <button
          type="button"
          className="button primary compact"
          onClick={() => {
            setConnecting(true);
          }}
        >
          Connect the status line…
        </button>
      )}
      {connecting && (
        <StatusLineDialog
          connect
          onClose={() => {
            setConnecting(false);
          }}
        />
      )}
    </div>
  );
}

function FirstRead({ progress }: { progress: Progress }) {
  return (
    <div className="strip-cell session">
      <div className="strip-head">
        <span>Reading your Claude Code history</span>
        <span className="strip-count">
          {grouped(progress.done)} of {grouped(progress.total)} files
        </span>
      </div>
      <div
        role="progressbar"
        aria-label="Reading your history"
        aria-valuemin={0}
        aria-valuemax={progress.total}
        aria-valuenow={progress.done}
        className="progress"
      >
        <div
          className="progress-fill"
          style={{
            width: `${String(progress.total > 0 ? (progress.done / progress.total) * 100 : 0)}%`,
          }}
        />
      </div>
    </div>
  );
}

function SessionCell({
  session,
  now,
  inFolder,
}: {
  session: ShownSession | null | undefined;
  now: number;
  /** Whether VS Code has a folder open. */
  inFolder: boolean;
}) {
  const { change } = useView();
  if (session === undefined) return <div className="strip-cell session" />;
  if (session === null) {
    return (
      <div className="strip-cell session">
        <span>No Claude Code session today yet.</span>
        <span className="strip-detail">
          It shows here as soon as one starts{inFolder ? " in this folder" : ""}.
        </span>
      </div>
    );
  }
  const { row, here, whole } = session;
  // The row's figures are the whole session's when its first request is the session's first;
  // they are newer than the whole figures, which are asked for at most every 10 seconds.
  const covered = !whole || whole.firstMs >= row.firstMs;
  const totals = covered ? row.totals : whole.totals;
  const firstMs = covered ? row.firstMs : whole.firstMs;
  const running = isRunning(row, now);
  const open = (event: MouseEvent) => {
    event.preventDefault();
    change({ section: "sessions", session: row.id });
    toTop();
  };
  const model = row.models[0];
  return (
    <div className="strip-cell session">
      <div className="strip-head">
        <span className="strip-name">
          {here ? (
            "Latest session in this project"
          ) : (
            <>
              Latest session · <span className="mono strip-project">{row.project.name}</span>
            </>
          )}
        </span>
        <span className="strip-state">
          {running && <span className="running-dot" aria-hidden="true" />}
          {running
            ? `Running · ${duration(now - firstMs)}`
            : `Ended ${ago(row.lastMs, now)} · ran ${duration(row.lastMs - firstMs)}`}
        </span>
      </div>
      <a href={`#session-${row.id}`} className="strip-title" onClick={open}>
        {row.title ?? "Untitled session"}
      </a>
      <Facts
        facts={[
          costText(totals) === "Unpriced" ? (
            <span className="level">
              <Warning size={13} className="caution-mark" />
              Unpriced
            </span>
          ) : (
            <span>
              <span className="strip-cost">{costText(totals)}</span> API-equivalent
            </span>
          ),
          `${grouped(totals.requests)} ${totals.requests === 1 ? "request" : "requests"}`,
          `${compact(tokensOf(totals))} tokens`,
          ...(model ? [model.name] : []),
        ]}
      />
    </div>
  );
}
