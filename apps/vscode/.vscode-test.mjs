// Integration tests: VS Code with the extension as it is being developed, on a synthetic Claude Code
// folder and a history of their own, so that they never read or change the person's.
// VSCODE_VERSION picks the VS Code to download, such as the oldest the extension supports, and
// VSCODE_PATH runs one already installed instead; the latest stable release otherwise.
// VSCODE_LOGS keeps VS Code's logs and crash reports in that folder.

import { copyFileSync, mkdirSync } from "node:fs";
import { basename, join } from "node:path";

import { prepare } from "@claude-dashboard/app/testing";
import { defineConfig } from "@vscode/test-cli";

const prepared = await prepare();
// The program goes where a package keeps it.
mkdirSync(join(import.meta.dirname, "server"), { recursive: true });
copyFileSync(prepared.program, join(import.meta.dirname, "server", basename(prepared.program)));

const run = process.env.VSCODE_PATH
  ? { useInstallation: { fromPath: process.env.VSCODE_PATH } }
  : { version: process.env.VSCODE_VERSION ?? "stable" };

export default defineConfig({
  label: "integration",
  files: "out/test/integration/**/*.test.js",
  ...run,
  workspaceFolder: prepared.folder,
  launchArgs: [
    "--disable-extensions",
    "--disable-workspace-trust",
    "--skip-welcome",
    "--skip-release-notes",
    ...(process.env.VSCODE_LOGS
      ? [
          `--logsPath=${join(process.env.VSCODE_LOGS, "logs")}`,
          `--crash-reporter-directory=${join(process.env.VSCODE_LOGS, "crashes")}`,
        ]
      : []),
  ],
  env: {
    CLAUDE_CONFIG_DIR: prepared.env.CLAUDE_CONFIG_DIR,
    CLAUDE_DASHBOARD_HISTORY: prepared.env.CLAUDE_DASHBOARD_HISTORY,
    CLAUDE_DASHBOARD_NOW: prepared.env.CLAUDE_DASHBOARD_NOW,
    TZ: prepared.env.TZ,
  },
  mocha: { ui: "bdd", timeout: 60_000 },
});
