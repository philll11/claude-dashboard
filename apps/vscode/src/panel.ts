// The dashboard in an editor tab: the web app in a webview that runs only its own scripts, passing
// its requests to the server and the server's notifications back, and saving its exports where the
// person chooses. It is restored after a restart, and keeps its section and filters itself,
// through the webview's state.

import * as crypto from "node:crypto";
import * as fs from "node:fs";
import * as os from "node:os";
import { join } from "node:path";

import type {
  ExportFormat,
  Exported,
  ExportParams,
  FromExtension,
  Method,
  Notifications,
  Params,
  Show,
  ToExtension,
} from "@claude-dashboard/protocol";
import * as vscode from "vscode";
import { ErrorCodes, ResponseError } from "vscode-jsonrpc/node";

import type { Server } from "./server";
import type { SummaryModel } from "./summary";

/** The webview type, which `onWebviewPanel:` activation and the serializer name. */
export const PANEL_TYPE = "claudeDashboard.dashboard";

const TITLE = "Usage dashboard";

/** The code of a request that was cancelled, as the server answers a cancelled export. */
const REQUEST_CANCELLED = -32800;

/** The requests the dashboard makes, and so the only ones passed on to the server. An export writes
 * a file, so only a destination the person chose in the extension's save dialog may be written. */
const DASHBOARD_REQUESTS: Readonly<Record<Method, true>> = {
  status: true,
  choices: true,
  overview: true,
  sessions: true,
  session: true,
  limits: true,
  prices: true,
  health: true,
  refresh: true,
  "endpoint/set": true,
  "statusLine/plan": true,
  "statusLine/apply": true,
  settings: true,
  "settings/set": true,
  "folders/add": true,
  "folders/remove": true,
  "import/offer": true,
  "import/run": true,
  "import/dismiss": true,
  "export/name": true,
};

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
  /** The export under way, one at a time, and whether the person has cancelled it. */
  #exporting: { cancelled: boolean } | undefined;
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
      case "export":
        await this.#export(message.id, message.params);
        return;
      case "cancelExport":
        await this.#cancelExport();
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
    if (!Object.hasOwn(DASHBOARD_REQUESTS, method)) {
      this.#post({
        type: "response",
        id,
        error: {
          code: ErrorCodes.MethodNotFound,
          message: `The dashboard cannot ask for ${method}.`,
        },
      });
      return;
    }
    try {
      const result: unknown = await this.server.request(method, params as Params<typeof method>);
      this.#post({ type: "response", id, result });
      if (SETTINGS_CHANGES.includes(method)) void this.summary.refresh(true);
    } catch (error) {
      this.#post({ type: "response", id, error: failure(error) });
    }
  }

  /** Saves an export where the person chooses, and tells the web app how it went. */
  async #export(id: number, params: ExportParams): Promise<void> {
    if (this.#exporting) {
      this.#post({
        type: "exported",
        id,
        error: {
          code: ErrorCodes.InvalidParams,
          message: "An export is under way; wait for it to finish, or cancel it.",
        },
      });
      return;
    }
    this.#exporting = { cancelled: false };
    try {
      const result = await this.#saveExport(params);
      this.#post(result ? { type: "exported", id, result } : { type: "exported", id });
    } catch (error) {
      this.#post({ type: "exported", id, error: failure(error) });
    } finally {
      this.#exporting = undefined;
    }
  }

  /** The export as saved, or `undefined` when the person chooses no file. */
  async #saveExport(params: ExportParams): Promise<Exported | undefined> {
    const { name } = await this.server.request("export/name", params);
    this.#stopIfCancelled();
    const file = await chooseFile(name, params.format);
    if (!file) return undefined;
    this.#stopIfCancelled();
    // A `file:` URI names a file on the machine the server runs on, even in a remote window, so
    // the server writes it. Any other, such as the local computer's from a remote window, only VS
    // Code can write.
    if (file.scheme === "file") {
      return this.server.extensionRequest("export/save", { export: params, path: file.fsPath });
    }
    const { rows, what, content } = await this.server.extensionRequest("export/content", params);
    if (content == null) throw new Error("claude-dashboard returned no content to save.");
    await vscode.workspace.fs.writeFile(file, Buffer.from(content, "utf8"));
    return { rows, what };
  }

  /** Ends an export that the person cancelled before the server began it, as the server ends one
   * it has begun. */
  #stopIfCancelled(): void {
    if (this.#exporting?.cancelled) {
      throw new ResponseError(REQUEST_CANCELLED, "The export was cancelled.");
    }
  }

  async #cancelExport(): Promise<void> {
    if (!this.#exporting) return;
    this.#exporting.cancelled = true;
    try {
      await this.server.extensionRequest("export/cancel", null);
    } catch {
      // The server has ended, and the export with it.
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

/** How a failed request is told to the web app: the server's own code and message, or else an
 * internal error. */
function failure(error: unknown): { code: number; message: string } {
  return error instanceof ResponseError
    ? { code: error.code, message: error.message }
    : {
        code: ErrorCodes.InternalError,
        message: error instanceof Error ? error.message : String(error),
      };
}

/** The file an export is saved to: the person's choice in a save dialog. */
async function chooseFile(name: string, format: ExportFormat): Promise<vscode.Uri | undefined> {
  // A test cannot answer a save dialog, so a folder named here receives the file instead.
  const folder = process.env.CLAUDE_DASHBOARD_EXPORT_TO;
  if (folder) return vscode.Uri.file(join(folder, name));
  const label = format === "csv" ? "CSV" : "JSON";
  return vscode.window.showSaveDialog({
    defaultUri: suggestedFile(name),
    filters: { [label]: [format] },
    saveLabel: "Export",
    title: `Export as ${label}`,
  });
}

/** Where the save dialog starts: the Downloads folder when there is one, otherwise the first
 * folder of the workspace, otherwise the home folder. */
function suggestedFile(name: string): vscode.Uri {
  const downloads = join(os.homedir(), "Downloads");
  if (fs.existsSync(downloads)) return vscode.Uri.file(join(downloads, name));
  const workspace = vscode.workspace.workspaceFolders?.[0];
  return workspace
    ? vscode.Uri.joinPath(workspace.uri, name)
    : vscode.Uri.file(join(os.homedir(), name));
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
