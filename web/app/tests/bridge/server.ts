// Runs `claude-dashboard serve --stdio` over a synthetic Claude Code folder for the dashboard's
// previews and browser tests, speaking JSON-RPC to it the way the VS Code extension does.
//
// The server runs at a fixed moment (CLAUDE_DASHBOARD_NOW) in a fixed time zone, on a history of
// its own and a Claude Code folder of its own, so it never reads or changes the person's.

import { spawn } from "node:child_process";
import { existsSync, mkdirSync, realpathSync } from "node:fs";
import { rm } from "node:fs/promises";
import { join } from "node:path";

import {
  createMessageConnection,
  type MessageConnection,
  StreamMessageReader,
  StreamMessageWriter,
} from "vscode-jsonrpc/node";

import { prepare, type Setup } from "./folder.ts";
import type { Options } from "./synthetic.ts";

/** Where the dashboard is shown the folders and the history, whatever the machine. */
export const SHOWN_ROOT = "/home/you/.claude";
export const SHOWN_HISTORY = "/home/you/.local/share/claude-dashboard/history.sqlite";
export const SHOWN_PROGRAM = "/home/you/.local/bin/claude-dashboard";
export const SHOWN_ADDED_FOLDER = "//wsl.localhost/Ubuntu/home/you/.claude";
export const SHOWN_DOWNLOADS = "/home/you/Downloads";
/** The version the dashboard is shown, whatever the program's, so that screenshots stay the same
 * from one release to the next. */
export const SHOWN_VERSION = "0.1.0-alpha.1";

export interface Scenario {
  /** The variant of the synthetic folder. */
  options?: Options;
  /** No Claude Code folder at all. */
  empty?: boolean;
  /** Changes the server's answers, for a state that real data cannot hold still for a test. */
  patch?: (method: string, result: unknown) => unknown;
  /** The server cannot read the history, so it answers every request with why. */
  unreadable?: boolean;
}

type Listener = (method: string, params: unknown) => void;

/** The ways a path can appear in the server's answers: as given, as the operating system
 * resolves it, and with forward slashes. */
function spellings(path: string): string[] {
  const resolved = existsSync(path) ? realpathSync.native(path) : path;
  return [...new Set([path, resolved].flatMap((each) => [each, each.replaceAll("\\", "/")]))];
}

export class Bridge {
  private readonly listeners = new Set<Listener>();
  /** Each text the dashboard was shown with a path replaced, and the text the server sent. */
  private readonly originals = new Map<string, string>();

  private constructor(
    private readonly folder: string,
    /** Where exports are saved, as a save dialog would. */
    private readonly downloads: string,
    private readonly connection: MessageConnection,
    private readonly stopServer: () => Promise<void>,
    private readonly scenario: Scenario,
    private readonly shown: [actual: string, shown: string][],
  ) {
    connection.onNotification((method, params: unknown) => {
      for (const listener of this.listeners) listener(method, this.shownAs(params));
    });
  }

  /** Writes the scenario's Claude Code folder, reads it into a new history, and starts a server
   * on them. */
  static async start(scenario: Scenario = {}): Promise<Bridge> {
    const setup: Setup = { empty: scenario.empty ?? false };
    if (scenario.options) setup.options = scenario.options;
    const { folder, root, history, program, synthetic, env } = await prepare(setup);

    const server = spawn(program, ["serve", "--stdio"], {
      env,
      stdio: ["pipe", "pipe", "inherit"],
    });
    const connection = createMessageConnection(
      new StreamMessageReader(server.stdout),
      new StreamMessageWriter(server.stdin),
    );
    connection.listen();
    const downloads = join(folder, "Downloads");
    mkdirSync(downloads, { recursive: true });
    const shown: [string, string][] = [
      ...spellings(downloads).map((path): [string, string] => [path, SHOWN_DOWNLOADS]),
      ...spellings(program).map((path): [string, string] => [path, SHOWN_PROGRAM]),
      ...spellings(history).map((path): [string, string] => [path, SHOWN_HISTORY]),
      ...synthetic.folders.flatMap((folder) =>
        spellings(folder).map((path): [string, string] => [path, SHOWN_ADDED_FOLDER]),
      ),
      ...spellings(root).map((path): [string, string] => [path, SHOWN_ROOT]),
    ];
    const exited = new Promise<void>((resolve) =>
      server.once("exit", () => {
        resolve();
      }),
    );
    const bridge = new Bridge(
      folder,
      downloads,
      connection,
      async () => {
        connection.dispose();
        server.stdin.end();
        const timeout = setTimeout(() => server.kill(), 5_000);
        await exited;
        clearTimeout(timeout);
      },
      scenario,
      shown,
    );
    return bridge;
  }

  async request(method: string, params: unknown): Promise<unknown> {
    const result: unknown = await this.connection.sendRequest(method, this.sentAs(params));
    const patched = this.scenario.patch ? this.scenario.patch(method, result) : result;
    const versioned =
      method === "status" && patched !== null && typeof patched === "object"
        ? { ...patched, version: SHOWN_VERSION }
        : patched;
    return this.shownAs(versioned);
  }

  /** Saves an export in the Downloads folder under the name the server suggests, as a person
   * accepting the save dialog would. */
  async exportTo(params: unknown): Promise<unknown> {
    const { name } = await this.connection.sendRequest<{ name: string }>("export/name", params);
    const path = join(this.downloads, name);
    return this.shownAs(await this.connection.sendRequest("export/save", { export: params, path }));
  }

  async cancelExport(): Promise<void> {
    await this.connection.sendRequest("export/cancel", null);
  }

  onNotification(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  async stop(): Promise<void> {
    await this.stopServer();
    // Only the asynchronous rm retries on Windows, where a process that has just ended can still hold files.
    await rm(this.folder, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  }

  /** `value` with the temporary folders' paths replaced by the ones shown. */
  private shownAs(value: unknown): unknown {
    if (typeof value === "string") return this.shownPath(value);
    if (Array.isArray(value)) return value.map((item) => this.shownAs(item));
    if (value !== null && typeof value === "object") {
      return Object.fromEntries(
        Object.entries(value).map(([key, item]) => [key, this.shownAs(item)]),
      );
    }
    return value;
  }

  private shownPath(text: string): string {
    let shown = text;
    for (const [actual, instead] of this.shown) {
      for (let index = shown.indexOf(actual); index >= 0; index = shown.indexOf(actual)) {
        const rest = shown.slice(index + actual.length);
        const path = /^[^\s"'`]*/.exec(rest)?.[0] ?? "";
        shown =
          shown.slice(0, index) + instead + path.replaceAll("\\", "/") + rest.slice(path.length);
      }
    }
    if (shown !== text) this.originals.set(shown, text);
    return shown;
  }

  /** `value` with every text the dashboard was shown back as the server sent it. */
  private sentAs(value: unknown): unknown {
    if (typeof value === "string") return this.originals.get(value) ?? value;
    if (Array.isArray(value)) return value.map((item) => this.sentAs(item));
    if (value !== null && typeof value === "object") {
      return Object.fromEntries(
        Object.entries(value).map(([key, item]) => [key, this.sentAs(item)]),
      );
    }
    return value;
  }
}
