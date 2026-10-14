// The dashboard in an editor tab. It is restored after a restart, and keeps its section and filters
// itself, through the webview's state.

import type { Show } from "@claude-dashboard/protocol";
import * as vscode from "vscode";

import type { Channel, Channels } from "./channel";
import { page, pageOptions, TITLE } from "./page";

/** The webview type, which `onWebviewPanel:` activation and the serializer name. */
export const PANEL_TYPE = "claudeDashboard.dashboard";

export class Dashboard implements vscode.WebviewPanelSerializer, vscode.Disposable {
  #panel: vscode.WebviewPanel | undefined;
  #channel: Channel | undefined;

  constructor(
    private readonly extensionUri: vscode.Uri,
    private readonly channels: Channels,
  ) {}

  /** Opens the dashboard, or brings it forward, showing `show` when given. */
  open(show?: Show): void {
    if (this.#panel) {
      this.#panel.reveal();
    } else {
      this.#adopt(
        vscode.window.createWebviewPanel(
          PANEL_TYPE,
          TITLE,
          vscode.ViewColumn.Active,
          pageOptions(this.extensionUri),
        ),
      );
    }
    if (show) this.#channel?.show(show);
  }

  deserializeWebviewPanel(panel: vscode.WebviewPanel): Promise<void> {
    this.#adopt(panel);
    return Promise.resolve();
  }

  dispose(): void {
    this.#panel?.dispose();
  }

  #adopt(panel: vscode.WebviewPanel): void {
    const channel = this.channels.open(panel.webview);
    this.#panel = panel;
    this.#channel = channel;
    panel.iconPath = {
      light: vscode.Uri.joinPath(this.extensionUri, "media", "gauge-light.svg"),
      dark: vscode.Uri.joinPath(this.extensionUri, "media", "gauge-dark.svg"),
    };
    panel.webview.options = pageOptions(this.extensionUri);
    panel.webview.html = page(panel.webview, this.extensionUri, "tab");
    const disposed = panel.onDidDispose(() => {
      this.#panel = undefined;
      this.#channel = undefined;
      channel.dispose();
      disposed.dispose();
    });
  }
}
