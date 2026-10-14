import {
  type JSX,
  type MouseEvent,
  useEffect,
  useMemo,
  useState,
  useSyncExternalStore,
} from "react";

import type { Status } from "@claude-dashboard/protocol";

import { HostContext, StatusContext, useHost } from "./context";
import { Store, StoreContext, useReadingProgress, useRequest } from "./data";
import { ExportNotice, Exports, ExportsContext } from "./exports";
import { ago, fullDate } from "./format";
import { errorCode, HISTORY_TOO_NEW, type Host, type Updates } from "./host";
import { Limits } from "./sections/Limits";
import { Models } from "./sections/Models";
import { Overview } from "./sections/Overview";
import { Projects } from "./sections/Projects";
import { Sessions } from "./sections/Sessions";
import { KeepRunningNotice } from "./sections/dialogs";
import { Settings } from "./sections/Settings";
import { Strip } from "./Strip";
import {
  LimitsIcon,
  ModelsIcon,
  OverviewIcon,
  ProjectsIcon,
  SessionsIcon,
  SettingsIcon,
} from "./ui/icons";
import { Problem } from "./ui/panels";
import { toTop, VIEW_SCROLL_ID } from "./ui/scroll";
import { useNow } from "./ui/time";
import {
  rememberedView,
  shown,
  type Section,
  SECTION_NAMES,
  SECTIONS,
  type View,
  ViewContext,
  useView,
} from "./view";

/** Filters and paging start over when what they apply to changes. */
const STARTS_OVER: readonly (keyof View)[] = ["section", "range", "project", "model", "search"];

export function App({ host }: { host: Host }) {
  const store = useMemo(() => new Store(host.backend), [host]);
  const exports = useMemo(() => new Exports(host), [host]);
  const [view, setView] = useState(() => rememberedView(host.memory.read()));
  const viewState = useMemo(
    () => ({
      view,
      change: (change: Partial<View>) => {
        setView((current) => {
          const next = { ...current, ...change };
          if (STARTS_OVER.some((key) => key in change) && !("offset" in change)) next.offset = 0;
          return next;
        });
      },
    }),
    [view],
  );
  useEffect(() => {
    // Beside whatever else the dashboard keeps there, such as the strip's figures.
    const state = host.memory.read();
    host.memory.write({ ...(typeof state === "object" ? state : {}), ...view });
  }, [host, view]);
  useEffect(
    () =>
      host.onShow?.((show) => {
        setView((current) => shown(current, show));
        toTop();
      }),
    [host],
  );
  return (
    <HostContext.Provider value={host}>
      <StoreContext.Provider value={store}>
        <ExportsContext.Provider value={exports}>
          <ViewContext.Provider value={viewState}>
            <Shell />
          </ViewContext.Provider>
        </ExportsContext.Provider>
      </StoreContext.Provider>
    </HostContext.Provider>
  );
}

const BODIES: Record<Section, () => JSX.Element> = {
  overview: Overview,
  sessions: Sessions,
  projects: Projects,
  models: Models,
  limits: Limits,
  settings: Settings,
};

/** The desktop sidebar's icon for each section. */
const SECTION_ICONS: Record<Section, (props: { size?: number }) => JSX.Element> = {
  overview: OverviewIcon,
  sessions: SessionsIcon,
  projects: ProjectsIcon,
  models: ModelsIcon,
  limits: LimitsIcon,
  settings: SettingsIcon,
};

