// The dashboard in the desktop app: requests go to the server in the app's own process through one
// Tauri command, and its notifications come back as one event (ADR 0008). The app's window follows
// the system's light or dark theme, and stays hidden until the dashboard has drawn in it.

import type {
  Backend,
  Method,
  Notifications,
  Params,
  Result,
  Show,
} from "@claude-dashboard/protocol";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";

import { type Host, RequestError } from "./host";
import { createUpdates, type UpdateApi } from "./updates";

/** Where the section and filters are kept between runs of the app. */
export const VIEW_KEY = "claude-dashboard.view";

type Theme = "light" | "dark";

/** What the desktop host needs from Tauri, which the tests stand in for. */
export interface DesktopApi extends UpdateApi {
  invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
  listen(event: string, handler: (payload: unknown) => void): Promise<() => void>;
  theme(): Promise<Theme | null>;
  onThemeChanged(handler: (theme: Theme) => void): Promise<() => void>;
  /** Shows the window. */
  show(): Promise<void>;
  pickFolder(): Promise<string | undefined>;
  reveal(path: string): Promise<void>;
  /** Puts the dashboard in the desktop's light or dark theme. */
  applyTheme(theme: Theme): void;
}

/** The fault the server gives for a request it refused, as Tauri passes it on. */
function refused(error: unknown): Error {
  if (typeof error === "object" && error !== null && "code" in error && "message" in error) {
    return new RequestError(String(error.message), Number(error.code));
  }
  return error instanceof Error ? error : new Error(String(error));
}

/** The desktop host, made from `api`. */
export async function desktopHost(api: DesktopApi): Promise<Host> {
  api.applyTheme((await api.theme()) ?? "dark");
  await api.onThemeChanged((theme) => {
    api.applyTheme(theme);
  });

  const listeners = new Map<string, Set<(params: unknown) => void>>();
  await api.listen("notification", (payload) => {
    const { method, params } = payload as { method: string; params: unknown };
    for (const listener of listeners.get(method) ?? []) listener(params);
  });

  const showing = new Set<(show: Show) => void>();
  // A section asked for before the dashboard listens is shown once it does.
  let waiting = (await api.invoke("take_show")) as Show | null;
  await api.listen("show", (payload) => {
    if (showing.size === 0) waiting = payload as Show;
    for (const listener of showing) listener(payload as Show);
  });

  const backend: Backend = {
    async request<M extends Method>(method: M, params: Params<M>): Promise<Result<M>> {
      try {
        return (await api.invoke("request", { method, params: params ?? null })) as Result<M>;
      } catch (error) {
        throw refused(error);
      }
    },
    onNotification<N extends keyof Notifications>(
      method: N,
      listener: (params: Notifications[N]) => void,
    ) {
      const set = listeners.get(method) ?? new Set();
      listeners.set(method, set);
      const wrapped = listener as (params: unknown) => void;
      set.add(wrapped);
      return () => set.delete(wrapped);
    },
  };

  return {
    surface: "desktop",
    backend,
    memory: {
      read() {
        try {
          const saved = api.storage?.getItem(VIEW_KEY);
          return saved ? (JSON.parse(saved) as unknown) : undefined;
        } catch {
          return undefined;
        }
      },
      write(state) {
        try {
          api.storage?.setItem(VIEW_KEY, JSON.stringify(state));
        } catch {
          // The view starts over next time.
        }
      },
    },
    reveal: (path) => {
      void api.reveal(path);
    },
    pickFolder: () => api.pickFolder(),
    onShow(listener) {
      showing.add(listener);
      if (waiting) listener(waiting);
      waiting = null;
      return () => {
        showing.delete(listener);
      };
    },
    ready: () => {
      void api.show();
    },
    updates: createUpdates(api),
  };
}

function storage(): Storage | undefined {
  try {
    return window.localStorage;
  } catch {
    return undefined;
  }
}

/** The host the desktop app provides, or `undefined` outside it. */
export async function tauriHost(): Promise<Host | undefined> {
  if (!isTauri()) return undefined;
  const window = getCurrentWindow();
  const store = storage();
  return desktopHost({
    invoke: (command, args) => invoke(command, args),
    listen: (event, handler) =>
      listen(event, ({ payload }) => {
        handler(payload);
      }),
    theme: () => window.theme(),
    onThemeChanged: (handler) =>
      window.onThemeChanged(({ payload }) => {
        handler(payload);
      }),
    show: () => window.show(),
    pickFolder: async () => (await open({ directory: true, multiple: false })) ?? undefined,
    reveal: (path) => revealItemInDir(path),
    applyTheme(theme) {
      if (theme === "light") document.documentElement.dataset.theme = "desktop-light";
      else delete document.documentElement.dataset.theme;
    },
    check: () => check(),
    relaunch: () => relaunch(),
    ...(store ? { storage: store } : {}),
    // `tauri dev` runs the app from the build folder, which an update would not replace.
    automaticAtFirst: !import.meta.env.DEV,
  });
}
