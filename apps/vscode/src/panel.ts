// The dashboard in an editor tab: the web app in a webview that runs only its own scripts, passing
// its requests to the server and the server's notifications back. It is restored after a restart,
// and keeps its section and filters itself, through the webview's state.

import * as crypto from "node:crypto";

import type {
  FromExtension,
  Method,
  Notifications,
  Params,
  Show,
  ToExtension,
} from "@claude-dashboard/protocol";
import * as vscode from "vscode";
import { ResponseError } from "vscode-jsonrpc/node";

import type { Server } from "./server";
import type { SummaryModel } from "./summary";

/** The webview type, which `onWebviewPanel:` activation and the serializer name. */
export const PANEL_TYPE = "claudeDashboard.dashboard";

const TITLE = "Usage dashboard";

/** Requests after which the summary reads the person's settings again. */
const SETTINGS_CHANGES: readonly Method[] = [
  "statusLine/apply",
  "endpoint/set",
  "settings/set",
  "folders/add",
  "folders/remove",
  "import/run",
  "import/dismiss",
];

export class Dashboard implements vscode.WebviewPanelSerializer, vscode.Disposable {
  #panel: vscode.WebviewPanel | undefined;
  /** Whether the web app in the panel listens yet. */
  #ready = false;
  /** A section asked for before the web app listened. */
  #waitingShow: Show | undefined;
  readonly #panelSubscriptions: vscode.Disposable[] = [];
  readonly #subscriptions: vscode.Disposable[];

  constructor(
    private readonly extensionUri: vscode.Uri,
    private readonly server: Server,
    private readonly summary: SummaryModel,
  ) {
    this.#subscriptions = [
      server.onNotification(({ method, params }) => {
        this.#post({ type: "notification", method, params });
      }),
      server.onDidChangeState((state) => {
        // Everything the web app shows is asked for again from a server that starts afresh.
        if (state.kind === "running") this.#notify("history/changed", { days: null });
      }),
    ];
  }

  /** Opens the dashboard, or brings it forward, showing `show` when given. */
  open(show?: Show): void {
    if (this.#panel) {
      this.#panel.reveal();
    } else {
      this.#adopt(
        vscode.window.createWebviewPanel(PANEL_TYPE, TITLE, vscode.ViewColumn.Active, {
          enableScripts: true,
          localResourceRoots: [this.#webviewRoot()],
        }),
      );
    }
    if (show) this.#show(show);
  }

  deserializeWebviewPanel(panel: vscode.WebviewPanel): Promise<void> {
    this.#adopt(panel);
    return Promise.resolve();
  }

  dispose(): void {
    this.#panel?.dispose();
    for (const subscription of this.#subscriptions) subscription.dispose();
  }

  #webviewRoot(): vscode.Uri {
    return vscode.Uri.joinPath(this.extensionUri, "dist", "webview");
  }

  #adopt(panel: vscode.WebviewPanel): void {
    this.#panel = panel;
    this.#ready = false;
    panel.iconPath = {
      light: vscode.Uri.joinPath(this.extensionUri, "media", "gauge-light.svg"),
      dark: vscode.Uri.joinPath(this.extensionUri, "media", "gauge-dark.svg"),
    };
    panel.webview.options = { enableScripts: true, localResourceRoots: [this.#webviewRoot()] };
    panel.webview.html = this.#html(panel.webview);
    this.#panelSubscriptions.push(
      panel.webview.onDidReceiveMessage((message: ToExtension) => {
        void this.#received(message);
      }),
      panel.onDidDispose(() => {
        this.#panel = undefined;
        this.#ready = false;
        this.#waitingShow = undefined;
        for (const subscription of this.#panelSubscriptions.splice(0)) subscription.dispose();
      }),
    );
  }

  #html(webview: vscode.Webview): string {
    const nonce = crypto.randomBytes(16).toString("base64");
    const asset = (name: string) =>
      webview.asWebviewUri(vscode.Uri.joinPath(this.#webviewRoot(), name)).toString();
    const policy = [
      "default-src 'none'",
      `style-src ${webview.cspSource}`,
      `img-src ${webview.cspSource} data:`,
      `font-src ${webview.cspSource}`,
      `script-src 'nonce-${nonce}'`,
    ].join("; ");
    return `<!doctype html>
<html lang="en-GB">
  <head>
    <meta charset="utf-8" />
    <meta http-equiv="Content-Security-Policy" content="${policy}" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>${TITLE}</title>
    <link rel="stylesheet" href="${asset("dashboard.css")}" />
  </head>
  <body>
    <div id="root"></div>
    <script type="module" nonce="${nonce}" src="${asset("dashboard.js")}"></script>
  </body>
</html>`;
  }

  async #received(message: ToExtension): Promise<void> {
    switch (message.type) {
      case "ready":
        this.#ready = true;
        if (this.#waitingShow) this.#post({ type: "show", show: this.#waitingShow });
        this.#waitingShow = undefined;
        return;
      case "request":
        await this.#request(message.id, message.method, message.params);
        return;
      case "reveal":
        await reveal(message.path);
        return;
      case "pickFolder": {
        const picked = await vscode.window.showOpenDialog({
          canSelectFiles: false,
          canSelectFolders: true,
          canSelectMany: false,
          openLabel: "Add Folder",
          title: "Add a data folder",
        });
        const path = picked?.[0]?.fsPath;
        this.#post(
          path ? { type: "picked", id: message.id, path } : { type: "picked", id: message.id },
        );
        return;
      }
    }
  }

  async #request(id: number, method: Method, params: unknown): Promise<void> {
    try {
      const result: unknown = await this.server.request(method, params as Params<typeof method>);
      this.#post({ type: "response", id, result });
      if (SETTINGS_CHANGES.includes(method)) void this.summary.refresh(true);
    } catch (error) {
      this.#post({
        type: "response",
        id,
        error:
          error instanceof ResponseError
            ? { code: error.code, message: error.message }
            : { code: -32603, message: error instanceof Error ? error.message : String(error) },
      });
    }
  }

  #show(show: Show): void {
    if (this.#ready) this.#post({ type: "show", show });
    else this.#waitingShow = show;
  }

  #notify<N extends keyof Notifications>(method: N, params: Notifications[N]): void {
    this.#post({ type: "notification", method, params });
  }

  #post(message: FromExtension): void {
    if (this.#panel && this.#ready) void this.#panel.webview.postMessage(message);
  }
}

/** Shows a file or folder in the system's file manager. A window on another machine, such as over
 * SSH or in WSL, cannot, so it offers to copy the path instead. */
async function reveal(path: string): Promise<void> {
  if (!vscode.env.remoteName) {
    await vscode.commands.executeCommand("revealFileInOS", vscode.Uri.file(path));
    return;
  }
  const copy = "Copy Path";
  const chosen = await vscode.window.showInformationMessage(
    `It is at ${path} on the machine this window is connected to.`,
    copy,
  );
  if (chosen === copy) await vscode.env.clipboard.writeText(path);
}
