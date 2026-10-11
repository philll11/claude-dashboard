// What the person should know before the numbers: a first read under way, no history, a history
// this version can only read, a folder that cannot be read, and prices that may be out of date.

import { useState, useSyncExternalStore } from "react";

import type { Health, Prices, Status } from "@claude-dashboard/protocol";

import { useHost, useStatus } from "../context";
import { useReadingProgress, useRequest, useStore } from "../data";
import { fullDate, grouped } from "../format";
import { RELEASES, type Updates } from "../host";
import { Download, EmptyFolder, ErrorCircle, Lock, Warning } from "../ui/icons";
import { Notice } from "../ui/panels";
import { useView } from "../view";

export function Notices() {
  const status = useStatus();
  const health = useRequest("health", null).data;
  const prices = useRequest("prices", { models: [] }).data;
  return (
    <>
      <FirstRun status={status} />
      {health && <NoHistory status={status} health={health} />}
      {status.access === "readOnly" && <ReadOnly />}
      {status.access === "readWrite" && <ImportOffer />}
      {health && <Unreadable health={health} />}
      {prices && <PriceDrift prices={prices} />}
    </>
  );
}

/** The first read of the history, newest transcripts first. */
function FirstRun({ status }: { status: Status }) {
  const progress = useReadingProgress() ?? status.reading;
  if (!progress || status.readMs) return null;
  const share = progress.total > 0 ? progress.done / progress.total : 0;
  return (
    <section className="panel first-run" aria-labelledby="first-run-title">
      <div className="first-run-head">
        <h2 id="first-run-title">Reading your Claude Code history</h2>
        <span className="subtitle figure">
          {grouped(progress.done)} of {grouped(progress.total)} files
        </span>
      </div>
      <div
        role="progressbar"
        aria-labelledby="first-run-title"
        aria-valuemin={0}
        aria-valuemax={progress.total}
        aria-valuenow={progress.done}
        className="progress"
      >
        <div className="progress-fill" style={{ width: `${String(share * 100)}%` }} />
      </div>
      <p className="note">
        Newest first: the last few days are ready below, and older days fill in as they are read.
        You can keep using the dashboard meanwhile.
      </p>
    </section>
  );
}

function NoHistory({ status, health }: { status: Status; health: Health }) {
  const store = useStore();
  const progress = useReadingProgress() ?? status.reading;
  if (status.firstDay || progress) return null;
  const folders = health.folders.map((folder) => folder.path);
  return (
    <section className="panel empty" aria-labelledby="no-history-title">
      <span className="empty-icon">
        <EmptyFolder size={28} />
      </span>
      <h2 id="no-history-title">No Claude Code history found</h2>
      <p className="note">
        We looked in{" "}
        {folders.map((folder, index) => (
          <span key={folder}>
            {index > 0 && (index === folders.length - 1 ? " and " : ", ")}
            <span className="mono">{folder}</span>
          </span>
        ))}
        . Claude Code saves its history there after your first session. If you keep it somewhere
        else, set <span className="mono">CLAUDE_CONFIG_DIR</span> to that folder.
      </p>
      <div className="actions">
        <button
          type="button"
          className="button"
          onClick={() => {
            void store.send("refresh", null);
          }}
        >
          Check again
        </button>
      </div>
    </section>
  );
}

/** The claude-usage dashboard's history, which Claude Code may have deleted from its own. */
function ImportOffer() {
  const store = useStore();
  const offer = useRequest("import/offer", null).data;
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  if (!offer || offer.requests === 0 || offer.dismissed) return null;
  const span =
    offer.firstDay && offer.lastDay
      ? offer.firstDay === offer.lastDay
        ? ` from ${dayMonth(offer.firstDay)}`
        : ` from ${dayMonth(offer.firstDay)} to ${dayMonth(offer.lastDay)}`
      : "";
  const run = (method: "import/run" | "import/dismiss") => {
    setBusy(true);
    setProblem(null);
    store.send(method, null).then(
      () => {
        setBusy(false);
        store.refresh(() => true);
      },
      (error: unknown) => {
        setBusy(false);
        setProblem(error instanceof Error ? error.message : String(error));
      },
    );
  };
  return (
    <Notice
      tone="accent"
      icon={<Download size={18} />}
      title="History from the claude-usage dashboard"
      actions={
        <>
          <button
            type="button"
            className="button primary"
            disabled={busy}
            onClick={() => {
              run("import/run");
            }}
          >
            Import
          </button>
          <button
            type="button"
            className="button quiet"
            disabled={busy}
            onClick={() => {
              run("import/dismiss");
            }}
          >
            Not now
          </button>
        </>
      }
    >
      {grouped(offer.requests)} {offer.requests === 1 ? "request" : "requests"}
      {span} that this history doesn't hold, most likely because Claude Code has deleted them from
      its own. Importing adds them here; nothing is changed or removed in the other database.
      {problem && <span className="problem-line">{problem}</span>}
    </Notice>
  );
}

