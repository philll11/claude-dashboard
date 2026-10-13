// The messages between the dashboard in a VS Code webview and the extension that hosts it. The
// extension passes requests on to `claude-dashboard serve`, and its answers and notifications
// back.

import type { ExportParams, Exported } from "./generated/protocol";
import type { Method, Notifications } from "./index";

/** A section of the dashboard. */
export type Section = "overview" | "sessions" | "projects" | "models" | "limits" | "settings";

/** The days a section covers. */
export type Range = "today" | "7-days" | "30-days" | "this-month" | "all-time";

/** A section to show, with the days and filters to show it with; what it leaves out stays. */
export interface Show {
  section: Section;
  range?: Range;
  /** A project's id, or `null` for every project. */
  project?: string | null;
  /** A model's id, or `null` for every model. */
  model?: string | null;
}

/** What the webview sends the extension. The extension relays only the requests the dashboard
 * makes; an export goes as `export`, so that the file it writes is always one the person chose
 * in the extension's save dialog. */
export type ToExtension =
  | { type: "ready" }
  | { type: "request"; id: number; method: Method; params: unknown }
  | { type: "reveal"; path: string }
  | { type: "pickFolder"; id: number }
  | { type: "export"; id: number; params: ExportParams }
  | { type: "cancelExport" };

/** What the extension sends the webview. */
export type FromExtension =
  | { type: "response"; id: number; result?: unknown; error?: { code: number; message: string } }
  | { type: "notification"; method: keyof Notifications; params: unknown }
  | { type: "picked"; id: number; path?: string }
  | { type: "show"; show: Show }
  /** How an export ended: `result` when it was saved, neither when the person chose no file. */
  | {
      type: "exported";
      id: number;
      result?: Exported;
      error?: { code: number; message: string };
    };
