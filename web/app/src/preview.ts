// While Vite serves the dashboard for development, it runs against the browser tests' bridge:
// `?scenario=stale-limits&theme=vscode-light&surface=vscode` picks what it shows, and
// `&placement=view&folder=/home/you/code/claude-dashboard` shows VS Code's view with that folder
// open.

import type { VsCodeTheme } from "../tests/bridge/themes";
import type { ExportProgress, OpenFolders } from "@claude-dashboard/protocol";

import type { Host, UpdateState } from "./host";

export async function previewHost(): Promise<Host | undefined> {
  const query = new URLSearchParams(window.location.search);
  const theme = query.get("theme") ?? "vscode-dark";
  const { NOW_MS } = await import("../tests/bridge/moment");
  const started = performance.now();
  Date.now = () => Math.round(NOW_MS + performance.now() - started);
  const { applyTheme, VSCODE_THEMES } = await import("../tests/bridge/themes");
  const themes: Record<string, VsCodeTheme | undefined> = VSCODE_THEMES;
  const placement = query.get("placement") ?? "tab";
  applyTheme({ theme, vscode: themes[theme], placement });
  window.claudeDashboardBridge = {
    scenario: query.get("scenario") ?? "default",
    surface: query.get("surface") ?? (theme.startsWith("desktop") ? "desktop" : "vscode"),
    placement,
    folders: { folders: query.getAll("folder"), ignoreCase: false },
  };
  await import("../tests/bridge/client.js");
  return window.claudeDashboardHost;
}

declare global {
  interface Window {
    claudeDashboardBridge?: {
      scenario?: string;
      surface?: string;
      /** In VS Code, `tab` or `view`. */
      placement?: string;
      /** The folders VS Code has open, which `setFolders` changes. */
      folders?: OpenFolders;
      setFolders?: (folders: OpenFolders) => void;
      /** What the dashboard remembered from an earlier visit, and what it remembers now. */
      state?: unknown;
      memory?: () => unknown;
      revealed?: string;
      pickedFolder?: string;
      /** Where the desktop app's updates stand. */
      update?: UpdateState;
      /** Requests on their way. */
      pending?: number;
      /** Holds an export under way at this progress until `releaseExport` is called. */
      exportHeld?: ExportProgress;
      releaseExport?: () => void;
      cancelHeld?: () => void;
      /** Asks the desktop app's question for when its window first closes. */
      askClose?: () => void;
      /** The answer to that question. */
      closeChoice?: boolean;
      /** Whether the desktop app opens at login. */
      autostart?: boolean;
    };
  }
}
