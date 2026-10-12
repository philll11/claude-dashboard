// Exports: each section's Export button and its menu, the export under way, and the notice that
// says where an export was saved. One export runs at a time, wherever it was started.

import {
  createContext,
  type KeyboardEvent,
  type ReactNode,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";

import type {
  Dataset,
  ExportFormat,
  ExportParams,
  Exported,
  ExportProgress,
} from "@claude-dashboard/protocol";

import { useHost } from "./context";
import { grouped } from "./format";
import { EXPORT_CANCELLED, type Host } from "./host";
import { Check, Close, Download, Warning } from "./ui/icons";

/** What an export holds, without its format, which the menu chooses. */
export type ExportWhat = Omit<ExportParams, "format">;

/** Where the export stands. */
export type ExportState =
  | { kind: "idle" }
  | { kind: "running"; dataset: Dataset; written: number; total: number; path?: string }
  | { kind: "saved"; exported: Exported }
  | { kind: "failed"; message: string };

/** The export under way, or the last one's outcome, for every section to show. */
export class Exports {
  private current: ExportState = { kind: "idle" };
  private readonly listeners = new Set<() => void>();

  constructor(private readonly host: Host) {
    host.backend.onNotification("export/progress", (progress: ExportProgress) => {
      if (this.current.kind !== "running") return;
      this.become({
        ...this.current,
        written: progress.written,
        total: progress.total,
        ...(progress.path ? { path: progress.path } : {}),
      });
    });
  }

  readonly state = (): ExportState => this.current;

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  /** Saves `params` where the person chooses, unless an export is under way. */
  start(params: ExportParams): void {
    const exportTo = this.host.exportTo;
    if (!exportTo || this.current.kind === "running") return;
    this.become({ kind: "running", dataset: params.dataset, written: 0, total: 0 });
    exportTo(params).then(
      (exported) => {
        this.become(exported ? { kind: "saved", exported } : { kind: "idle" });
      },
      (error: unknown) => {
        const cancelled =
          typeof error === "object" &&
          error !== null &&
          "code" in error &&
          error.code === EXPORT_CANCELLED;
        this.become(
          cancelled
            ? { kind: "idle" }
            : { kind: "failed", message: error instanceof Error ? error.message : String(error) },
        );
      },
    );
  }

  cancel(): void {
    if (this.current.kind === "running") this.host.cancelExport?.();
  }

  /** Puts the last export's notice away. */
  dismiss(): void {
    if (this.current.kind === "saved" || this.current.kind === "failed") {
      this.become({ kind: "idle" });
    }
  }

  private become(next: ExportState): void {
    this.current = next;
    for (const listener of this.listeners) listener();
  }
}

export const ExportsContext = createContext<Exports | null>(null);

function useExports(): Exports {
  const exports = useContext(ExportsContext);
  if (!exports) throw new Error("useExports needs an ExportsContext");
  return exports;
}

/** Where the export stands. */
export function useExportState(): ExportState {
  const exports = useExports();
  return useSyncExternalStore(exports.subscribe, exports.state);
}

/** What the export menu offers, in the board's words. */
const FORMATS: readonly { format: ExportFormat; label: string; detail: string }[] = [
  { format: "csv", label: "Export as CSV…", detail: "Opens in Excel, Numbers and Google Sheets" },
  { format: "json", label: "Export as JSON…", detail: "For scripts and other tools" },
];

/** A button that opens the menu of export formats, with what the export holds under them. Without
 * a surface that can save files, there is none. */
export function ExportMenu({
  what,
  scope,
  children,
  className = "button",
  align = "end",
}: {
  what: ExportWhat;
  /** What the export holds: `All 214 sessions that match, not only this page`. */
  scope: string;
  /** The button's text; `Export` with its icon unless given. */
  children?: ReactNode;
  className?: string;
  /** Which of the button's edges the menu lines up with. */
  align?: "start" | "end";
}) {
  const host = useHost();
  const exports = useExports();
  const state = useExportState();
  const [open, setOpen] = useState<"first" | "last" | null>(null);
  const button = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const menuId = useId();

  const items = () => [...(menu.current?.querySelectorAll<HTMLElement>("[role=menuitem]") ?? [])];

  useLayoutEffect(() => {
    if (open === null) return;
    const all = items();
    (open === "first" ? all[0] : all.at(-1))?.focus();
  }, [open]);

  useEffect(() => {
    if (open === null) return;
    const close = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!menu.current?.contains(target) && !button.current?.contains(target)) setOpen(null);
    };
    document.addEventListener("pointerdown", close);
    return () => {
      document.removeEventListener("pointerdown", close);
    };
  }, [open]);

  if (!host.exportTo) return null;
  const busy = state.kind === "running";

  const choose = (format: ExportFormat) => {
    setOpen(null);
    button.current?.focus();
    exports.start({ ...what, format });
  };

  const onButtonKey = (event: KeyboardEvent) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setOpen("first");
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setOpen("last");
    }
  };

  const onMenuKey = (event: KeyboardEvent) => {
    const all = items();
    const at = all.indexOf(document.activeElement as HTMLElement);
    const focus = (index: number) => all[(index + all.length) % all.length]?.focus();
    switch (event.key) {
      case "ArrowDown":
        focus(at + 1);
        break;
      case "ArrowUp":
        focus(at - 1);
        break;
      case "Home":
        focus(0);
        break;
      case "End":
        focus(all.length - 1);
        break;
      case "Escape":
        setOpen(null);
        button.current?.focus();
        break;
      case "Tab":
        setOpen(null);
        return;
      default:
        return;
    }
    event.preventDefault();
  };

  return (
    <div className="picker">
      <button
        ref={button}
        type="button"
        className={className}
        aria-haspopup="menu"
        aria-expanded={open !== null}
        aria-controls={open === null ? undefined : menuId}
        disabled={busy}
        onClick={() => {
          setOpen(open === null ? "first" : null);
        }}
        onKeyDown={onButtonKey}
      >
        {children ?? (
          <>
            <Download size={14} />
            Export
          </>
        )}
      </button>
      {open !== null && (
        <div
          ref={menu}
          id={menuId}
          role="menu"
          aria-label="Export"
          className={align === "start" ? "menu export-menu start" : "menu export-menu"}
          onKeyDown={onMenuKey}
        >
          {FORMATS.map((choice) => (
            <button
              key={choice.format}
              type="button"
              role="menuitem"
              tabIndex={-1}
              onClick={() => {
                choose(choice.format);
              }}
            >
              <span className="export-label">{choice.label}</span>
              <span className="export-detail">{choice.detail}</span>
            </button>
          ))}
          <div role="separator" className="menu-separator" />
          <span className="export-scope">{scope}</span>
        </div>
      )}
    </div>
  );
}

