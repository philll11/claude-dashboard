// The desktop app's updates, from the project's GitHub releases. While they are automatic, the app
// checks at start, every 6 hours, and an hour after a check that failed: an update found at start
// installs straight away and the app restarts, and one found later downloads and waits for the
// person to restart. A start at login,
// which keeps the window hidden, never restarts the app by itself: its update waits too, offered
// in the app's item in the notification area. Tauri's updater checks each update's signature
// before installing it.

import type { UpdateState, Updates } from "./host";

/** How often the app checks for an update while it runs. */
export const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;

/** How soon the app checks again after a check that failed, as when the network was down. */
export const RETRY_AFTER_MS = 60 * 60 * 1000;

/** Where the person's choice is kept, as `on` or `off`. */
export const AUTOMATIC_KEY = "claude-dashboard.updates.automatic";

/** A newer version, as Tauri's updater offers it. */
export interface Update {
  version: string;
  download: () => Promise<void>;
  install: () => Promise<void>;
  downloadAndInstall: () => Promise<void>;
}

export interface UpdateApi {
  /** The newer version, or `null` when this one is the newest. */
  check(): Promise<Update | null>;
  relaunch(): Promise<void>;
  /** Where the choice is kept. */
  storage?: Pick<Storage, "getItem" | "setItem">;
  /** Whether updates are automatic until the person chooses; they are unless this says no. */
  automaticAtFirst?: boolean;
  /** Whether an update found at start installs straight away; it does unless this says no. */
  installAtStart?: boolean;
  /** Told the version of an update that waits for a restart, and `null` once none does. */
  waiting?: (version: string | null) => void;
}

function reason(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function createUpdates(api: UpdateApi): Updates {
  let state: UpdateState = { kind: "idle" };
  let ready: Update | undefined;
  let timer: ReturnType<typeof setInterval> | undefined;
  let retry: ReturnType<typeof setTimeout> | undefined;
  const listeners = new Set<() => void>();

  const changed = () => {
    for (const listener of listeners) listener();
  };
  const become = (next: UpdateState) => {
    const wasReady = state.kind === "ready";
    state = next;
    if (next.kind === "ready") api.waiting?.(next.version);
    else if (wasReady) api.waiting?.(null);
    changed();
  };
  let on = api.automaticAtFirst ?? true;
  try {
    const chosen = api.storage?.getItem(AUTOMATIC_KEY);
    if (chosen === "on" || chosen === "off") on = chosen === "on";
  } catch {
    // Without storage, the first choice stands until the person makes another.
  }
  const automatic = () => on;

  async function check(atStart: boolean) {
    if (state.kind === "checking" || state.kind === "ready" || state.kind === "installing") return;
    become({ kind: "checking" });
    try {
      const update = await api.check();
      if (!update) {
        become({ kind: "current" });
      } else if (atStart && (api.installAtStart ?? true)) {
        become({ kind: "installing", version: update.version });
        await update.downloadAndInstall();
        await api.relaunch();
      } else {
        await update.download();
        ready = update;
        become({ kind: "ready", version: update.version });
      }
    } catch (error) {
      become({ kind: "failed", reason: reason(error) });
      if (automatic()) {
        clearTimeout(retry);
        retry = setTimeout(() => {
          void check(false);
        }, RETRY_AFTER_MS);
      }
    }
  }

  const schedule = () => {
    clearInterval(timer);
    if (!automatic()) clearTimeout(retry);
    timer = automatic()
      ? setInterval(() => {
          void check(false);
        }, CHECK_EVERY_MS)
      : undefined;
  };

  if (automatic()) void check(true);
  schedule();

  return {
    state: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    automatic,
    setAutomatic(choice) {
      on = choice;
      try {
        api.storage?.setItem(AUTOMATIC_KEY, choice ? "on" : "off");
      } catch {
        // Without storage, the choice lasts until the app ends.
      }
      schedule();
      changed();
      if (choice && state.kind === "idle") void check(false);
    },
    check() {
      void check(false);
    },
    install() {
      const update = ready;
      if (!update) return;
      become({ kind: "installing", version: update.version });
      update
        .install()
        .then(() => api.relaunch())
        .catch((error: unknown) => {
          become({ kind: "failed", reason: reason(error) });
        });
    },
  };
}
