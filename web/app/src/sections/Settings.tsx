// Settings: the Claude Code data folders, the history, the plan limit sources, the prices, time,
// and what the dashboard does on the network.

import { type ReactNode, useId, useState } from "react";

import type { Folder, Health, Limits as LimitsAnswer, Prices } from "@claude-dashboard/protocol";

import { useHost, useStatus } from "../context";
import { useRequest, useStore } from "../data";
import { fullDate, grouped } from "../format";
import { Switch } from "../ui/controls";
import { Check, Plus, Warning } from "../ui/icons";
import { Panel, Pill } from "../ui/panels";
import { SectionHead } from "./common";
import { ConsentDialog, StatusLineDialog } from "./dialogs";

const WEEKDAYS = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];

export function Settings() {
  const health = useRequest("health", null).data;
  const limits = useRequest("limits", null).data;
  const prices = useRequest("prices", { models: [] }).data;
  return (
    <div className="section settings">
      <SectionHead title="Settings" />
      {health && <Folders health={health} />}
      {health && <History health={health} />}
      {limits && <PlanLimits limits={limits} />}
      {prices && <PricesPanel prices={prices} />}
      <Time />
      <Privacy />
    </div>
  );
}

/** Sends a change, then asks again for whatever it affects; says what went wrong if it fails. */
function useChange(): [(send: () => Promise<unknown>) => void, string | null] {
  const store = useStore();
  const [problem, setProblem] = useState<string | null>(null);
  const change = (send: () => Promise<unknown>) => {
    setProblem(null);
    send().then(
      () => {
        store.refresh(() => true);
      },
      (error: unknown) => {
        setProblem(error instanceof Error ? error.message : String(error));
      },
    );
  };
  return [change, problem];
}

function megabytes(bytes: number): string {
  if (bytes < 1_000_000) return `${grouped(Math.max(1, Math.round(bytes / 1_000)))} KB`;
  return `${grouped(Math.round(bytes / 1_000_000))} MB`;
}

const SOURCES: Record<string, string> = {
  default: "Default",
  environment: "From CLAUDE_CONFIG_DIR",
  added: "Added by you",
};

function Folders({ health }: { health: Health }) {
  const host = useHost();
  const store = useStore();
  const [change, problem] = useChange();
  const add = () => {
    void host.pickFolder?.().then((path) => {
      if (path) change(() => store.send("folders/add", { path }));
    });
  };
  return (
    <Panel
      title="Claude Code data folders"
      subtitle="Where to read transcripts from. CLAUDE_CONFIG_DIR is used when it is set."
      actions={
        host.pickFolder ? (
          <button type="button" className="button small raised" onClick={add}>
            <Plus />
            Add folder
          </button>
        ) : undefined
      }
    >
      <ul className="folders">
        {health.folders.map((folder) => (
          <FolderRow
            key={folder.path}
            folder={folder}
            onRemove={() => {
              change(() => store.send("folders/remove", { path: folder.path }));
            }}
          />
        ))}
      </ul>
      {problem && <p className="note problem">{problem}</p>}
    </Panel>
  );
}

function FolderRow({ folder, onRemove }: { folder: Folder; onRemove: () => void }) {
  const state = folder.state;
  const detail =
    state.state === "found"
      ? `${grouped(state.transcripts)} ${state.transcripts === 1 ? "transcript" : "transcripts"} · ${megabytes(state.bytes)}`
      : state.state === "missing"
        ? "Claude Code has not saved anything here yet"
        : state.reason;
  return (
    <li className="folder">
      <span className="folder-text">
        <span className="mono folder-path">{folder.path}</span>
        <span className="subtitle">
          {SOURCES[folder.source] ?? folder.source} · {detail}
        </span>
      </span>
      <span className="folder-state">
        {state.state === "found" ? (
          <span className="state-line">
            <Check size={13} />
            Found
          </span>
        ) : state.state === "missing" ? (
          <span className="state-line">Not found</span>
        ) : (
          <span className="state-line strong-text">
            <Warning size={13} className="warning-mark" />
            {state.state === "unreachable" ? "Not reachable" : "Can't be read"}
          </span>
        )}
        {folder.source === "added" && (
          <button type="button" className="button small quiet" onClick={onRemove}>
            Remove
          </button>
        )}
      </span>
    </li>
  );
}

