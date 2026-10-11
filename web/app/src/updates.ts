// The desktop app's updates, from the project's GitHub releases. While they are automatic, the app
// checks at start and every 6 hours: an update found at start installs straight away and the app
// restarts, and one found later downloads and waits for the person to restart. Tauri's updater
// checks each update's signature before installing it.

import type { UpdateState, Updates } from "./host";

/** How often the app checks for an update while it runs. */
export const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;

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
}

function reason(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function createUpdates(api: UpdateApi): Updates {
  let state: UpdateState = { kind: "idle" };
  let ready: Update | undefined;
  let timer: ReturnType<typeof setInterval> | undefined;
  const listeners = new Set<() => void>();

  const changed = () => {
    for (const listener of listeners) listener();
  };
  const become = (next: UpdateState) => {
    state = next;
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
      } else if (atStart) {
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
    }
  }

  const schedule = () => {
    clearInterval(timer);
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
