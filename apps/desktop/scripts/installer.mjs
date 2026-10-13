// Installs the desktop app from its installer, as a person would, where a test or check can find
// it, and removes it again. On Windows it installs into a folder the caller names; on Linux the .deb
// package installs with dpkg, which needs sudo.

import { execFileSync } from "node:child_process";
import { join } from "node:path";

/** Runs an NSIS installer or uninstaller, which takes a folder last and unquoted, spaces and all. */
function nsis(program, ...options) {
  execFileSync(program, options, {
    stdio: "inherit",
    windowsVerbatimArguments: true,
    argv0: `"${program}"`,
  });
}

/**
 * Installs the app from `installer`, into `folder` on Windows, and says where the app is and how to
 * remove it again.
 */
export function install(installer, folder) {
  if (process.platform === "win32") {
    nsis(installer, "/S", `/D=${folder}`);
    return {
      app: join(folder, "claude-dashboard-desktop.exe"),
      uninstall() {
        // `_?=` runs the uninstaller in place until it has finished, rather than from a copy of
        // itself that carries on after this returns.
        nsis(join(folder, "uninstall.exe"), "/S", `_?=${folder}`);
      },
    };
  }
  const name = execFileSync("dpkg-deb", ["--field", installer, "Package"], { encoding: "utf8" });
  execFileSync("sudo", ["dpkg", "-i", installer], { stdio: "inherit" });
  return {
    app: "/usr/bin/claude-dashboard-desktop",
    uninstall() {
      execFileSync("sudo", ["dpkg", "-r", name.trim()], { stdio: "inherit" });
    },
  };
}