function Facts({ children }: { children: ReactNode }) {
  return <dl className="facts">{children}</dl>;
}

function History({ health }: { health: Health }) {
  const status = useStatus();
  const host = useHost();
  const store = useStore();
  const offer = useRequest("import/offer", null).data;
  const [change, problem] = useChange();
  const facts = health.history;
  return (
    <Panel title="History">
      <Facts>
        {health.historyPath && (
          <>
            <dt>Stored in</dt>
            <dd className="mono">{health.historyPath}</dd>
          </>
        )}
        {facts && (
          <>
            <dt>Covers</dt>
            <dd>
              {status.firstDay ? `${fullDate(status.firstDay)} to today` : "Nothing yet"} ·{" "}
              {grouped(facts.requests)} {facts.requests === 1 ? "request" : "requests"} in{" "}
              {grouped(facts.sessions)} {facts.sessions === 1 ? "session" : "sessions"} ·{" "}
              {megabytes(facts.bytes)}
            </dd>
          </>
        )}
        {facts?.imported && (
          <>
            <dt>Imported</dt>
            <dd>
              {grouped(facts.imported.requests)} requests from the claude-usage dashboard on{" "}
              {fullDate(new Date(facts.imported.importedMs).toISOString().slice(0, 10))}
            </dd>
          </>
        )}
        <dt>Claude Code clean-up</dt>
        <dd>
          Claude Code deletes transcripts after {grouped(health.cleanupDays)} days; everything read
          before then stays here.
        </dd>
      </Facts>
      <div className="actions">
        {health.historyPath && host.reveal && (
          <button
            type="button"
            className="button small quiet"
            onClick={() => {
              if (health.historyPath) host.reveal?.(health.historyPath);
            }}
          >
            Show in folder
          </button>
        )}
        {offer && offer.requests > 0 && (
          <button
            type="button"
            className="button small raised"
            onClick={() => {
              change(() => store.send("import/run", null));
            }}
          >
            Import {grouped(offer.requests)} requests from claude-usage
          </button>
        )}
      </div>
      {problem && <p className="note problem">{problem}</p>}
    </Panel>
  );
}

