// The design's containers: panels with a heading, notices that need the person's attention, and
// the small pills that label a state.

import { type ReactNode, useId } from "react";

import { ErrorCircle } from "./icons";

/** A titled panel. `subtitle` says what its numbers cover; `actions` sit on the right. */
export function Panel({
  title,
  aside,
  subtitle,
  actions,
  className,
  children,
}: {
  title: ReactNode;
  /** Muted text after the title, such as `· this month`. */
  aside?: ReactNode;
  subtitle?: ReactNode;
  actions?: ReactNode;
  className?: string;
  children?: ReactNode;
}) {
  const id = useId();
  return (
    <section aria-labelledby={id} className={className ? `panel ${className}` : "panel"}>
      <div className="panel-head">
        <div className="panel-titles">
          <h2 id={id}>
            {title}
            {aside && <span className="aside"> {aside}</span>}
          </h2>
          {subtitle && <span className="subtitle">{subtitle}</span>}
        </div>
        {actions}
      </div>
      {children}
    </section>
  );
}

export type Tone = "accent" | "caution" | "critical";

/** A small rounded label, such as `Running` or `Experimental`. */
export function Pill({ tone, children }: { tone?: Tone; children: ReactNode }) {
  return <span className={tone ? `pill ${tone}` : "pill"}>{children}</span>;
}

/** Something the person may want to act on, with an icon, a title, text and actions. */
export function Notice({
  tone,
  icon,
  title,
  children,
  actions,
  role = "region",
}: {
  tone: Tone | "plain";
  icon: ReactNode;
  title: ReactNode;
  children?: ReactNode;
  actions?: ReactNode;
  role?: "region" | "alert" | "status";
}) {
  const id = useId();
  return (
    <section
      role={role}
      aria-labelledby={role === "region" ? id : undefined}
      className={`notice ${tone}`}
    >
      <div className="notice-body">
        <span className="notice-icon">{icon}</span>
        <div className="notice-text">
          <span id={id} className="notice-title">
            {title}
          </span>
          {children && <span className="notice-detail">{children}</span>}
        </div>
      </div>
      {actions && <div className="notice-actions">{actions}</div>}
    </section>
  );
}

/** Something that went wrong, with what the person can do about it. */
export function Problem({
  title,
  detail,
  actions,
}: {
  title: string;
  detail?: string;
  actions?: ReactNode;
}) {
  return (
    <Notice
      tone="critical"
      role="alert"
      icon={<ErrorCircle size={18} />}
      title={title}
      actions={actions}
    >
      {detail}
    </Notice>
  );
}

/** A labelled figure in a tile: `Today`, `$5.05`, and a line under it. */
export function Tile({
  label,
  value,
  note,
  children,
}: {
  label: string;
  value: ReactNode;
  note?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <section className="panel tile" aria-label={label}>
      <h2 className="tile-label">{label}</h2>
      <div className="tile-body">
        <div className="tile-figures">
          <span className="tile-value">{value}</span>
          {note && <span className="tile-note">{note}</span>}
        </div>
        {children}
      </div>
    </section>
  );
}
