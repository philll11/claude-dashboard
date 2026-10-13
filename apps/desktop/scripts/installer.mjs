// Installs the desktop app from its installer, as a person would, where a test or check can find
// it, and removes it again. On Windows it installs into a folder the caller names; on Linux the .deb
// package installs with dpkg, which needs sudo.
//
// A test copy registers itself as the app does: on Windows its entry in Apps & features, its
// shortcut and the folder the installer records, on Linux the package itself. Removing the copy
// would remove an installed app's, so it is never installed over one.

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const configuration = JSON.parse(
  readFileSync(
    join(dirname(fileURLToPath(import.meta.url)), "../src-tauri/tauri.conf.json"),
    "utf8",
  ),
);
const UNINSTALL_KEY = `HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\${configuration.productName}`;
const PUBLISHER_KEY = `HKCU\\Software\\${configuration.bundle.publisher}`;
/** Where the installer records the folder it installed into, which a later install reuses. */
const LOCATION_KEY = `${PUBLISHER_KEY}\\${configuration.productName}`;

/** Runs an NSIS installer or uninstaller, which takes a folder last and unquoted, spaces and all. */
function nsis(program, ...options) {
  execFileSync(program, options, {
    stdio: "inherit",
    windowsVerbatimArguments: true,
    argv0: `"${program}"`,
  });
}

/** A value under `key` as `reg query` gives it, the key's own without `name`; `undefined` without
 * one. */
function registryValue(key, name) {
  try {
    const answer = execFileSync("reg", ["query", key, ...(name ? ["/v", name] : ["/ve"])], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
    return /REG_\w+\s+(.*)$/m
      .exec(answer)?.[1]
      ?.trim()
      .replace(/^"(.*)"$/, "$1");
  } catch {
    return undefined;
  }
}

function sameFolder(a, b) {
  return resolve(a).toLowerCase() === resolve(b).toLowerCase();
}

/**
 * Installs the app from `installer`, into `folder` on Windows, and says where the app is and how to
 * remove it again.
 */
export function install(installer, folder) {
  if (process.platform === "win32") {
    const installed = registryValue(UNINSTALL_KEY, "InstallLocation");
    if (installed && !sameFolder(installed, folder)) {
      throw new Error(
        `The desktop app is installed in ${installed}, and a test copy would replace its registration: uninstall it first, or test on another machine.`,
      );
    }
    nsis(installer, "/S", `/D=${folder}`);
    return {
      app: join(folder, "claude-dashboard-desktop.exe"),
      uninstall() {
        // `_?=` runs the uninstaller in place until it has finished, rather than from a copy of
        // itself that carries on after this returns.
        nsis(join(folder, "uninstall.exe"), "/S", `_?=${folder}`);
        // Tauri's uninstaller keeps the recorded folder unless it deletes the app's data, which a
        // silent uninstall never does, and a later install would go there.
        const recorded = registryValue(LOCATION_KEY);
        if (recorded && sameFolder(recorded, folder)) {
          execFileSync("reg", ["delete", LOCATION_KEY, "/f"], { stdio: "ignore" });
          const rest = execFileSync("reg", ["query", PUBLISHER_KEY], { encoding: "utf8" });
          if (rest.split(/\r?\n/).filter((line) => line.trim()).length <= 1) {
            execFileSync("reg", ["delete", PUBLISHER_KEY, "/f"], { stdio: "ignore" });
          }
        }
      },
    };
  }
  const name = execFileSync("dpkg-deb", ["--field", installer, "Package"], {
    encoding: "utf8",
  }).trim();
  let status = "";
  try {
    status = execFileSync("dpkg-query", ["--show", "--showformat=${Status}", name], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
  } catch {
    // dpkg has never heard of the package.
  }
  if (status.includes("installed") && !status.includes("not-installed")) {
    throw new Error(
      `The ${name} package is installed, and testing would replace it: remove it first, or test on another machine.`,
    );
  }
  execFileSync("sudo", ["dpkg", "-i", installer], { stdio: "inherit" });
  return {
    app: "/usr/bin/claude-dashboard-desktop",
    uninstall() {
      execFileSync("sudo", ["dpkg", "-r", name], { stdio: "inherit" });
    },
  };
}
