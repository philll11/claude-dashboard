// Packages the extension with vsce: for one platform with its claude-dashboard program, or for no
// platform, as the package VS Code installs where no other fits, which explains that the platform
// needs `claudeDashboard.server.path`.
//
//   node scripts/package.mjs --target win32-x64 --program <claude-dashboard.exe> [--out <file>]
//   node scripts/package.mjs [--out <file>]

import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, mkdirSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const repository = join(root, "..", "..");
const { values } = parseArgs({
  options: {
    target: { type: "string" },
    program: { type: "string" },
    out: { type: "string" },
  },
});
if (Boolean(values.target) !== Boolean(values.program)) {
  throw new Error("--target and --program go together.");
}

execFileSync(process.execPath, [join(root, "scripts/build.mjs"), "--production"], {
  stdio: "inherit",
});
for (const file of ["LICENSE", "CHANGELOG.md"])
  copyFileSync(join(repository, file), join(root, file));

const server = join(root, "server");
rmSync(server, { recursive: true, force: true });
if (values.program) {
  const name = values.target.startsWith("win32-") ? "claude-dashboard.exe" : "claude-dashboard";
  mkdirSync(server);
  copyFileSync(resolve(values.program), join(server, name));
  chmodSync(join(server, name), 0o755);
}

const vsce = createRequire(import.meta.url).resolve("@vscode/vsce/vsce");
execFileSync(
  process.execPath,
  [
    vsce,
    "package",
    "--no-dependencies",
    ...(values.target ? ["--target", values.target] : []),
    ...(values.out ? ["--out", resolve(values.out)] : []),
  ],
  { cwd: root, stdio: "inherit" },
);
