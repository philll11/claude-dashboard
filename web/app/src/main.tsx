import "./fonts.css";
import "./theme.css";
import "./app.css";

import { StrictMode } from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import type { Host } from "./host";
import { tauriHost } from "./tauri";
import { vscodeHost } from "./vscode";

async function findHost(): Promise<Host | undefined> {
  const host = vscodeHost() ?? (await tauriHost()) ?? window.claudeDashboardHost;
  if (host || !import.meta.env.DEV) return host;
  const { previewHost } = await import("./preview");
  return previewHost();
}

const root = document.getElementById("root");
if (root) {
  const host = await findHost();
  // The first render goes into the page at once, so that `ready` follows it.
  flushSync(() => {
    createRoot(root).render(
      <StrictMode>
        {host ? (
          <App host={host} />
        ) : (
          <p className="no-host">
            This is the dashboard of claude-dashboard. Open it from VS Code or the desktop app.
          </p>
        )}
      </StrictMode>,
    );
  });
  // A visible page draws the render by the second animation frame after it. A hidden one, as the
  // desktop window's is on Linux until the window shows, runs no animation frames and draws
  // nothing until then, so the render is all there is to wait for.
  if (document.visibilityState === "hidden") {
    host?.ready?.();
  } else {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        host?.ready?.();
      });
    });
  }
}
