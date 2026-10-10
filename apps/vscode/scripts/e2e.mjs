// End-to-end tests: installs a package of the extension into a fresh VS Code, as a person would,
// and checks what it shows, inside the dashboard's webview too, on a synthetic Claude Code folder.
// ExTester 8.27 drives VS Code 1.135 to 1.137, so the tests run on 1.137; the integration tests
// cover the oldest VS Code the extension supports. Screenshots land in test-resources/screenshots.
//
//   node scripts/e2e.mjs --vsix <package>

import { copyFileSync, globSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import { prepare } from "@claude-dashboard/app/testing";
import { build } from "esbuild";
import { ExTester, ReleaseQuality } from "vscode-extension-tester";

const VSCODE = "1.137.0";
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const { values } = parseArgs({ options: { vsix: { type: "string" } } });
if (!values.vsix) throw new Error("--vsix names the package to install.");

// Glob patterns take forward slashes on every system.
const tests = join(root, "out/test/e2e/*.test.js").replaceAll("\\", "/");
await build({
  entryPoints: [join(root, "test/e2e/*.test.ts")],
  outdir: join(root, "out/test/e2e"),
  bundle: true,
  format: "cjs",
  platform: "node",
  target: "node22",
  packages: "external",
  logLevel: "warning",
});
if (globSync(tests).length === 0) throw new Error(`No tests match ${tests}.`);

const storage = join(root, "test-resources");
const settings = join(storage, "e2e-settings.json");
mkdirSync(storage, { recursive: true });
copyFileSync(join(root, "test/e2e/settings.json"), settings);

const prepared = prepare();
for (const key of ["CLAUDE_CONFIG_DIR", "CLAUDE_DASHBOARD_HISTORY", "CLAUDE_DASHBOARD_NOW", "TZ"]) {
  process.env[key] = prepared.env[key];
}
process.env.E2E_SETTINGS = join(storage, "settings", "User", "settings.json");
process.env.E2E_SCREENSHOTS = join(storage, "screenshots");

let code;
try {
  const tester = new ExTester(storage, ReleaseQuality.Stable, join(storage, "extensions"));
  await tester.downloadCode(VSCODE);
  await tester.downloadChromeDriver(VSCODE);
  await tester.installVsix({ vsixFile: resolve(values.vsix), installDependencies: false });
  code = await tester.runTests(tests, {
    vscodeVersion: VSCODE,
    settings,
    resources: [prepared.folder],
  });
} finally {
  rmSync(prepared.folder, { recursive: true, force: true, maxRetries: 5, retryDelay: 500 });
}
process.exitCode = code;
