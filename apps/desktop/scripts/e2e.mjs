// End-to-end tests: installs the desktop app from its installer, as a person would, and drives it
// through tauri-driver on a synthetic Claude Code folder and a history of its own. Tauri's driver
// runs on Windows, with Microsoft Edge WebDriver matching the WebView2 runtime, which this
// downloads from Microsoft unless --native-driver names one, and on Linux, with WebKitWebDriver;
// macOS has none.
//
// On Windows the app installs into the test's own folder and is uninstalled afterwards; on Linux the
// .deb package installs with dpkg, which needs sudo.
//
//   node scripts/e2e.mjs --installer <setup.exe or .deb> [--native-driver <msedgedriver.exe>]

import { execFileSync, spawn } from "node:child_process";
import { existsSync, globSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { rm } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import { prepare } from "@claude-dashboard/app/testing";
import { build } from "esbuild";
import Mocha from "mocha";

import { newAppFolders } from "./app-folders.mjs";
import { install } from "./installer.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const { values } = parseArgs({
  options: { installer: { type: "string" }, "native-driver": { type: "string" } },
});
if (!values.installer) throw new Error("--installer names the installer to test.");
const installer = resolve(values.installer);
const windows = process.platform === "win32";

// Glob patterns take forward slashes on every system.
const tests = join(root, "out/test/e2e/*.test.cjs").replaceAll("\\", "/");
await build({
  entryPoints: [join(root, "test/e2e/*.test.ts")],
  outdir: join(root, "out/test/e2e"),
  // The package is an ES module one, so the CommonJS tests take the .cjs extension.
  outExtension: { ".js": ".cjs" },
  bundle: true,
  format: "cjs",
  platform: "node",
  target: "node24",
  packages: "external",
  logLevel: "warning",
});
if (globSync(tests).length === 0) throw new Error(`No tests match ${tests}.`);

/** Where Windows records the WebView2 runtime's version: for the computer, or for the person. */
const WEBVIEW2_KEYS = [
  "HKLM\\SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
  "HKCU\\Software\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
];

/** Microsoft Edge WebDriver for the WebView2 runtime this computer has, downloaded once. */
async function edgeDriver() {
  let version;
  for (const key of WEBVIEW2_KEYS) {
    try {
      const output = execFileSync("reg", ["query", key, "/v", "pv"], { encoding: "utf8" });
      version = /pv\s+REG_SZ\s+(\S+)/.exec(output)?.[1];
    } catch {
      // The other key holds it.
    }
    if (version) break;
  }
  if (!version) throw new Error("This computer has no WebView2 runtime.");
  console.log(`WebView2 ${version}, driven by Microsoft Edge WebDriver ${version}.`);
  const folder = join(root, "out/drivers", version);
  const driver = join(folder, "msedgedriver.exe");
  if (existsSync(driver)) return driver;
  const architecture = process.arch === "arm64" ? "arm64" : "win64";
  const url = `https://msedgedriver.microsoft.com/${version}/edgedriver_${architecture}.zip`;
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url} answered ${String(response.status)}.`);
  mkdirSync(folder, { recursive: true });
  const zip = join(folder, "edgedriver.zip");
  writeFileSync(zip, Buffer.from(await response.arrayBuffer()));
  // Windows' own tar reads zip files; another tar earlier on the PATH, such as Git's, may not.
  const tar = join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe");
  execFileSync(tar, ["-xf", zip, "-C", folder]);
  return driver;
}

const nativeDriver = values["native-driver"] ?? (windows ? await edgeDriver() : undefined);
const prepared = await prepare();
// The tests end the app by closing its window, so it quits then rather than keep running in the
// notification area; the one test of keeping it running switches that on and off again itself.
const history = new DatabaseSync(prepared.history);
history.prepare("INSERT INTO settings (name, value) VALUES ('keep_running', 'off')").run();
history.close();
const screenshots = join(root, "out/screenshots");
rmSync(screenshots, { recursive: true, force: true });
mkdirSync(screenshots, { recursive: true });
// The app saves exports here without its save dialog, which a test cannot answer.
const exportFolder = join(prepared.folder, "exports");
mkdirSync(exportFolder);

// Whatever fails, the app is uninstalled again, with the folders it created.
const created = newAppFolders();
let installed;
let driver;
let log = "";
let failures = 1;
try {
  installed = install(installer, join(prepared.folder, "app"));
  if (!existsSync(installed.app)) throw new Error(`The installer put no app at ${installed.app}.`);
  // tauri-driver starts the app for each session, in its own environment; on Windows, Microsoft
  // Edge WebDriver gives each session a WebView2 profile of its own. The drivers keep their
  // temporary files, such as those profiles, in the test's folder, which goes with it.
  const temporary = join(prepared.folder, "temp");
  mkdirSync(temporary);
  driver = spawn(
    "tauri-driver",
    ["--port", "4444", ...(nativeDriver ? ["--native-driver", nativeDriver] : [])],
    {
      env: {
        ...prepared.env,
        CLAUDE_DASHBOARD_EXPORT_TO: exportFolder,
        TEMP: temporary,
        TMP: temporary,
        TMPDIR: temporary,
      },
      stdio: ["ignore", "inherit", "pipe"],
    },
  );
  // What the drivers and the app they start write, which on Linux includes the app's log.
  driver.stderr.on("data", (chunk) => {
    process.stderr.write(chunk);
    log += String(chunk);
  });
  await new Promise((resolve, reject) => {
    driver.on("error", reject);
    setTimeout(resolve, 1500);
  });
  process.env.E2E_APP = installed.app;
  process.env.E2E_PROGRAM = prepared.program;
  process.env.E2E_FOLDER = prepared.folder;
  process.env.E2E_ROOT = prepared.root;
  process.env.E2E_SCREENSHOTS = screenshots;
  process.env.E2E_EXPORTS = exportFolder;
  process.env.E2E_ENV = JSON.stringify(prepared.env);
  const mocha = new Mocha({ ui: "bdd", timeout: 120_000, reporter: "spec" });
  for (const file of globSync(tests).sort()) mocha.addFile(file);
  failures = await new Promise((resolve) => mocha.run(resolve));
  // The app shows its window once the dashboard has drawn, and only otherwise through the fallback
  // that start.rs logs.
  if (log.includes("did not draw in time")) {
    console.error(
      "The window showed only through its fallback: the dashboard never said it had drawn.",
    );
    failures += 1;
  }
} finally {
  driver?.kill();
  if (installed) {
    // The app ends with the WebDriver session; give it a moment before uninstalling it.
    await new Promise((resolve) => setTimeout(resolve, 2000));
    installed.uninstall();
    // Uninstalling the app leaves the history, which every other surface shares.
    if (!existsSync(prepared.history)) {
      console.error("Uninstalling the app removed the history.");
      failures += 1;
    }
  }
  // WebView2's processes outlive the app by a moment, holding files in the folder, and only the
  // asynchronous rm waits for them on Windows.
  await rm(prepared.folder, { recursive: true, force: true, maxRetries: 20, retryDelay: 500 });
  await created.remove();
}
process.exitCode = failures > 0 ? 1 : 0;
