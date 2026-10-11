// While Vite serves the dashboard for development, it runs against the browser tests' bridge:
// `?scenario=stale-limits&theme=vscode-light&surface=vscode` picks what it shows.

import type { VsCodeTheme } from "../tests/bridge/themes";
import type { Host, UpdateState } from "./host";

export async function previewHost(): Promise<Host | undefined> {
  const query = new URLSearchParams(window.location.search);
  const theme = query.get("theme") ?? "vscode-dark";
  const { NOW_MS } = await import("../tests/bridge/moment");
  const started = performance.now();
  Date.now = () => Math.round(NOW_MS + performance.now() - started);
  const { applyTheme, VSCODE_THEMES } = await import("../tests/bridge/themes");
  const themes: Record<string, VsCodeTheme | undefined> = VSCODE_THEMES;
  applyTheme({ theme, vscode: themes[theme] });
  window.claudeDashboardBridge = {
    scenario: query.get("scenario") ?? "default",
    surface: query.get("surface") ?? (theme.startsWith("desktop") ? "desktop" : "vscode"),
  };
  await import("../tests/bridge/client.js");
  return window.claudeDashboardHost;
}

declare global {
  interface Window {
    claudeDashboardBridge?: {
      scenario?: string;
      surface?: string;
      revealed?: string;
      pickedFolder?: string;
      /** Where the desktop app's updates stand. */
      update?: UpdateState;
      /** Requests on their way. */
      pending?: number;
    };
  }
}
