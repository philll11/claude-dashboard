// Builds the desktop app's installers for one target, with the claude-dashboard program it brings
// next to it (Tauri's externalBin, which src-tauri/tauri.bundle.json adds to the configuration),
// and copies them to out/bundles under names that say the version and the platform, such as
// claude-dashboard-desktop-0.2.0-windows-x64-setup.exe.
//
// With --updater it also writes the signed update bundles, with their signatures beside them, which
// need the signing key in TAURI_SIGNING_PRIVATE_KEY and its password in
// TAURI_SIGNING_PRIVATE_KEY_PASSWORD. Without it, as for the tests, the app it builds has nowhere to
// look for updates, so that it never installs a release over itself.
//
//   node scripts/bundle.mjs --target x86_64-pc-windows-msvc --program <claude-dashboard.exe>
//     [--bundles nsis] [--updater]

import { execFileSync } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  existsSync,
  globSync,
  mkdirSync,
  readFileSync,
  rmSync,
} from "node:fs";
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
    bundles: { type: "string" },
    updater: { type: "boolean", default: false },
  },
});
const target = values.target;
if (!target || !values.program) throw new Error("--target and --program are needed.");

/** The platform as the installers' names say it. */
const PLATFORMS = {
  "x86_64-pc-windows-msvc": "windows-x64",
  "aarch64-pc-windows-msvc": "windows-arm64",
  "x86_64-apple-darwin": "macos-x64",
  "aarch64-apple-darwin": "macos-arm64",
  "x86_64-unknown-linux-gnu": "linux-x64",
  "aarch64-unknown-linux-gnu": "linux-arm64",
};
const platform = PLATFORMS[target];
if (!platform) throw new Error(`The desktop app is not built for ${target}.`);

// Tauri looks for the program under its name followed by the target, and installs it without it.
const binaries = join(root, "src-tauri/binaries");
mkdirSync(binaries, { recursive: true });
const extension = target.includes("-windows-") ? ".exe" : "";
const program = join(binaries, `claude-dashboard-${target}${extension}`);
copyFileSync(resolve(values.program), program);
chmodSync(program, 0o755);

const tauri = createRequire(import.meta.url).resolve("@tauri-apps/cli/tauri.js");
execFileSync(
  process.execPath,
  [
    tauri,
    "build",
    "--target",
    target,
    "--config",
    "src-tauri/tauri.bundle.json",
    "--config",
    JSON.stringify(
      values.updater
        ? { bundle: { createUpdaterArtifacts: true } }
        : { plugins: { updater: { endpoints: [] } } },
    ),
    ...(values.bundles ? ["--bundles", values.bundles] : []),
  ],
  { cwd: root, stdio: "inherit" },
);

const version = /\[workspace\.package\][^[]*?version = "([^"]+)"/.exec(
  readFileSync(join(repository, "Cargo.toml"), "utf8"),
)?.[1];
if (!version) throw new Error("The workspace's Cargo.toml names no version.");

// The installers, and the update bundles the updater downloads; an installer that updates itself
// is its own update bundle.
const KINDS = [
  { pattern: "nsis/*-setup.exe", suffix: "-setup.exe" },
  { pattern: "macos/*.app.tar.gz", suffix: ".app.tar.gz" },
  { pattern: "dmg/*.dmg", suffix: ".dmg" },
  { pattern: "appimage/*.AppImage", suffix: ".AppImage" },
  { pattern: "deb/*.deb", suffix: ".deb" },
];
const built = join(repository, "target", target, "release", "bundle");
const out = join(root, "out", "bundles");
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
for (const { pattern, suffix } of KINDS) {
  // Glob patterns take forward slashes on every system.
  for (const file of globSync(`${built}/${pattern}`.replaceAll("\\", "/"))) {
    const name = `claude-dashboard-desktop-${version}-${platform}${suffix}`;
    copyFileSync(file, join(out, name));
    if (existsSync(`${file}.sig`)) copyFileSync(`${file}.sig`, join(out, `${name}.sig`));
    console.log(join(out, name));
  }
}
