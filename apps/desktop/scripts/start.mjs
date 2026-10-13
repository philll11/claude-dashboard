// Starts the desktop app, built for a target, as a release publishes it, or installed from an
// installer, where no WebDriver can drive it, on a synthetic Claude Code folder and a history of its
// own that starts empty. It checks that the app keeps running, reads the transcripts into the
// history, makes its item in the menu bar or the notification area, and shows its window once the
// dashboard has drawn rather than through its fallback. On macOS it keeps a screenshot of the
// screen as well. An app it installs, it uninstalls again.
//
//   node scripts/start.mjs --target aarch64-apple-darwin
//   node scripts/start.mjs --app "<folder>/Dashboard for Claude Code.app"
//   node scripts/start.mjs --installer <setup.exe or .deb>

import { execFileSync, spawn } from "node:child_process";
import { existsSync, globSync, mkdirSync } from "node:fs";
import { rm } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import { prepare } from "@claude-dashboard/app/testing";

import { newAppFolders } from "./app-folders.mjs";
import { install } from "./installer.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const repository = join(root, "..", "..");
const { values } = parseArgs({
  options: { target: { type: "string" }, app: { type: "string" }, installer: { type: "string" } },
});
if ([values.target, values.app, values.installer].filter(Boolean).length !== 1) {
  throw new Error(
    "--target names a build to start, --app an app, or --installer an installer; one of them.",
  );
}

/** The app's program inside a macOS app bundle, or the program itself elsewhere. */
function program(app) {
  return process.platform === "darwin" ? join(app, "Contents/MacOS/claude-dashboard-desktop") : app;
}

/** The app as built for `target`, with the claude-dashboard program Tauri put next to it. */
function built(target) {
  const release = join(repository, "target", target, "release");
  return process.platform === "darwin"
    ? globSync(
        `${release}/bundle/macos/*.app/Contents/MacOS/claude-dashboard-desktop`.replaceAll(
          "\\",
          "/",
        ),
      )[0]
    : join(release, `claude-dashboard-desktop${process.platform === "win32" ? ".exe" : ""}`);
}

const prepared = await prepare();
const installed =
  values.installer && install(resolve(values.installer), join(prepared.folder, "app"));
const app = installed ? installed.app : values.app ? program(values.app) : built(values.target);
if (!app || !existsSync(app)) {
  if (installed) installed.uninstall();
  throw new Error(`There is no app at ${app ?? `the build for ${values.target}`}.`);
}

const history = join(prepared.folder, "started", "history.sqlite");
mkdirSync(dirname(history), { recursive: true });
const screenshots = join(root, "out/screenshots");
mkdirSync(screenshots, { recursive: true });

/** Long enough for the app's fallback, which shows the window after five seconds if the dashboard
 * has not drawn by then (`start.rs`), to have shown it. */
const PAST_FALLBACK_MS = 7000;

const created = newAppFolders();
const started = Date.now();
const child = spawn(app, [], {
  env: {
    ...prepared.env,
    CLAUDE_DASHBOARD_HISTORY: history,
    // WebView2 keeps the window's storage in a folder of the check's own.
    WEBVIEW2_USER_DATA_FOLDER: join(prepared.folder, "webview"),
  },
  stdio: ["ignore", "inherit", "pipe"],
});
// The app's log, which says when the window showed only through the fallback.
let log = "";
child.stderr.on("data", (chunk) => {
  process.stderr.write(chunk);
  log += String(chunk);
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
while (ended === undefined && Date.now() - started < PAST_FALLBACK_MS) await sleep(500);
if (process.platform === "darwin" && ended === undefined) {
  execFileSync("screencapture", ["-x", join(screenshots, "started.png")]);
}
// Whether the app ended before this check stops it.
const endedEarly = ended;
child.kill();
if (installed) {
  // The app ends with the check; give it a moment before uninstalling it.
  await sleep(2000);
  installed.uninstall();
}
// WebView2's processes outlive the app by a moment, holding files in the folder, and only the
// asynchronous rm waits for them on Windows.
await rm(prepared.folder, { recursive: true, force: true, maxRetries: 20, retryDelay: 500 });
await created.remove();

if (endedEarly !== undefined) {
  throw new Error(`The app ended by itself (${String(endedEarly)}).`);
}
if (read === 0) throw new Error("The app read no requests into the history within 2 minutes.");
if (log.includes("did not draw in time")) {
  throw new Error(
    "The window showed only through its fallback: the dashboard never said it had drawn.",
  );
}
if (!log.includes("Added the item to the notification area.")) {
  throw new Error("The app made no item in the notification area or the menu bar.");
}
console.log(`The app started and read ${String(read)} requests into its history.`);
