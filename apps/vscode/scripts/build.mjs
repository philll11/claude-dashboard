// Builds the extension: its code bundled into dist/extension.js, the dashboard's built web app
// copied to dist/webview for the webview to load, the sidebar's model swatches drawn into
// dist/swatches, and, with --tests, the integration tests bundled into out/. Build the web app
// first (`pnpm --recursive build` builds both, in order).

import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { build } from "esbuild";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const production = process.argv.includes("--production");
const tests = process.argv.includes("--tests");

const common = {
  bundle: true,
  format: "cjs",
  platform: "node",
  target: "node22",
  external: ["vscode", "mocha"],
  sourcemap: production ? false : "linked",
  minify: production,
  logLevel: "warning",
};

await build({
  ...common,
  entryPoints: [join(root, "src/extension.ts")],
  outfile: join(root, "dist/extension.js"),
});

const app = dirname(createRequire(import.meta.url).resolve("@claude-dashboard/app/package.json"));
const webview = join(root, "dist/webview");
rmSync(webview, { recursive: true, force: true });
mkdirSync(webview, { recursive: true });
for (const name of ["dashboard.js", "dashboard.css"]) {
  const built = join(app, "dist", name);
  if (!existsSync(built)) {
    throw new Error(`${built} is missing: build the web app first (pnpm --recursive build).`);
  }
  copyFileSync(built, join(webview, name));
}

// Each model series' swatch as the design draws it: a 10-pixel square with rounded corners, in the
// dark series' colour for dark and high contrast themes and the light series' for light ones, as
// design/tokens.json says. VS Code picks the file for the theme kind.
const tokens = JSON.parse(readFileSync(join(root, "../../design/tokens.json"), "utf8"));
const swatches = join(root, "dist/swatches");
rmSync(swatches, { recursive: true, force: true });
mkdirSync(swatches, { recursive: true });
for (const kind of ["dark", "light"]) {
  const colours = { ...tokens.series[kind], other: tokens.themes[`desktop-${kind}`].bar };
  for (const [series, colour] of Object.entries(colours)) {
    writeFileSync(
      join(swatches, `${series}-${kind}.svg`),
      `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><rect x="3" y="3" width="10" height="10" rx="2" fill="${colour}"/></svg>\n`,
    );
  }
}

if (tests) {
  rmSync(join(root, "out"), { recursive: true, force: true });
  await build({
    ...common,
    entryPoints: [join(root, "test/integration/*.test.ts")],
    outdir: join(root, "out/test/integration"),
  });
}
