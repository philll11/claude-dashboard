// The dashboard as a view, in the panel beside the terminal or in a sidebar: the whole dashboard
// under a strip that keeps the plan windows and the latest session in this project in sight (ADR
// 0013). It does not keep running while hidden: VS Code builds it again each time it shows it, and
// the web app keeps its section and its last figures in the view's state meanwhile.

import * as vscode from "vscode";

import type { Channel, Channels } from "./channel";
import { page, pageOptions } from "./page";

/** The view's id, which `contributes.views` names. */
export const VIEW_ID = "claudeDashboard.view";

export class DashboardView implements vscode.WebviewViewProvider, vscode.Disposable {
  #channel: Channel | undefined;

  constructor(
    private readonly extensionUri: vscode.Uri,
    private readonly channels: Channels,
  ) {}

  resolveWebviewView(view: vscode.WebviewView): void {
    this.#channel?.dispose();
    const channel = this.channels.open(view.webview, { folders: true });
    this.#channel = channel;
    view.webview.options = pageOptions(this.extensionUri);
    view.webview.html = page(view.webview, this.extensionUri, "view");
    const disposed = view.onDidDispose(() => {
      if (this.#channel === channel) this.#channel = undefined;
      channel.dispose();
      disposed.dispose();
    });
  }

  dispose(): void {
    this.#channel?.dispose();
  }
}
