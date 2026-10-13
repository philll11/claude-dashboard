// What the dashboard needs from the surface it runs in: VS Code, the desktop app, or a page that
// provides `window.claudeDashboardHost`, as the browser tests do.

import type { Backend, ExportParams, Exported, Show } from "@claude-dashboard/protocol";

/** Where every version of claude-dashboard is published, with its installers and packages. */
export const RELEASES = "https://github.com/philll11/claude-dashboard/releases";

/** What the dashboard remembers between visits, such as the section and the filters. */
export interface Memory {
  read(): unknown;
  write(state: unknown): void;
}

/** Where the desktop app's updates stand. */
export type UpdateState =
  /** Updates are not installed automatically, and nobody has checked yet. */
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current" }
  /** A newer version is downloaded and installs when the app restarts. */
  | { kind: "ready"; version: string }
  | { kind: "installing"; version: string }
  | { kind: "failed"; reason: string };

/** The desktop app's updates, from the project's GitHub releases. Its functions use no `this`,
 * so React can take `state` and `subscribe` as they are. */
export interface Updates {
  state: () => UpdateState;
  /** Calls `listener` whenever the state changes, until the returned function is called. */
  subscribe: (listener: () => void) => () => void;
  /** Whether updates are checked for and installed without asking. */
  automatic: () => boolean;
  setAutomatic: (on: boolean) => void;
  check: () => void;
  /** Installs the update that is ready, and restarts the app. */
  install: () => void;
}

/** What only the desktop app does: its item in the notification area or the menu bar, opening at
 * login, and what happens the first time its window closes. */
export interface Desktop {
  /** Whether the app has its item in the notification area or the menu bar; a Linux desktop
   * without one leaves the app without it. */
  tray: boolean;
  /** The words for where the item is: `notification area`, or `menu bar` on macOS. */
  trayPlace: string;
  /** Whether the app opens at login, as the operating system has it. */
  autostart: () => Promise<boolean>;
  setAutostart: (on: boolean) => Promise<void>;
  /** Calls `listener` when the window first closes while the app would keep running, until the
   * returned function is called; the person's answer goes to `closeChoice`. */
  onCloseAsked: (listener: () => void) => () => void;
  /** Keeps the app running with its window hidden, or quits it, and remembers the choice. */
  closeChoice: (keepRunning: boolean) => void;
}

/** The JSON-RPC error code of an export that was cancelled. */
export const EXPORT_CANCELLED = -32800;

export interface Host {
  /** Where the dashboard runs, which decides its layout. */
  surface: "vscode" | "desktop";
  backend: Backend;
  memory: Memory;
  /** Shows a file or folder in the system's file manager, where the surface can. */
  reveal?: (path: string) => void;
  /** Asks the person for a folder, where the surface can; `undefined` when they choose none. */
  pickFolder?: () => Promise<string | undefined>;
  /** Calls `listener` whenever the surface asks for a section, such as from VS Code's sidebar,
   * until the returned function is called. */
  onShow?: (listener: (show: Show) => void) => () => void;
  /** Called once the dashboard has drawn for the first time, for a surface that waits for it. */
  ready?: () => void;
  updates?: Updates;
  /** Saves an export where the person chooses in the surface's save dialog, reporting progress
   * as `export/progress` notifications; `undefined` when they choose no file. It fails with
   * `EXPORT_CANCELLED` when `cancelExport` stops it. */
  exportTo?: (params: ExportParams) => Promise<Exported | undefined>;
  cancelExport?: () => void;
  desktop?: Desktop;
}

/** A request the server refused, with the JSON-RPC error code it gave. */
export class RequestError extends Error {
  readonly code: number;

  constructor(message: string, code: number) {
    super(message);
    this.code = code;
  }
}

declare global {
  interface Window {
    claudeDashboardHost?: Host;
  }
}

/** Memory that lasts as long as the page. */
export function pageMemory(): Memory {
  let state: unknown;
  return {
    read: () => state,
    write: (next) => {
      state = next;
    },
  };
}
