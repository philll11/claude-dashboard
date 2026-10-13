// Settings: the Claude Code data folders, the history, the plan limit sources, the prices, time,
// and what the dashboard does on the network.

import { type ReactNode, useEffect, useId, useState, useSyncExternalStore } from "react";

import type {
  Folder,
  Health,
  Limits as LimitsAnswer,
  PriceRefresh,
  Prices,
  Settings as SettingsAnswer,
} from "@claude-dashboard/protocol";

import { useHost, useStatus } from "../context";
import { useRequest, useStore } from "../data";
import { CancelExport, ExportMenu, ExportProgressLine, useExportState } from "../exports";
import { dayWords, dayWordsAt, fullDate, grouped } from "../format";
import { type Desktop, RELEASES, type UpdateState, type Updates } from "../host";
import { Switch } from "../ui/controls";
import { Check, Plus, Spinner, Warning } from "../ui/icons";
import { useNow } from "../ui/time";
import { Panel, Pill } from "../ui/panels";
import { SectionHead } from "./common";
import { ConsentDialog, StatusLineDialog } from "./dialogs";

const WEEKDAYS = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];

export function Settings() {
  const host = useHost();
  const health = useRequest("health", null).data;
  const limits = useRequest("limits", null).data;
  const prices = useRequest("prices", { models: [] }).data;
  const settings = useRequest("settings", null).data;
  return (
    <div className="section settings">
      <SectionHead title="Settings" />
      {health && <Folders health={health} />}
      {health && <History health={health} />}
      {limits && <PlanLimits limits={limits} />}
      {prices && <PricesPanel prices={prices} />}
      {settings && <Time settings={settings} />}
      {settings && host.desktop?.tray && (
        <NotificationArea desktop={host.desktop} settings={settings} />
      )}
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
          <button type="button" className="button compact raised" onClick={add}>
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
          <button type="button" className="button compact quiet" onClick={onRemove}>
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

/** One fact in `Facts`: its name, and what it is. */
function Fact({
  name,
  mono = false,
  children,
}: {
  name: string;
  mono?: boolean;
  children: ReactNode;
}) {
  return (
    <div>
      <dt>{name}</dt>
      <dd className={mono ? "mono" : undefined}>{children}</dd>
    </div>
  );
}

function History({ health }: { health: Health }) {
  const status = useStatus();
  const host = useHost();
  const store = useStore();
  const offer = useRequest("import/offer", null).data;
  const exporting = useExportState();
  const [change, problem] = useChange();
  const facts = health.history;
  const exportingAll = exporting.kind === "running" && exporting.dataset === "requests";
  return (
    <Panel title="History">
      <Facts>
        {health.historyPath && (
          <Fact name="Stored in" mono>
            {health.historyPath}
          </Fact>
        )}
        {facts && (
          <Fact name="Covers">
            {status.firstDay ? `${fullDate(status.firstDay)} to today` : "Nothing yet"} ·{" "}
            {grouped(facts.requests)} {facts.requests === 1 ? "request" : "requests"} in{" "}
            {grouped(facts.sessions)} {facts.sessions === 1 ? "session" : "sessions"} ·{" "}
            {megabytes(facts.bytes)}
          </Fact>
        )}
        {facts?.imported && (
          <Fact name="Imported">
            {grouped(facts.imported.requests)} requests from the claude-usage dashboard on{" "}
            {fullDate(new Date(facts.imported.importedMs).toISOString().slice(0, 10))}
          </Fact>
        )}
        <Fact name="Claude Code clean-up">
          Claude Code deletes transcripts after {grouped(health.cleanupDays)} days; everything read
          before then stays here.
        </Fact>
      </Facts>
      <div className="actions">
        {health.historyPath && host.reveal && (
          <button
            type="button"
            className="button compact quiet"
            onClick={() => {
              if (health.historyPath) host.reveal?.(health.historyPath);
            }}
          >
            Show in folder
          </button>
        )}
        {facts && facts.requests > 0 && !exportingAll && (
          <ExportMenu
            what={{ dataset: "requests", filters: {} }}
            scope={`All ${grouped(facts.requests)} ${facts.requests === 1 ? "request" : "requests"}${
              status.firstDay ? ` since ${fullDate(status.firstDay)}` : ""
            }`}
            className="button compact quiet"
            align="start"
          >
            Export all history…
          </ExportMenu>
        )}
        {exportingAll && (
          <>
            <button type="button" className="button compact quiet busy" disabled aria-busy="true">
              <Spinner className="spin" />
              Exporting all history…
            </button>
            <CancelExport />
          </>
        )}
        {offer && offer.requests > 0 && (
          <button
            type="button"
            className="button compact raised"
            onClick={() => {
              change(() => store.send("import/run", null));
            }}
          >
            Import {grouped(offer.requests)} requests from claude-usage
          </button>
        )}
      </div>
      {exportingAll && <ExportProgressLine label="Exporting all history" />}
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
              className="button compact quiet"
              onClick={() => {
                setDialog("disconnect");
              }}
            >
              Disconnect
            </button>
          ) : statusLine.state === "notConnected" ? (
            <button
              type="button"
              className="button compact raised"
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
  const { timeZone } = useStatus();
  const now = useNow(60_000);
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
        <Fact name="Price table">
          {fullDate(prices.checked)}, from Anthropic's pricing page
          {prices.downloadedMs != null &&
            `, downloaded ${dayWords(prices.downloadedMs, now, timeZone)} from this project's repository`}
        </Fact>
        <Fact name="Check">{check}</Fact>
      </Facts>
      <PriceRefreshRow refresh={prices.refresh} />
    </Panel>
  );
}

/** What came of the last check for a newer price table, as the end of a sentence. */
function outcomeWords(refresh: PriceRefresh): string | undefined {
  switch (refresh.outcome) {
    case "updated":
      return "a newer price table arrived";
    case "up-to-date":
      return "up to date";
    case "unreachable":
      return `cannot reach this project's repository${refresh.detail ? ` (${refresh.detail})` : ""}`;
    case "newer-format":
      return "the newest price table needs a newer version of claude-dashboard";
    case "refused":
      return `the newest price table cannot be used${refresh.detail ? `: ${refresh.detail}` : ""}`;
    default:
      return undefined;
  }
}

/** The switch for the daily check, with when it last ran and what came of it. */
function PriceRefreshRow({ refresh }: { refresh: PriceRefresh }) {
  const status = useStatus();
  const store = useStore();
  const now = useNow(60_000);
  const [change, problem] = useChange();
  const nameId = useId();
  const textId = useId();
  const outcome = outcomeWords(refresh);
  const last =
    refresh.checkedMs != null && outcome
      ? ` Last checked ${dayWordsAt(refresh.checkedMs, now, status.timeZone)}: ${outcome}.`
      : "";
  return (
    <div className="setting-row">
      <span className="setting-text">
        <span className="setting-name" id={nameId}>
          Check for a newer price table once a day
        </span>
        <span className="ink-2" id={textId}>
          Downloads it from this project's repository on GitHub, at most once a day.{last}
        </span>
        {problem && <span className="note problem">{problem}</span>}
      </span>
      <Switch
        checked={refresh.enabled}
        labelledBy={nameId}
        describedBy={textId}
        onChange={(on) => {
          change(() => store.send("settings/set", { priceRefresh: on }));
        }}
      />
    </div>
  );
}

function Time({ settings }: { settings: SettingsAnswer }) {
  const store = useStore();
  const [change, problem] = useChange();
  const zoneId = useId();
  const weekId = useId();
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

/** An on and off setting, with its name and what it does. */
function SwitchRow({
  name,
  text,
  checked,
  disabled = false,
  onChange,
}: {
  name: string;
  text: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (on: boolean) => void;
}) {
  const nameId = useId();
  const textId = useId();
  return (
    <div className="setting-row">
      <span className="setting-text">
        <span className="setting-name" id={nameId}>
          {name}
        </span>
        <span className="ink-2" id={textId}>
          {text}
        </span>
      </span>
      <Switch
        checked={checked}
        disabled={disabled}
        labelledBy={nameId}
        describedBy={textId}
        onChange={onChange}
      />
    </div>
  );
}

/** The desktop app's item in the notification area or the menu bar: whether it shows, whether
 * the app keeps running in it when the window closes, and whether the app opens at login. */
function NotificationArea({ desktop, settings }: { desktop: Desktop; settings: SettingsAnswer }) {
  const store = useStore();
  const [change, problem] = useChange();
  const [autostart, setAutostart] = useState<boolean | undefined>(undefined);
  const [autostartProblem, setAutostartProblem] = useState<string | null>(null);
  useEffect(() => {
    let current = true;
    desktop.autostart().then(
      (on) => {
        if (current) setAutostart(on);
      },
      (error: unknown) => {
        if (current) setAutostartProblem(error instanceof Error ? error.message : String(error));
      },
    );
    return () => {
      current = false;
    };
  }, [desktop]);
  const place = desktop.trayPlace;
  return (
    <Panel title={place.charAt(0).toUpperCase() + place.slice(1)}>
      <SwitchRow
        name={`Show the five-hour window in the ${place}`}
        text="Its icon shows how much of the window you have used, and its menu lists every plan window and opens the dashboard."
        checked={settings.tray}
        onChange={(tray) => {
          change(() => store.send("settings/set", { tray }));
        }}
      />
      <SwitchRow
        name="Keep running when the window closes"
        text="Quit from the icon's menu. Updates wait there until you restart."
        checked={settings.tray && settings.keepRunning}
        disabled={!settings.tray}
        onChange={(keepRunning) => {
          change(() => store.send("settings/set", { keepRunning }));
        }}
      />
      <SwitchRow
        name="Open at login"
        text={`Starts in the ${place}, without the window.`}
        checked={autostart ?? false}
        disabled={autostart === undefined}
        onChange={(on) => {
          setAutostart(on);
          setAutostartProblem(null);
          desktop.setAutostart(on).catch((error: unknown) => {
            setAutostart(!on);
            setAutostartProblem(error instanceof Error ? error.message : String(error));
          });
        }}
      />
      {(problem ?? autostartProblem) && (
        <p className="note problem">{problem ?? autostartProblem}</p>
      )}
    </Panel>
  );
}

function Privacy() {
  const host = useHost();
  const status = useStatus();
  return (
    <Panel title="Updates and privacy">
      {host.updates ? (
        <UpdatesRow updates={host.updates} version={status.version} />
      ) : (
        <p className="note">
          Version {status.version}.
          {host.surface === "vscode" && (
            <>
              {" "}
              New versions are on the <a href={RELEASES}>releases page</a>.
            </>
          )}
        </p>
      )}
      <p className="note">
        claude-dashboard collects no telemetry. It never stores prompt or code content, and it only
        uses the network for the features switched on above.
      </p>
    </Panel>
  );
}

/** Where the desktop app's updates stand, after the version, in the Settings board's words. */
function updateWords(state: UpdateState): string {
  switch (state.kind) {
    case "idle":
      return "updates are signed and checked before installing";
    case "checking":
      return "checking for a newer version…";
    case "current":
      return "up to date · updates are signed and checked before installing";
    case "ready":
      return `version ${state.version} is ready to install`;
    case "installing":
      return `installing version ${state.version}…`;
    case "failed":
      return "cannot check for updates now";
  }
}

/** The desktop app's updates: whether they install automatically, and where they stand. */
function UpdatesRow({ updates, version }: { updates: Updates; version: string }) {
  const state = useSyncExternalStore(updates.subscribe, updates.state);
  const automatic = useSyncExternalStore(updates.subscribe, updates.automatic);
  const nameId = useId();
  const textId = useId();
  const canCheck =
    state.kind === "failed" || (!automatic && (state.kind === "idle" || state.kind === "current"));
  return (
    <div className="setting-row">
      <span className="setting-text">
        <span className="setting-name" id={nameId}>
          Install updates automatically
        </span>
        <span className="ink-2" id={textId}>
          Version {version} · {updateWords(state)}.
        </span>
        {state.kind === "failed" && (
          <span className="note problem">
            <Warning size={14} className="warning-mark" />
            {state.reason}
          </span>
        )}
        {(state.kind === "ready" || canCheck) && (
          <span className="actions setting-actions">
            {state.kind === "ready" ? (
              <button
                type="button"
                className="button compact raised"
                onClick={() => {
                  updates.install();
                }}
              >
                Restart to update
              </button>
            ) : (
              <button
                type="button"
                className="button compact quiet"
                onClick={() => {
                  updates.check();
                }}
              >
                Check now
              </button>
            )}
          </span>
        )}
      </span>
      <Switch
        checked={automatic}
        labelledBy={nameId}
        describedBy={textId}
        onChange={(on) => {
          updates.setAutomatic(on);
        }}
      />
    </div>
  );
}
