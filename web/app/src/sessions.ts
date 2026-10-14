// Sessions wherever the dashboard shows them: whether one is running, and, for the strip in VS
// Code's view, the latest session in the project VS Code has open.

import type { OpenFolders, SessionRow } from "@claude-dashboard/protocol";

/** How recently a session that Claude Code's own figure does not cover made a request, to count
 * as running. */
export const RUNNING_MS = 15 * 60_000;

/** Whether a session is running: it made a request in the last 15 minutes, and Claude Code, which
 * writes its figure as each of the session's processes ends, has written none that covers it. */
export function isRunning(
  session: { lastMs: number; drift?: number | null },
  nowMs: number,
): boolean {
  return (
    (session.drift === null || session.drift === undefined) && nowMs - session.lastMs < RUNNING_MS
  );
}

/** A path as compared: with forward slashes, without one at the end, and in lower case where file
 * names ignore case. */
function comparable(path: string, ignoreCase: boolean): string {
  const slashed = path.replaceAll("\\", "/").replace(/(.)\/+$/, "$1");
  return ignoreCase ? slashed.toLowerCase() : slashed;
}

/** Whether `inner` is `outer` or a folder inside it. */
function within(inner: string, outer: string): boolean {
  return inner === outer || inner.startsWith(outer.endsWith("/") ? outer : `${outer}/`);
}

/** How a project's folder stands to the folders VS Code has open: `inside` when it is one of them
 * or inside one, `around` when it holds one, such as a repository whose part VS Code has open. */
export function relation(
  path: string | null | undefined,
  open: OpenFolders,
): "inside" | "around" | undefined {
  if (!path) return undefined;
  const project = comparable(path, open.ignoreCase);
  const folders = open.folders.map((folder) => comparable(folder, open.ignoreCase));
  if (folders.some((folder) => within(project, folder))) return "inside";
  if (folders.some((folder) => within(folder, project))) return "around";
  return undefined;
}

/** The latest session, by its last request, and whether its project is one VS Code has open. */
export interface Latest {
  row: SessionRow;
  here: boolean;
}

/** The latest session in a project inside a folder VS Code has open; else the latest in the
 * nearest project holding one, so that a session in, say, the home folder does not stand for the
 * repository; else the latest in any project. `undefined` when there is none. */
export function latestSession(rows: readonly SessionRow[], open: OpenFolders): Latest | undefined {
  const latest = (candidates: readonly SessionRow[]) =>
    candidates.reduce<SessionRow | undefined>(
      (best, row) => (best === undefined || row.lastMs > best.lastMs ? row : best),
      undefined,
    );
  const inside = latest(rows.filter((row) => relation(row.project.path, open) === "inside"));
  if (inside) return { row: inside, here: true };
  const around = rows.filter((row) => relation(row.project.path, open) === "around");
  const depth = (row: SessionRow) => comparable(row.project.path ?? "", open.ignoreCase).length;
  const nearest = Math.max(...around.map(depth));
  const enclosing = latest(around.filter((row) => depth(row) === nearest));
  if (enclosing) return { row: enclosing, here: true };
  const anywhere = latest(rows);
  return anywhere ? { row: anywhere, here: false } : undefined;
}
