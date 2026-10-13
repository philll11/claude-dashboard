// The extension inside VS Code, through VS Code's API: it activates, registers what it contributes
// and opens the dashboard in one editor tab. The end-to-end tests look at what it shows.

import * as assert from "node:assert/strict";
import { mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import * as vscode from "vscode";

const ID = "philll11.claude-dashboard";

function dashboardTabs(): vscode.Tab[] {
  return vscode.window.tabGroups.all
    .flatMap((group) => group.tabs)
    .filter(
      (tab) =>
        tab.input instanceof vscode.TabInputWebview &&
        tab.input.viewType.endsWith("claudeDashboard.dashboard"),
    );
}

/** Waits until `check` holds, for at most 20 seconds. */
async function until(check: () => boolean, what: string): Promise<void> {
  const deadline = Date.now() + 20_000;
  while (!check()) {
    if (Date.now() > deadline) assert.fail(`${what} did not happen in time`);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

/** The largest session transcript in the synthetic Claude Code folder's projects. */
function largestTranscript(projects: string): string {
  let largest = { path: "", size: -1 };
  for (const project of readdirSync(projects)) {
    for (const name of readdirSync(join(projects, project))) {
      if (!name.endsWith(".jsonl")) continue;
      const path = join(projects, project, name);
      const { size } = statSync(path);
      if (size > largest.size) largest = { path, size };
    }
  }
  return largest.path;
}

/** The history's files as they stand, which change while claude-dashboard writes to them. */
function historyState(history: string): string {
  return [history, `${history}-wal`]
    .map((file) => {
      try {
        const { size, mtimeMs } = statSync(file);
        return `${String(size)}@${String(mtimeMs)}`;
      } catch {
        return "none";
      }
    })
    .join(" ");
}

describe("Dashboard for Claude Code", () => {
  before(async () => {
    const extension = vscode.extensions.getExtension(ID);
    assert.ok(extension, `${ID} is installed`);
    await extension.activate();
  });

  afterEach(async () => {
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
  });

  it("registers its commands", async () => {
    const commands = await vscode.commands.getCommands(true);
    for (const command of [
      "claudeDashboard.open",
      "claudeDashboard.show",
      "claudeDashboard.refresh",
      "claudeDashboard.connectLimits",
      "claudeDashboard.importHistory",
      "claudeDashboard.chooseFolders",
      "claudeDashboard.showLog",
      "claudeDashboard.restart",
    ]) {
      assert.ok(commands.includes(command), command);
    }
  });

  it("lets only user and remote settings choose the program", () => {
    const setting = vscode.workspace.getConfiguration("claudeDashboard").inspect("server.path");
    assert.equal(setting?.defaultValue, "");
    const manifest = vscode.extensions.getExtension(ID)?.packageJSON as {
      contributes: { configuration: { properties: Record<string, { scope: string }> } };
    };
    assert.equal(
      manifest.contributes.configuration.properties["claudeDashboard.server.path"]?.scope,
      "machine",
    );
  });

  it("opens the dashboard in one editor tab", async () => {
    await vscode.commands.executeCommand("claudeDashboard.open");
    await until(() => dashboardTabs().length === 1, "The dashboard opening");
    await vscode.commands.executeCommand("claudeDashboard.show", { section: "limits" });
    await vscode.commands.executeCommand("claudeDashboard.open");
    // VS Code marks the tab active a moment after it comes forward.
    await until(() => dashboardTabs()[0]?.isActive === true, "The dashboard coming forward");
    const tabs = dashboardTabs();
    assert.equal(tabs.length, 1);
    assert.equal(tabs[0]?.label, "Usage dashboard");
  });

  it("opens the dashboard from a tree item", async () => {
    await vscode.commands.executeCommand("claudeDashboard.show", {
      section: "sessions",
      project: "C--code-claude-dashboard",
    });
    await until(() => dashboardTabs().length === 1, "The dashboard opening");
  });

  it("starts claude-dashboard again on request", async () => {
    await vscode.commands.executeCommand("claudeDashboard.restart");
    await vscode.commands.executeCommand("claudeDashboard.refresh");
  });

  it("leaves reading to claude-dashboard, so the extension host stays idle", async () => {
    const root = process.env.CLAUDE_CONFIG_DIR ?? "";
    const history = process.env.CLAUDE_DASHBOARD_HISTORY ?? "";
    assert.ok(root && history, "the tests name a Claude Code folder and a history");
    const projects = join(root, "projects");
    const transcript = readFileSync(largestTranscript(projects));
    // Copies of one transcript: claude-dashboard reads every byte of them, though they add no
    // requests to the history.
    const copies = join(projects, "C--load");
    mkdirSync(copies, { recursive: true });
    let bytes = 0;
    for (let copy = 0; bytes < 48 * 1024 * 1024; copy += 1) {
      writeFileSync(join(copies, `load-${String(copy)}.jsonl`), transcript);
      bytes += transcript.length;
    }
    try {
      const before = historyState(history);
      const cpu = process.cpuUsage();
      const started = performance.now();
      await vscode.commands.executeCommand("claudeDashboard.refresh");
      await until(() => historyState(history) !== before, "claude-dashboard reading the copies");
      // The read has finished once the history has stayed the same for two seconds.
      let state = historyState(history);
      let changed = performance.now();
      while (performance.now() - changed < 2000) {
        assert.ok(performance.now() - started < 120_000, "claude-dashboard finished reading");
        await new Promise((resolve) => setTimeout(resolve, 100));
        const now = historyState(history);
        if (now !== state) {
          state = now;
          changed = performance.now();
        }
      }
      const used = process.cpuUsage(cpu);
      const share = (used.user + used.system) / 1000 / (changed - started);
      console.log(
        `      The extension host used ${(share * 100).toFixed(1)}% of a processor while claude-dashboard read ${String(Math.round(bytes / 1024 / 1024))} MiB in ${String(Math.round(changed - started))} ms.`,
      );
      assert.ok(share < 0.1, `the extension host used ${(share * 100).toFixed(1)}% of a processor`);
    } finally {
      rmSync(copies, { recursive: true, force: true });
    }
  });
});
