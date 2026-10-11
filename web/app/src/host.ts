// What the dashboard needs from the surface it runs in: VS Code, the desktop app, or a page that
// provides `window.claudeDashboardHost`, as the browser tests do.

import type { Backend, Show } from "@claude-dashboard/protocol";

/** What the dashboard remembers between visits, such as the section and the filters. */
export interface Memory {
  read(): unknown;
  write(state: unknown): void;
}

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