function Shell() {
  const host = useHost();
  const { view, change } = useView();
  const status = useRequest("status", null);
  const Body = BODIES[view.section];
  // On the desktop the sections are a sidebar; in VS Code, a row above the section, under the
  // strip in the view.
  const desktop = host.surface === "desktop";
  const inView = !desktop && host.placement === "view";

  const go = (section: Section) => (event: MouseEvent) => {
    event.preventDefault();
    change({ section, session: section === view.section ? null : view.session });
    toTop();
  };

  const links = (
    <div className="section-links">
      {SECTIONS.map((section) => {
        const Icon = SECTION_ICONS[section];
        return (
          <a
            key={section}
            href={`#${section}`}
            aria-current={view.section === section ? "page" : undefined}
            onClick={go(section)}
          >
            {desktop && <Icon size={16} />}
            {SECTION_NAMES[section]}
          </a>
        );
      })}
    </div>
  );

  const nav = (
    <nav aria-label="Dashboard sections" className="sections">
      {desktop ? (
        <div className="sidebar">
          <div className="brand">
            <span className="brand-name">Dashboard</span>
            <span className="brand-for">for Claude Code</span>
          </div>
          {links}
          <div className="sidebar-foot">
            {status.data && <HistoryLine status={status.data} stacked />}
            {host.updates && <UpdateLine updates={host.updates} />}
          </div>
        </div>
      ) : (
        <>
          {links}
          {status.data && <HistoryLine status={status.data} />}
        </>
      )}
    </nav>
  );
  const main = (
    <main className="body">
      {status.data ? (
        <StatusContext.Provider value={status.data}>
          <Body />
        </StatusContext.Provider>
      ) : status.error && errorCode(status.error) === HISTORY_TOO_NEW ? (
        <Problem
          title="Your history needs a newer claude-dashboard"
          detail="A newer version has changed it in a way this one cannot read. Install the newest release to see it again."
        />
      ) : status.error ? (
        <Problem
          title="The dashboard cannot reach claude-dashboard"
          detail={status.error.message}
        />
      ) : null}
    </main>
  );

  // In the view the strip stays put and the rest scrolls beneath it, under a scrollbar of its own,
  // so that nothing focused below can hide under the strip.
  if (inView) {
    return (
      <div className="shell view">
        <Strip />
        <div className="view-scroll" id={VIEW_SCROLL_ID}>
          {nav}
          {main}
        </div>
        <ExportNotice />
      </div>
    );
  }
  return (
    <div className={desktop ? "shell desktop" : "shell"}>
      {nav}
      {main}
      <ExportNotice />
      {host.desktop && <KeepRunningNotice desktop={host.desktop} />}
    </div>
  );
}

/** Where the history starts and when it was last read, or that a read is under way: on one line,
 * or `stacked` one above the other, as the desktop sidebar shows them. */
function HistoryLine({ status, stacked = false }: { status: Status; stacked?: boolean }) {
  const now = useNow(15_000);
  const progress = useReadingProgress() ?? status.reading;
  if (progress && progress.done < progress.total) {
    const files = progress.total - progress.done;
    return (
      <span className="history-line reading">
        <span className="dot" aria-hidden="true" />
        {status.readMs
          ? `Reading ${files === 1 ? "1 changed file" : `${String(files)} changed files`}…`
          : "Reading your history…"}
      </span>
    );
  }
  const since = status.firstDay ? `History since ${fullDate(status.firstDay)}` : "No history yet";
  const read = status.readMs ? ago(status.readMs, now) : undefined;
  if (stacked) {
    return (
      <span className="history-line stacked">
        <span>{since}</span>
        {read && <span>Read {read}</span>}
      </span>
    );
  }
  return <span className="history-line">{read ? `${since} · read ${read}` : since}</span>;
}

/** An update that is installing, or ready to install with a restart. */
function UpdateLine({ updates }: { updates: Updates }) {
  const state = useSyncExternalStore(updates.subscribe, updates.state);
  if (state.kind === "installing") {
    return (
      <span className="history-line reading">
        <span className="dot" aria-hidden="true" />
        Updating to version {state.version}…
      </span>
    );
  }
  if (state.kind !== "ready") return null;
  return (
    <span className="update-line">
      <span>Version {state.version} is ready</span>
      <button
        type="button"
        className="button link"
        onClick={() => {
          updates.install();
        }}
      >
        Restart to update
      </button>
    </span>
  );
}
