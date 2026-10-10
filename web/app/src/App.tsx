import { type JSX, type MouseEvent, useEffect, useMemo, useState } from "react";

import type { Status } from "@claude-dashboard/protocol";

import { HostContext, StatusContext } from "./context";
import { Store, StoreContext, useReadingProgress, useRequest } from "./data";
import { ago, fullDate } from "./format";
import type { Host } from "./host";
import { Limits } from "./sections/Limits";
import { Models } from "./sections/Models";
import { Overview } from "./sections/Overview";
import { Projects } from "./sections/Projects";
import { Sessions } from "./sections/Sessions";
import { Settings } from "./sections/Settings";
import { Problem } from "./ui/panels";
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
    host.memory.write(view);
  }, [host, view]);
  useEffect(
    () =>
      host.onShow?.((show) => {
        setView((current) => shown(current, show));
        window.scrollTo(0, 0);
      }),
    [host],
  );
  return (
    <HostContext.Provider value={host}>
      <StoreContext.Provider value={store}>
        <ViewContext.Provider value={viewState}>
          <Shell />
        </ViewContext.Provider>
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

function Shell() {
  const { view, change } = useView();
  const status = useRequest("status", null);
  const Body = BODIES[view.section];

  const go = (section: Section) => (event: MouseEvent) => {
    event.preventDefault();
    change({ section, session: section === view.section ? null : view.session });
    window.scrollTo(0, 0);
  };

  return (
    <div className="shell">
      <nav aria-label="Dashboard sections" className="sections">
        <div className="section-links">
          {SECTIONS.map((section) => (
            <a
              key={section}
              href={`#${section}`}
              aria-current={view.section === section ? "page" : undefined}
              onClick={go(section)}
            >
              {SECTION_NAMES[section]}
            </a>
          ))}
        </div>
        {status.data && <HistoryLine status={status.data} />}
      </nav>
      <main className="body">
        {status.data ? (
          <StatusContext.Provider value={status.data}>
            <Body />
          </StatusContext.Provider>
        ) : status.error ? (
          <Problem
            title="The dashboard cannot reach claude-dashboard"
            detail={status.error.message}
          />
        ) : null}
      </main>
    </div>
  );
}

/** Where the history starts and when it was last read, or that a read is under way. */
function HistoryLine({ status }: { status: Status }) {
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
  const parts = [
    status.firstDay ? `History since ${fullDate(status.firstDay)}` : "No history yet",
    status.readMs ? `read ${ago(status.readMs, now)}` : undefined,
  ].filter(Boolean);
  return <span className="history-line">{parts.join(" · ")}</span>;
}
