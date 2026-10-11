// The folders the app keeps its own files in, apart from the history: its log, its window's place
// and its web view's storage. The tests and checks that run the app remove those they created.

import { existsSync, readFileSync } from "node:fs";
import { rm } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const configuration = join(dirname(fileURLToPath(import.meta.url)), "../src-tauri/tauri.conf.json");
const { identifier } = JSON.parse(readFileSync(configuration, "utf8"));

/** Where Tauri, the log and the web view keep the app's files on this system. */
function appFolders() {
  const home = homedir();
  switch (process.platform) {
    case "win32":
      return [process.env.LOCALAPPDATA, process.env.APPDATA]
        .filter((base) => base !== undefined)
        .map((base) => join(base, identifier));
    case "darwin":
      return [
        ...["Application Support", "Caches", "HTTPStorages", "Logs", "WebKit"].map((folder) =>
          join(home, "Library", folder, identifier),
        ),
        join(home, "Library", "Saved Application State", `${identifier}.savedState`),
      ];
    default:
      return [
        process.env.XDG_DATA_HOME ?? join(home, ".local/share"),
        process.env.XDG_CONFIG_HOME ?? join(home, ".config"),
        process.env.XDG_CACHE_HOME ?? join(home, ".cache"),
      ].map((base) => join(base, identifier));
  }
}

/** Notes which of the app's folders do not exist yet, so that `remove` can remove them again. */
export function newAppFolders() {
  const missing = appFolders().filter((folder) => !existsSync(folder));
  return {
    async remove() {
      for (const folder of missing) {
        // The web view's processes outlive the app by a moment, holding files in the folder, and
        // only the asynchronous rm waits for them on Windows.
        await rm(folder, { recursive: true, force: true, maxRetries: 20, retryDelay: 500 });
      }
    },
  };
}
