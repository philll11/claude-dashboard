// The sidebar tree: plan limits, today, this week and the week's top projects, each opening the
// matching section of the dashboard.

import * as vscode from "vscode";

import { now } from "./clock";
import { type Icon, type Node, tree } from "./shape";
import type { SummaryModel } from "./summary";

function iconOf(icon: Icon | undefined, extensionUri: vscode.Uri): vscode.TreeItem["iconPath"] {
  switch (icon?.kind) {
    case undefined:
      return undefined;
    case "gauge":
      return new vscode.ThemeIcon("dashboard");
    case "level":
      return new vscode.ThemeIcon(
        "warning",
        new vscode.ThemeColor(
          icon.level === "critical"
            ? "problemsErrorIcon.foreground"
            : "problemsWarningIcon.foreground",
        ),
      );
    case "folder":
      return new vscode.ThemeIcon("folder");
    case "model": {
      const swatch = (kind: string) =>
        vscode.Uri.joinPath(extensionUri, "dist", "swatches", `${icon.series}-${kind}.svg`);
      return { light: swatch("light"), dark: swatch("dark") };
    }
    case "connect":
      return new vscode.ThemeIcon("plug");
  }
}

export class SummaryTree implements vscode.TreeDataProvider<Node>, vscode.Disposable {
  readonly #changed = new vscode.EventEmitter<undefined>();
  readonly #subscription: vscode.Disposable;
  readonly onDidChangeTreeData = this.#changed.event;

  constructor(
    private readonly model: SummaryModel,
    private readonly extensionUri: vscode.Uri,
  ) {
    this.#subscription = model.onDidChange(() => {
      this.#changed.fire(undefined);
    });
  }

  getChildren(node?: Node): Node[] {
    if (node) return node.children;
    const summary = this.model.summary;
    return summary ? tree(summary, now()) : [];
  }

  getTreeItem(node: Node): vscode.TreeItem {
    const item = new vscode.TreeItem(
      node.label,
      node.children.length === 0
        ? vscode.TreeItemCollapsibleState.None
        : node.expanded
          ? vscode.TreeItemCollapsibleState.Expanded
          : vscode.TreeItemCollapsibleState.Collapsed,
    );
    item.id = node.id;
    item.description = node.description;
    const icon = iconOf(node.icon, this.extensionUri);
    if (icon) item.iconPath = icon;
    item.accessibilityInformation = {
      label: node.description ? `${node.label}, ${node.description}` : node.label,
    };
    item.command = {
      command: "claudeDashboard.show",
      title: "Show in the dashboard",
      arguments: [node.show],
    };
    return item;
  }

  dispose(): void {
    this.#subscription.dispose();
    this.#changed.dispose();
  }
}
