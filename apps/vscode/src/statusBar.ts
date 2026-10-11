// The status bar item: the five-hour window at a glance, or today's cost without plan limits. It
// opens the dashboard, and its tooltip lists every window and where the numbers come from.

import * as vscode from "vscode";

import { now } from "./clock";
import type { ServerState } from "./server";
import { statusItem, type Summary } from "./shape";

const NAME = "Dashboard for Claude Code";

/** Text for HTML, which the tooltip is written in. */
function escaped(text: string): string {
  return text.replace(/[&<>"]/g, (c) => `&#${String(c.charCodeAt(0))};`);
}

export class StatusBar implements vscode.Disposable {
  readonly #item = vscode.window.createStatusBarItem(
    "claudeDashboard.status",
    vscode.StatusBarAlignment.Right,
    100,
  );

  constructor() {
    this.#item.name = NAME;
    this.#item.command = "claudeDashboard.open";
    this.#item.text = "$(dashboard)";
    this.#item.tooltip = "Reading your Claude Code history…";
    this.#item.show();
  }

  show(summary: Summary): void {
    const item = statusItem(summary, now());
    this.#item.text = `$(dashboard) ${item.text}`;
    this.#item.accessibilityInformation = { label: item.label };
    this.#item.backgroundColor =
      item.level === "critical"
        ? new vscode.ThemeColor("statusBarItem.errorBackground")
        : item.level === "warning"
          ? new vscode.ThemeColor("statusBarItem.warningBackground")
          : undefined;
    this.#item.command = "claudeDashboard.open";
    const rows = item.lines
      .map(
        (line) =>
          `<tr><td>${escaped(line.label)}</td><td align="right">${escaped(line.value)}</td></tr>`,
      )
      .join("");
    const tooltip = new vscode.MarkdownString(undefined, true);
    tooltip.supportHtml = true;
    tooltip.isTrusted = { enabledCommands: ["claudeDashboard.open"] };
    tooltip.appendMarkdown(`<table>${rows}</table>\n\n`);
    tooltip.appendMarkdown(`<span>${escaped(item.note)}</span>\n\n`);
    tooltip.appendMarkdown("[Open dashboard](command:claudeDashboard.open)");
    this.#item.tooltip = tooltip;
  }

  /** What the item shows while there are no numbers: the server starting, or why it is not. */
  showState(state: ServerState): void {
    this.#item.backgroundColor = undefined;
    switch (state.kind) {
      case "starting":
      case "running":
        return;
      case "stopped":
      case "noProgram":
        this.#item.text = "$(dashboard) $(error)";
        this.#item.tooltip = `${state.reason} Select to show the log.`;
        this.#item.accessibilityInformation = {
          label: `${NAME} cannot show your usage: ${state.reason}`,
        };
        this.#item.command = "claudeDashboard.showLog";
    }
  }

  dispose(): void {
    this.#item.dispose();
  }
}
