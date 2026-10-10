// Which claude-dashboard program the extension runs: the one `claudeDashboard.server.path` names,
// or else the one that comes with the extension for this platform.

import { constants, promises as fs } from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

import * as vscode from "vscode";

export type Found =
  | {
      kind: "found";
      path: string;
      /** Whether it is the program that comes with the extension, which an update moves. */
      bundled: boolean;
    }
  | { kind: "missing"; reason: string };

const FILE_NAME = process.platform === "win32" ? "claude-dashboard.exe" : "claude-dashboard";

/** The program to run, or why there is none. */
export async function findProgram(extensionPath: string): Promise<Found> {
  const configured = vscode.workspace
    .getConfiguration("claudeDashboard")
    .get<string>("server.path", "")
    .trim();
  if (configured) {
    const named = /^~(?=$|[\\/])/.test(configured)
      ? path.join(os.homedir(), configured.slice(1))
      : configured;
    return (await runnable(named))
      ? { kind: "found", path: named, bundled: false }
      : {
          kind: "missing",
          reason: `claudeDashboard.server.path names ${configured}, which is not a program that can run.`,
        };
  }
  const bundled = path.join(extensionPath, "server", FILE_NAME);
  return (await runnable(bundled))
    ? { kind: "found", path: bundled, bundled: true }
    : {
        kind: "missing",
        reason: `The extension has no claude-dashboard program for ${process.platform}-${process.arch}.`,
      };
}

/** Whether `file` is there and can run. A program that lost its permission to run on the way
 * out of the package gets it back. */
async function runnable(file: string): Promise<boolean> {
  try {
    await fs.access(file, constants.F_OK);
  } catch {
    return false;
  }
  if (process.platform === "win32") return true;
  try {
    await fs.access(file, constants.X_OK);
    return true;
  } catch {
    try {
      await fs.chmod(file, 0o755);
      return true;
    } catch {
      return false;
    }
  }
}
