// Starts the desktop app built for a target, where no WebDriver can drive it, on a synthetic Claude
// Code folder and a history of its own that starts empty, and checks that the app keeps running and
// reads the transcripts into the history. On macOS it keeps a screenshot of the screen as well.
//
//   node scripts/start.mjs --target aarch64-apple-darwin

import { execFileSync, spawn } from "node:child_process";
import { existsSync, globSync, mkdirSync } from "node:fs";
import { rm } from "node:fs/promises";
import { dirname, join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import { prepare } from "@claude-dashboard/app/testing";

import { newAppFolders } from "./app-folders.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const repository = join(root, "..", "..");
const { values } = parseArgs({ options: { target: { type: "string" } } });
if (!values.target) throw new Error("--target names the build to start.");

// The app as built, with the claude-dashboard program Tauri put next to it.
const release = join(repository, "target", values.target, "release");
const app =
  process.platform === "darwin"
    ? globSync(
        `${release}/bundle/macos/*.app/Contents/MacOS/claude-dashboard-desktop`.replaceAll(
          "\\",
          "/",
        ),
      )[0]
    : join(release, `claude-dashboard-desktop${process.platform === "win32" ? ".exe" : ""}`);
if (!app || !existsSync(app)) throw new Error(`There is no app built for ${values.target}.`);

const prepared = prepare();
const history = join(prepared.folder, "started", "history.sqlite");
mkdirSync(dirname(history), { recursive: true });
const screenshots = join(root, "out/screenshots");
mkdirSync(screenshots, { recursive: true });

const created = newAppFolders();
const child = spawn(app, [], {
  env: {
    ...prepared.env,
    CLAUDE_DASHBOARD_HISTORY: history,
    // WebView2 keeps the window's storage in a folder of the check's own.
    WEBVIEW2_USER_DATA_FOLDER: join(prepared.folder, "webview"),
  },
  stdio: "inherit",
});
let ended;
child.on("exit", (code, signal) => {
  ended = code ?? signal;
});

/** The requests the history holds, once the app has created it. */
function requests() {
  if (!existsSync(history)) return 0;
  try {
    const database = new DatabaseSync(history, { readOnly: true });
    try {
      return Number(database.prepare("SELECT count(*) AS n FROM requests").get().n);
    } finally {
      database.close();
    }
  } catch {
    // The app is still creating it.
    return 0;
  }
}

let read = 0;
const deadline = Date.now() + 120_000;
while (Date.now() < deadline && ended === undefined && read === 0) {
  await sleep(1000);
  read = requests();
}
// The window has drawn by the time the transcripts are read; the screenshot shows it.
if (process.platform === "darwin" && ended === undefined) {
  await sleep(3000);
  execFileSync("screencapture", ["-x", join(screenshots, "started.png")]);
}
// Whether the app ended before this check stops it.
const endedEarly = ended;
child.kill();
// WebView2's processes outlive the app by a moment, holding files in the folder, and only the
// asynchronous rm waits for them on Windows.
await rm(prepared.folder, { recursive: true, force: true, maxRetries: 20, retryDelay: 500 });
await created.remove();

if (endedEarly !== undefined) {
  throw new Error(`The app ended by itself (${String(endedEarly)}).`);
}
if (read === 0) throw new Error("The app read no requests into the history within 2 minutes.");
console.log(`The app started and read ${String(read)} requests into its history.`);
