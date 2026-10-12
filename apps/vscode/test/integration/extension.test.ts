// The extension inside VS Code, through VS Code's API: it activates, registers what it contributes
// and opens the dashboard in one editor tab. The end-to-end tests look at what it shows.

import * as assert from "node:assert/strict";

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
});
