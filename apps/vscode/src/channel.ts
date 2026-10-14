// What passes between a webview showing the dashboard and the server: the web app's requests and
// the answers, the server's notifications, and exports saved where the person chooses. Each webview
// has a channel of its own, so that an answer goes back to the webview that asked; the channels
// share the server, which runs one export at a time.

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
  OpenFolders,
  Params,
  Show,
  ToExtension,
} from "@claude-dashboard/protocol";
import * as vscode from "vscode";
import { ErrorCodes, ResponseError } from "vscode-jsonrpc/node";

import type { Server } from "./server";
import type { SummaryModel } from "./summary";

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

/** The channels to every webview showing the dashboard, and what they share. */
export class Channels implements vscode.Disposable {
  readonly #open = new Set<Channel>();
  /** The export under way, the webview that asked for it, and whether the person has cancelled
   * it. */
  exporting: { channel: Channel; cancelled: boolean } | undefined;
  readonly #subscriptions: vscode.Disposable[];

  constructor(
    readonly server: Server,
    readonly summary: SummaryModel,
  ) {
    this.#subscriptions = [
      server.onNotification(({ method, params }) => {
        for (const channel of this.#open) channel.post({ type: "notification", method, params });
      }),
      server.onDidChangeState((state) => {
        // Everything the web app shows is asked for again from a server that starts afresh.
        if (state.kind !== "running") return;
        for (const channel of this.#open) channel.notify("history/changed", { days: null });
      }),
    ];
  }

  /** A channel to `webview`, which also tells it the folders VS Code has open when `folders` is
   * set; dispose of it with the webview. */
  open(webview: vscode.Webview, { folders = false }: { folders?: boolean } = {}): Channel {
    const channel = new Channel(webview, this, folders, () => this.#open.delete(channel));
    this.#open.add(channel);
    return channel;
  }

  dispose(): void {
    for (const channel of this.#open) channel.dispose();
    for (const subscription of this.#subscriptions) subscription.dispose();
  }
}

export class Channel implements vscode.Disposable {
  /** Whether the web app in the webview listens yet. */
  #ready = false;
  #disposed = false;
  /** A section asked for before the web app listened. */
  #waitingShow: Show | undefined;
  readonly #subscriptions: vscode.Disposable[];

  constructor(
    private readonly webview: vscode.Webview,
    private readonly channels: Channels,
    /** Whether the webview is told the folders VS Code has open. */
    private readonly folders: boolean,
    private readonly closed: () => void,
  ) {
    this.#subscriptions = [
      webview.onDidReceiveMessage((message: ToExtension) => {
        void this.#received(message);
      }),
    ];
    if (folders) {
      this.#subscriptions.push(
        vscode.workspace.onDidChangeWorkspaceFolders(() => {
          this.post({ type: "folders", ...openFolders() });
        }),
      );
    }
  }

  /** Shows `show` in the webview, once the web app listens. */
  show(show: Show): void {
    if (this.#ready) this.post({ type: "show", show });
    else this.#waitingShow = show;
  }

  notify<N extends keyof Notifications>(method: N, params: Notifications[N]): void {
    this.post({ type: "notification", method, params });
  }

  post(message: FromExtension): void {
    if (this.#ready && !this.#disposed) void this.webview.postMessage(message);
  }

  /** Stops passing messages. An export the webview asked for goes on, since the person chose its
   * file, and its outcome goes nowhere. */
  dispose(): void {
    if (this.#disposed) return;
    this.#disposed = true;
    this.closed();
    for (const subscription of this.#subscriptions) subscription.dispose();
  }

  async #received(message: ToExtension): Promise<void> {
    switch (message.type) {
      case "ready":
        this.#ready = true;
        if (this.folders) this.post({ type: "folders", ...openFolders() });
        if (this.#waitingShow) this.post({ type: "show", show: this.#waitingShow });
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
        this.post(
          path ? { type: "picked", id: message.id, path } : { type: "picked", id: message.id },
        );
        return;
      }
    }
  }

  async #request(id: number, method: Method, params: unknown): Promise<void> {
    if (!Object.hasOwn(DASHBOARD_REQUESTS, method)) {
      this.post({
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
      const result: unknown = await this.channels.server.request(
        method,
        params as Params<typeof method>,
      );
      this.post({ type: "response", id, result });
      if (SETTINGS_CHANGES.includes(method)) void this.channels.summary.refresh(true);
    } catch (error) {
      this.post({ type: "response", id, error: failure(error) });
    }
  }

  /** Saves an export where the person chooses, and tells the web app how it went. */
  async #export(id: number, params: ExportParams): Promise<void> {
    if (this.channels.exporting) {
      this.post({
        type: "exported",
        id,
        error: {
          code: ErrorCodes.InvalidParams,
          message: "An export is under way; wait for it to finish, or cancel it.",
        },
      });
      return;
    }
    this.channels.exporting = { channel: this, cancelled: false };
    try {
      const result = await this.#saveExport(params);
      this.post(result ? { type: "exported", id, result } : { type: "exported", id });
    } catch (error) {
      this.post({ type: "exported", id, error: failure(error) });
    } finally {
      this.channels.exporting = undefined;
    }
  }

  /** The export as saved, or `undefined` when the person chooses no file. */
  async #saveExport(params: ExportParams): Promise<Exported | undefined> {
    const { server } = this.channels;
    const { name } = await server.request("export/name", params);
    this.#stopIfCancelled();
    const file = await chooseFile(name, params.format);
    if (!file) return undefined;
    this.#stopIfCancelled();
    // A `file:` URI names a file on the machine the server runs on, even in a remote window, so
    // the server writes it. Any other, such as the local computer's from a remote window, only VS
    // Code can write.
    if (file.scheme === "file") {
      return server.extensionRequest("export/save", { export: params, path: file.fsPath });
    }
    const { rows, what, content } = await server.extensionRequest("export/content", params);
    if (content == null) throw new Error("claude-dashboard returned no content to save.");
    await vscode.workspace.fs.writeFile(file, Buffer.from(content, "utf8"));
    return { rows, what };
  }

  /** Ends an export that the person cancelled before the server began it, as the server ends one
   * it has begun. */
  #stopIfCancelled(): void {
    if (this.channels.exporting?.cancelled) {
      throw new ResponseError(REQUEST_CANCELLED, "The export was cancelled.");
    }
  }

  /** Cancels the export under way, when this webview asked for it. */
  async #cancelExport(): Promise<void> {
    const exporting = this.channels.exporting;
    if (exporting?.channel !== this) return;
    exporting.cancelled = true;
    try {
      await this.channels.server.extensionRequest("export/cancel", null);
    } catch {
      // The server has ended, and the export with it.
    }
  }
}

/** The folders VS Code has open on the machine the server runs on, which a remote window's are
 * too. A folder that is a link to another is named both ways, since Claude Code may have run in
 * either. */
function openFolders(): OpenFolders {
  const folders = (vscode.workspace.workspaceFolders ?? [])
    .filter((folder) => folder.uri.scheme === "file" || folder.uri.scheme === "vscode-remote")
    .flatMap((folder) => {
      const path = folder.uri.fsPath;
      try {
        return [path, fs.realpathSync.native(path)];
      } catch {
        return [path];
      }
    });
  return {
    folders: [...new Set(folders)],
    ignoreCase: process.platform === "win32" || process.platform === "darwin",
  };
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
