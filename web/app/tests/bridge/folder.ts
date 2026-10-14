// A synthetic Claude Code folder read into a new history, in a temporary folder of its own, for
// whatever runs claude-dashboard on it: the dashboard's previews and browser tests, and the VS Code
// extension's tests. claude-dashboard runs at a fixed moment (CLAUDE_DASHBOARD_NOW) in a fixed time
// zone, so it never reads or changes the person's Claude Code folder or history.
//
// Node loads this file as it is, so it keeps to TypeScript that only needs its types removed.

import { execFile } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, rmSync } from "node:fs";
import { copyFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { Worker } from "node:worker_threads";

import { NOW_MS, type Options, type Synthetic, TIME_ZONE } from "./synthetic.ts";

/** What to write: the variant of the synthetic folder, or no Claude Code folder at all. */
export interface Setup {
  options?: Options;
  empty?: boolean;
  /** Puts the projects' working folders in a real home folder, so that an editor can open one. */
  home?: boolean;
}

export interface Prepared {
  /** The temporary folder that holds everything else. */
  folder: string;
  /** The Claude Code folder. */
  root: string;
  /** With `home`, the folder that stands for `/home/you`, whose `code/claude-dashboard` holds the
   * session that is running. */
  home?: string;
  history: string;
  /** This process's copy of claude-dashboard, which a build can replace the original of
   * meanwhile. */
  program: string;
  synthetic: Synthetic;
  /** The environment claude-dashboard runs in. */
  env: NodeJS.ProcessEnv;
}

/** The start of every such folder's name; the process id that follows says whose it is. */
const FOLDER_PREFIX = "claude-dashboard-bridge-";

function running(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code === "EPERM";
  }
}

/** Removes the folders of processes that ended without removing them, as happens when a test run
 * or a preview is stopped. */
let swept = false;
function sweep(): void {
  if (swept) return;
  swept = true;
  for (const name of readdirSync(tmpdir())) {
    if (!name.startsWith(FOLDER_PREFIX)) continue;
    const pid = Number(name.slice(FOLDER_PREFIX.length).split("-")[0]);
    if (Number.isInteger(pid) && pid !== process.pid && !running(pid)) {
      rmSync(join(tmpdir(), name), { recursive: true, force: true });
    }
  }
}

/** The claude-dashboard program that `CLAUDE_DASHBOARD_BIN` names, or the debug build. */
export function binary(): string {
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

/** Records the plan limit readings and the other state that only a running Claude Code, or a
 * person's choices, would leave in the history. */
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
    const addFolder = database.prepare("INSERT INTO data_folders (path, added_ms) VALUES (?, ?)");
    for (const folder of synthetic.folders) addFolder.run(folder, NOW_MS);
    if (synthetic.schema !== null)
      database.exec(`PRAGMA user_version = ${String(synthetic.schema)}`);
    if (synthetic.readableFrom !== null)
      database.exec(`UPDATE compatibility SET readable_from = ${String(synthetic.readableFrom)}`);
  } finally {
    database.close();
  }
}

const run = promisify(execFile);

/** Writes the synthetic Claude Code folder at `root` on a thread of its own: writing one takes the
 * best part of a second, which would otherwise hold up everything else the process does. */
function writeFolder(root: string, options: Options | undefined): Promise<Synthetic> {
  return new Promise((resolve, reject) => {
    const worker = new Worker(new URL("./folder-writer.ts", import.meta.url), {
      workerData: { root, options },
    });
    worker.once("message", (synthetic: Synthetic) => {
      resolve(synthetic);
    });
    worker.once("error", reject);
    worker.once("exit", (code) => {
      if (code !== 0)
        reject(new Error(`Writing the synthetic folder stopped with ${String(code)}.`));
    });
  });
}

/** This process's copy of claude-dashboard, made by the first folder prepared and removed as the
 * process ends. */
let copied: Promise<string> | undefined;
function programCopy(): Promise<string> {
  copied ??= (async () => {
    const folder = mkdtempSync(join(tmpdir(), `${FOLDER_PREFIX}${String(process.pid)}-`));
    process.once("exit", () => {
      try {
        rmSync(folder, { recursive: true, force: true });
      } catch {
        // On Windows a program still running from it holds it; a later sweep removes it.
      }
    });
    const program = join(folder, basename(binary()));
    await copyFile(binary(), program);
    return program;
  })();
  return copied;
}

/** Writes the Claude Code folder `setup` asks for and reads it into a new history, leaving the
 * process free meanwhile, as a server answering other requests needs. Remove `folder` once done. */
export async function prepare(setup: Setup = {}): Promise<Prepared> {
  sweep();
  const folder = mkdtempSync(join(tmpdir(), `${FOLDER_PREFIX}${String(process.pid)}-`));
  const root = join(folder, ".claude");
  const history = join(folder, "history.sqlite");
  const home = setup.home ? join(folder, "home") : undefined;
  const synthetic: Synthetic = setup.empty
    ? { readings: [], endpoint: null, folders: [], schema: null, readableFrom: null }
    : await writeFolder(root, home ? { ...setup.options, home } : setup.options);
  const env = {
    ...process.env,
    CLAUDE_CONFIG_DIR: root,
    CLAUDE_DASHBOARD_HISTORY: history,
    CLAUDE_DASHBOARD_NOW: new Date(NOW_MS).toISOString(),
    TZ: TIME_ZONE,
    NO_COLOR: "1",
  };
  const program = await programCopy();
  try {
    await run(program, ["daily", "--json"], { env, maxBuffer: 64 * 1024 * 1024 });
  } catch (error) {
    const { stderr } = error as { stderr?: unknown };
    throw new Error(
      `claude-dashboard could not read the synthetic folder: ${typeof stderr === "string" && stderr ? stderr : String(error)}`,
      { cause: error },
    );
  }
  seed(history, synthetic);
  return { folder, root, ...(home ? { home } : {}), history, program, synthetic, env };
}
