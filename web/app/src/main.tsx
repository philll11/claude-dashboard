import "./theme.css";
import "./app.css";

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import type { Host } from "./host";
import { vscodeHost } from "./vscode";

async function findHost(): Promise<Host | undefined> {
  const host = vscodeHost() ?? window.claudeDashboardHost;
  if (host || !import.meta.env.DEV) return host;
  const { previewHost } = await import("./preview");
  return previewHost();
}

const root = document.getElementById("root");
if (root) {
  const host = await findHost();
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
}