/** The last part of a path: the file's name. */
export function fileName(path: string): string {
  return path.split(/[\\/]/).at(-1) ?? path;
}

/** Cancels the export under way. */
export function CancelExport() {
  const exports = useExports();
  const state = useExportState();
  if (state.kind !== "running") return null;
  return (
    <button
      type="button"
      className="button compact text-button"
      onClick={() => {
        exports.cancel();
      }}
    >
      Cancel
    </button>
  );
}

/** How far an export has got, for one long enough to watch, such as the whole history. */
export function ExportProgressLine({ label }: { label: string }) {
  const state = useExportState();
  if (state.kind !== "running") return null;
  const share = state.total > 0 ? Math.min(1, state.written / state.total) : 0;
  const rows = state.dataset === "requests" ? "requests" : "rows";
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={state.total || undefined}
      aria-valuenow={state.total ? state.written : undefined}
      className="export-progress"
    >
      <div className="progress">
        <div className="progress-fill" style={{ width: `${(share * 100).toFixed(1)}%` }} />
      </div>
      <span className="subtitle">
        {grouped(state.written)}
        {state.total > 0 ? ` of ${grouped(state.total)}` : ""} {rows} written
        {state.path ? ` to ${fileName(state.path)}` : ""}
      </span>
    </div>
  );
}

/** Says where the last export was saved, or why it failed, until the person puts it away. */
export function ExportNotice() {
  const host = useHost();
  const exports = useExports();
  const state = useExportState();
  if (state.kind !== "saved" && state.kind !== "failed") return null;
  const dismiss = (
    <button
      type="button"
      className="toast-dismiss"
      aria-label="Dismiss"
      onClick={() => {
        exports.dismiss();
      }}
    >
      <Close size={14} />
    </button>
  );
  if (state.kind === "failed") {
    return (
      <div role="alert" className="toast">
        <Warning size={18} className="warning-mark toast-mark" />
        <div className="toast-text">
          <span className="toast-title">Cannot export</span>
          <span className="toast-detail">{state.message}</span>
        </div>
        {dismiss}
      </div>
    );
  }
  const { exported } = state;
  const path = exported.path;
  return (
    <div role="status" className="toast">
      <Check size={18} className="good-mark toast-mark" />
      <div className="toast-text">
        <span className="toast-title">Saved {exported.what}</span>
        {path && <span className="toast-path mono">{path}</span>}
        {path && host.reveal && (
          <span className="actions toast-actions">
            <button
              type="button"
              className="button compact quiet"
              onClick={() => {
                host.reveal?.(path);
              }}
            >
              Show in folder
            </button>
          </span>
        )}
      </div>
      {dismiss}
    </div>
  );
}
