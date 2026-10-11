// Dashboard for Claude Code: a status bar item, a sidebar tree and a dashboard tab, all fed by
// `claude-dashboard serve`, which reads Claude Code's transcripts into the history every surface
// shares. The extension starts once VS Code has, so that new activity is read as VS Code opens.

import type { Show } from "@claude-dashboard/protocol";
import * as vscode from "vscode";

import { Dashboard, PANEL_TYPE } from "./panel";
import { Server, type ServerState } from "./server";
import { StatusBar } from "./statusBar";
import { SummaryModel } from "./summary";
import { SummaryTree } from "./tree";

/** What the tree's welcome content and the walkthrough go by. */
function setContext(key: string, value: string | boolean): void {
  void vscode.commands.executeCommand("setContext", `claudeDashboard.${key}`, value);
}

export function activate(context: vscode.ExtensionContext): void {
  const log = vscode.window.createOutputChannel("Dashboard for Claude Code", { log: true });
  const server = new Server(context.extensionPath, log);
  const summary = new SummaryModel(server, log);
  const statusBar = new StatusBar();
  const tree = new SummaryTree(summary, context.extensionUri);
  const dashboard = new Dashboard(context.extensionUri, server, summary);

  const shown = (state: ServerState) => {
    setContext("state", state.kind === "running" && !summary.summary ? "starting" : state.kind);
    statusBar.showState(state);
    if (state.kind === "stopped") {
      void vscode.window
        .showErrorMessage(
          `${state.reason}. The dashboard cannot show your usage.`,
          "Restart",
          "Show Log",
        )
        .then((chosen) => {
          if (chosen === "Restart") void server.restart();
          else if (chosen === "Show Log") log.show();
        });
    }
  };
  shown(server.state);

  context.subscriptions.push(
    log,
    server,
    summary,
    statusBar,
    tree,
    dashboard,
    vscode.window.createTreeView("claudeDashboard.summary", { treeDataProvider: tree }),
    vscode.window.registerWebviewPanelSerializer(PANEL_TYPE, dashboard),
    server.onDidChangeState(shown),
    summary.onDidChange(() => {
      const current = summary.summary;
      // Once the server has stopped, the status bar says so rather than show its last numbers.
      if (!current || server.state.kind !== "running") return;
      statusBar.show(current);
      setContext("state", "running");
      setContext("limitsConnected", current.limitsConnected);
      setContext("importOffered", summary.importOffered);
    }),
    vscode.commands.registerCommand("claudeDashboard.open", () => {
      dashboard.open();
    }),
    vscode.commands.registerCommand("claudeDashboard.show", (show: Show) => {
      dashboard.open(show);
    }),
    vscode.commands.registerCommand("claudeDashboard.refresh", () =>
      server.request("refresh", null).catch((error: unknown) => {
        log.error(`Cannot read new activity: ${String(error)}`);
      }),
    ),
    vscode.commands.registerCommand("claudeDashboard.connectLimits", () => {
      dashboard.open({ section: "limits" });
    }),
    vscode.commands.registerCommand("claudeDashboard.importHistory", () => {
      dashboard.open({ section: "settings" });
    }),
    vscode.commands.registerCommand("claudeDashboard.chooseFolders", () => {
      dashboard.open({ section: "settings" });
    }),
    vscode.commands.registerCommand("claudeDashboard.showLog", () => {
      log.show();
    }),
    vscode.commands.registerCommand("claudeDashboard.restart", () => server.restart()),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("claudeDashboard.server.path")) void server.restart();
    }),
  );
  void server.start();
}
