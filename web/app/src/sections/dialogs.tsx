// The changes the dashboard makes only once the person agrees: turning on the usage endpoint, and
// connecting or disconnecting Claude Code's status line, which changes Claude Code's settings.

import { type ReactNode, useEffect, useId, useRef, useState } from "react";

import type { StatusLinePlan } from "@claude-dashboard/protocol";

import { useStore } from "../data";

/** A modal dialog that closes on Escape and gives focus back where it came from. */
function Dialog({
  title,
  onClose,
  children,
  actions,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  actions: ReactNode;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  useEffect(() => {
    const element = dialog.current;
    if (!element) return;
    const before = document.activeElement;
    element.showModal();
    return () => {
      element.close();
      if (before instanceof HTMLElement) before.focus();
    };
  }, []);
  return (
    <dialog
      ref={dialog}
      aria-labelledby={titleId}
      className="dialog"
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      <h2 id={titleId}>{title}</h2>
      {children}
      <div className="dialog-actions">{actions}</div>
    </dialog>
  );
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Asks before turning on Anthropic's usage endpoint, with what it means. */
export function ConsentDialog({
  consent,
  onClose,
}: {
  consent: readonly string[];
  onClose: () => void;
}) {
  const store = useStore();
  const [agreed, setAgreed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const turnOn = () => {
    setBusy(true);
    setProblem(null);
    store.send("endpoint/set", { enabled: true }).then(
      () => {
        store.refresh((entry) => entry.method === "limits" || entry.method === "status");
        onClose();
      },
      (error: unknown) => {
        setBusy(false);
        setProblem(message(error));
      },
    );
  };
  return (
    <Dialog
      title="Use Anthropic's usage endpoint?"
      onClose={onClose}
      actions={
        <>
          <button type="button" className="button quiet" onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className="button primary"
            disabled={!agreed || busy}
            onClick={turnOn}
          >
            Turn on
          </button>
        </>
      }
    >
      <ul className="consent">
        {consent.map((sentence) => (
          <li key={sentence}>
            {sentence
              .replace("api.anthropic.com", "\u0000")
              .split("\u0000")
              .flatMap((part, index) =>
                index === 0
                  ? [part]
                  : [
                      <span key="host" className="mono">
                        api.anthropic.com
                      </span>,
                      part,
                    ],
              )}
          </li>
        ))}
      </ul>
      <label className="agree">
        <input
          type="checkbox"
          checked={agreed}
          onChange={(event) => {
            setAgreed(event.target.checked);
          }}
        />
        <span>I understand this uses an undocumented endpoint</span>
      </label>
      {problem && <p className="note problem">{problem}</p>}
    </Dialog>
  );
}

/** Shows the exact change to Claude Code's settings before connecting or disconnecting its status
 * line, and makes it only when the person agrees. */
export function StatusLineDialog({ connect, onClose }: { connect: boolean; onClose: () => void }) {
  const store = useStore();
  const [plan, setPlan] = useState<StatusLinePlan | null>(null);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    let current = true;
    store.send("statusLine/plan", { connect }).then(
      (planned) => {
        if (current) setPlan(planned);
      },
      (error: unknown) => {
        if (current) setProblem(message(error));
      },
    );
    return () => {
      current = false;
    };
  }, [store, connect]);
  const change = plan?.kind === "change" ? plan : undefined;
  const apply = () => {
    if (!change) return;
    setBusy(true);
    setProblem(null);
    store
      .send("statusLine/apply", {
        connect,
        before: change.before ?? null,
        after: change.after ?? null,
      })
      .then(
        () => {
          store.refresh((entry) => entry.method === "limits" || entry.method === "status");
          onClose();
        },
        (error: unknown) => {
          setBusy(false);
          setProblem(message(error));
        },
      );
  };
  return (
    <Dialog
      title={
        connect ? "Connect Claude Code's status line?" : "Disconnect Claude Code's status line?"
      }
      onClose={onClose}
      actions={
        change ? (
          <>
            <button type="button" className="button quiet" onClick={onClose}>
              Cancel
            </button>
            <button type="button" className="button primary" disabled={busy} onClick={apply}>
              Make this change
            </button>
          </>
        ) : (
          <button type="button" className="button quiet" onClick={onClose}>
            Close
          </button>
        )
      }
    >
      {change && (
        <>
          <p className="note">
            {change.before && change.after
              ? "This changes one setting in "
              : change.after
                ? change.exists
                  ? "This adds one setting to "
                  : "This creates "
                : "This removes one setting from "}
            <span className="mono">{change.path}</span>
            {change.after && !change.before && !change.exists ? " with one setting:" : ":"}
          </p>
          <pre className="change">
            {change.before && change.after ? (
              <>
                {'  "statusLine": {\n'}
                <span className="removed">{`-   ${change.before}\n`}</span>
                <span className="added">{`+   ${change.after}\n`}</span>
                {"  }"}
              </>
            ) : change.after ? (
              <span className="added">{`+ "statusLine": { ${change.after} }`}</span>
            ) : (
              <span className="removed">{`- "statusLine": { ${change.before ?? ""} }`}</span>
            )}
          </pre>
          <p className="note">{change.effect}</p>
          {change.exists && <p className="note">A backup is saved next to settings.json.</p>}
        </>
      )}
      {plan && plan.kind !== "change" && <p className="note">{plan.reason}</p>}
      {problem && <p className="note problem">{problem}</p>}
    </Dialog>
  );
}