/** A day as `31 July`. */
function dayMonth(day: string): string {
  return fullDate(day).replace(/ \d{4}$/, "");
}

function ReadOnly() {
  const host = useHost();
  return (
    <Notice
      tone="caution"
      icon={<Lock size={18} />}
      title="Read-only until you update"
      actions={host.updates && <UpdateAction updates={host.updates} />}
    >
      A newer version of claude-dashboard has upgraded your history. This version can show it but
      not add to it.
      {!host.updates && (
        <>
          {" "}
          Install the newer version from the <a href={RELEASES}>releases page</a>.
        </>
      )}
    </Notice>
  );
}

/** Updating the desktop app: to the version that is ready, or else a check for one. */
function UpdateAction({ updates }: { updates: Updates }) {
  const state = useSyncExternalStore(updates.subscribe, updates.state);
  if (state.kind === "ready" || state.kind === "installing") {
    return (
      <button
        type="button"
        className="button primary"
        disabled={state.kind === "installing"}
        onClick={() => {
          updates.install();
        }}
      >
        {state.kind === "ready" ? `Update to ${state.version}` : `Updating to ${state.version}…`}
      </button>
    );
  }
  return (
    <button
      type="button"
      className="button quiet"
      disabled={state.kind === "checking"}
      onClick={() => {
        updates.check();
      }}
    >
      Check for updates
    </button>
  );
}

function Unreadable({ health }: { health: Health }) {
  const store = useStore();
  const { change } = useView();
  const problems = health.folders.filter(
    (folder) => folder.state.state === "unreadable" || folder.state.state === "unreachable",
  );
  return (
    <>
      {problems.map((folder) => (
        <Notice
          key={folder.path}
          tone="critical"
          role="alert"
          icon={<ErrorCircle size={18} />}
          title={
            <>
              Couldn't read <span className="mono">{folder.path}</span>
            </>
          }
          actions={
            <>
              <button
                type="button"
                className="button raised"
                onClick={() => {
                  void store.send("refresh", null);
                }}
              >
                Try again
              </button>
              <button
                type="button"
                className="button quiet"
                onClick={() => {
                  change({ section: "settings" });
                }}
              >
                Show details
              </button>
            </>
          }
        >
          {"reason" in folder.state ? `${capitalised(folder.state.reason)}. ` : ""}
          Everything read before stays available; new sessions appear once the folder can be read
          again.
        </Notice>
      ))}
    </>
  );
}

function capitalised(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

function PriceDrift({ prices }: { prices: Prices }) {
  const { change } = useView();
  const [first, ...others] = prices.outOfLine;
  if (!first) return null;
  const below = first.drift < 0;
  const share = Math.round(Math.abs(first.drift) * 100);
  const sessions = prices.outOfLine.reduce((sum, model) => sum + model.sessions, 0);
  const names = prices.outOfLine.map((model) => model.model.name);
  return (
    <Notice
      tone="caution"
      icon={<Warning size={18} stroke={1.75} />}
      title={`Our estimate is ${String(share)}% ${below ? "lower" : "higher"} than Claude Code's own figure`}
      actions={
        <button
          type="button"
          className="button quiet link"
          onClick={() => {
            change({ section: "sessions", model: first.model.id, session: null });
          }}
        >
          Show the sessions
        </button>
      }
    >
      This applies to {grouped(sessions)} {sessions === 1 ? "session" : "sessions"}
      {others.length === 0 ? `, all using ${first.model.name}` : ` using ${names.join(", ")}`}. The
      price table may be out of date for {others.length === 0 ? "that model" : "those models"}.
    </Notice>
  );
}