function PlanLimits({ limits }: { limits: LimitsAnswer }) {
  const store = useStore();
  const [dialog, setDialog] = useState<"connect" | "disconnect" | "consent" | null>(null);
  const [change, problem] = useChange();
  const endpointName = useId();
  const endpointText = useId();
  const statusLine = limits.statusLine;
  return (
    <Panel title="Plan limits">
      <div className="setting-row bordered">
        <span className="setting-text">
          <span className="setting-name">Claude Code's status line</span>
          <span className="ink-2">
            {statusLine.state === "connected"
              ? `Official. Connected in Claude Code's settings.json${
                  statusLine.wrapping ? ", running your own status line after it" : ""
                }.`
              : statusLine.state === "notConnected"
                ? "Official. Not connected: connecting changes one setting in Claude Code's settings.json, shown before it is made."
                : `claude-dashboard can't tell whether it is connected: ${statusLine.reason}.`}
          </span>
        </span>
        <span className="actions">
          {statusLine.state === "connected" ? (
            <button
              type="button"
              className="button small quiet"
              onClick={() => {
                setDialog("disconnect");
              }}
            >
              Disconnect
            </button>
          ) : statusLine.state === "notConnected" ? (
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
        </span>
      </div>
      <div className="setting-row">
        <span className="setting-text">
          <span className="setting-name" id={endpointName}>
            Anthropic's usage endpoint <Pill tone="caution">Experimental</Pill>
          </span>
          <span className="ink-2" id={endpointText}>
            Reads your Claude Code login without changing it and asks at most once every 5 minutes.
            Undocumented, so it may stop working without notice.
          </span>
          {limits.endpoint.problem && (
            <span className="note problem">
              <Warning size={14} className="warning-mark" />
              {limits.endpoint.problem}
            </span>
          )}
        </span>
        <Switch
          checked={limits.endpoint.enabled}
          labelledBy={endpointName}
          describedBy={endpointText}
          onChange={(on) => {
            if (on) setDialog("consent");
            else change(() => store.send("endpoint/set", { enabled: false }));
          }}
        />
      </div>
      {problem && <p className="note problem">{problem}</p>}
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

function PricesPanel({ prices }: { prices: Prices }) {
  const drift = prices.drift;
  const check =
    drift === null || drift === undefined ? (
      <span>No session can be compared yet</span>
    ) : prices.outOfLine.length > 0 ? (
      <span className="state-line strong-text">
        <Warning size={13} className="warning-mark" />
        Out of line for {prices.outOfLine.map((row) => row.model.name).join(", ")}: the price table
        may be out of date
      </span>
    ) : (
      <span className="state-line">
        <Check size={13} />
        Within {Math.max(0.1, Math.abs(drift) * 100).toFixed(1)}% of Claude Code's own session
        costs, over {grouped(prices.sessionsCompared)}{" "}
        {prices.sessionsCompared === 1 ? "session" : "sessions"}
      </span>
    );
  return (
    <Panel title="Prices">
      <Facts>
        <dt>Price table</dt>
        <dd>{fullDate(prices.checked)}, from Anthropic's pricing page</dd>
        <dt>Check</dt>
        <dd>{check}</dd>
      </Facts>
    </Panel>
  );
}

function Time() {
  const settings = useRequest("settings", null).data;
  const store = useStore();
  const [change, problem] = useChange();
  const zoneId = useId();
  const weekId = useId();
  if (!settings) return null;
  const zones = (() => {
    try {
      return Intl.supportedValuesOf("timeZone");
    } catch {
      return [];
    }
  })();
  const chosen = settings.timeZone ?? "";
  return (
    <Panel title="Time">
      <div className="time-fields">
        <label className="field" htmlFor={zoneId}>
          <span className="ink-2">Time zone</span>
          <select
            id={zoneId}
            value={chosen}
            onChange={(event) => {
              const timeZone = event.target.value;
              change(() => store.send("settings/set", { timeZone }));
            }}
          >
            <option value="">{settings.systemTimeZone} (this computer)</option>
            {zones
              .filter((zone) => zone !== settings.systemTimeZone || chosen === zone)
              .map((zone) => (
                <option key={zone} value={zone}>
                  {zone}
                </option>
              ))}
          </select>
        </label>
        <label className="field" htmlFor={weekId}>
          <span className="ink-2">First day of the week</span>
          <select
            id={weekId}
            value={settings.weekStart}
            onChange={(event) => {
              const weekStart = event.target.value;
              change(() => store.send("settings/set", { weekStart }));
            }}
          >
            {WEEKDAYS.map((day) => (
              <option key={day} value={day}>
                {day.charAt(0).toUpperCase() + day.slice(1)}
              </option>
            ))}
          </select>
        </label>
      </div>
      {problem && <p className="note problem">{problem}</p>}
    </Panel>
  );
}

function Privacy() {
  const host = useHost();
  const status = useStatus();
  return (
    <Panel title="Updates and privacy">
      <p className="note">
        Version {status.version}.
        {host.surface === "vscode" ? " VS Code keeps the extension up to date." : ""}
      </p>
      <p className="note">
        claude-dashboard collects no telemetry. It never stores prompt or code content, and it only
        uses the network for the features switched on above.
      </p>
    </Panel>
  );
}
