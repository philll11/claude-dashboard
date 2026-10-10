// Runs `claude-dashboard serve --stdio` over a synthetic Claude Code folder for the dashboard's
// previews and browser tests, speaking JSON-RPC to it the way the VS Code extension does.
//
// The server runs at a fixed moment (CLAUDE_DASHBOARD_NOW) in a fixed time zone, on a history of
// its own and a Claude Code folder of its own, so it never reads or changes the person's.

import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";

import {
  createMessageConnection,
  type MessageConnection,
  StreamMessageReader,
  StreamMessageWriter,
} from "vscode-jsonrpc/node";

import { NOW_MS, type Options, type Synthetic, TIME_ZONE, writeClaudeFolder } from "./synthetic";

/** Where the dashboard is shown the folders and the history, whatever the machine. */
export const SHOWN_ROOT = "/home/you/.claude";
export const SHOWN_HISTORY = "/home/you/.local/share/claude-dashboard/history.sqlite";

export interface Scenario {
  /** The variant of the synthetic folder. */
  options?: Options;
  /** No Claude Code folder at all. */
  empty?: boolean;
  /** Changes the server's answers, for a state that real data cannot hold still for a test. */
  patch?: (method: string, result: unknown) => unknown;
}

type Listener = (method: string, params: unknown) => void;

function binary(): string {
  const fromEnvironment = process.env.CLAUDE_DASHBOARD_BIN;
  if (fromEnvironment) return fromEnvironment;
  const name = process.platform === "win32" ? "claude-dashboard.exe" : "claude-dashboard";
  const built = fileURLToPath(new URL(`../../../../target/debug/${name}`, import.meta.url));
  if (!existsSync(built)) {
    throw new Error(
      `${built} does not exist: build it with \`cargo build -p claude-dashboard\`, or set CLAUDE_DASHBOARD_BIN`,
    );
  }
  return built;
}

/** The ways a path can appear in the server's answers. */
function spellings(path: string): string[] {
  return [...new Set([path, path.replaceAll("\\", "/")])];
}

function seed(history: string, synthetic: Synthetic): void {
  const database = new DatabaseSync(history);
  try {
    const insert = database.prepare(
      `INSERT OR REPLACE INTO limit_snapshots
         (window, source, bucket_ms, observed_ms, used_percentage, resets_at_ms)
       VALUES (?, ?, ?, ?, ?, ?)`,
    );
    for (const reading of synthetic.readings) {
      insert.run(
        reading.window,
        reading.source,
        Math.floor(reading.observedMs / 300_000) * 300_000,
        reading.observedMs,
        reading.usedPercentage,
        reading.resetsAtMs,
      );
    }
    if (synthetic.endpoint) {
      database
        .prepare(
          `UPDATE usage_endpoint
             SET enabled = 1, asked_ms = ?, outcome = ?, next_ask_ms = ?
           WHERE id = 1`,
        )
        .run(synthetic.endpoint.askedMs, synthetic.endpoint.outcome, synthetic.endpoint.nextAskMs);
    }
  } finally {
    database.close();
  }
}

export class Bridge {
  private readonly listeners = new Set<Listener>();

  private constructor(
    private readonly folder: string,
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
  static start(scenario: Scenario = {}): Promise<Bridge> {
    const folder = mkdtempSync(join(tmpdir(), "claude-dashboard-bridge-"));
    const root = join(folder, ".claude");
    const history = join(folder, "history.sqlite");
    const synthetic = scenario.empty
      ? { readings: [], endpoint: null }
      : writeClaudeFolder(root, scenario.options);
    const env = {
      ...process.env,
      CLAUDE_CONFIG_DIR: root,
      CLAUDE_DASHBOARD_HISTORY: history,
      CLAUDE_DASHBOARD_NOW: new Date(NOW_MS).toISOString(),
      TZ: TIME_ZONE,
      NO_COLOR: "1",
    };
    // A copy, so that the build can replace the binary while a preview runs.
    const program = join(folder, basename(binary()));
    copyFileSync(binary(), program);
    const read = spawnSync(program, ["daily", "--json"], { env, encoding: "utf8" });
    if (read.status !== 0) {
      throw new Error(`claude-dashboard could not read the synthetic folder: ${read.stderr}`);
    }
    seed(history, synthetic);

    const server = spawn(program, ["serve", "--stdio"], {
      env,
      stdio: ["pipe", "pipe", "inherit"],
    });
    const connection = createMessageConnection(
      new StreamMessageReader(server.stdout),
      new StreamMessageWriter(server.stdin),
    );
    connection.listen();
    const shown: [string, string][] = [
      ...spellings(history).map((path): [string, string] => [path, SHOWN_HISTORY]),
      ...spellings(root).map((path): [string, string] => [path, SHOWN_ROOT]),
    ];
    const exited = new Promise<void>((resolve) =>
      server.once("exit", () => {
        resolve();
      }),
    );
    const bridge = new Bridge(
      folder,
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
    return Promise.resolve(bridge);
  }

  async request(method: string, params: unknown): Promise<unknown> {
    const result: unknown = await this.connection.sendRequest(method, params);
    const patched = this.scenario.patch ? this.scenario.patch(method, result) : result;
    return this.shownAs(patched);
  }

  onNotification(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  async stop(): Promise<void> {
    await this.stopServer();
    rmSync(this.folder, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
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
    for (const [actual, shown] of this.shown) {
      const index = text.indexOf(actual);
      if (index < 0) continue;
      const rest = text.slice(index + actual.length);
      const path = /^[^\s"'`]*/.exec(rest)?.[0] ?? "";
      return this.shownPath(
        text.slice(0, index) + shown + path.replaceAll("\\", "/") + rest.slice(path.length),
      );
    }
    return text;
  }
}
