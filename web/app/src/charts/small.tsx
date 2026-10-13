// Small marks that sit beside a number: sparklines, plan window meters and share bars.

import type { Level } from "@claude-dashboard/protocol";

/** A line of recent values ending in a dot, `width` by `height` pixels, which tells a screen reader
 * its lowest, highest and last value. */
export function Sparkline({
  values,
  label,
  format,
  width = 96,
  height = 32,
  stroke = "var(--spark)",
}: {
  values: readonly number[];
  label: string;
  /** How a value reads, such as `$5.10`. */
  format: (value: number) => string;
  width?: number;
  height?: number;
  /** The line's colour. */
  stroke?: string;
}) {
  const max = Math.max(...values, 0);
  const span = Math.max(1, values.length - 1);
  const points = values.map((value, index) => ({
    x: (index / span) * width,
    y: height - 2 - (max > 0 ? (value / max) * (height - 4) : 0),
  }));
  const last = points.at(-1);
  const spoken =
    values.length === 0
      ? label
      : `${label}: lowest ${format(Math.min(...values))}, highest ${format(max)}, last ${format(values.at(-1) ?? 0)}`;
  return (
    <svg
      role="img"
      aria-label={spoken}
      className="sparkline"
      width={width}
      height={height}
      viewBox={`0 0 ${String(width)} ${String(height)}`}
    >
      <polyline
        points={points.map((point) => `${point.x.toFixed(1)},${point.y.toFixed(1)}`).join(" ")}
        fill="none"
        stroke={stroke}
        strokeWidth="2"
        strokeLinejoin="round"
        strokeLinecap="round"
      />
      {last && (
        <circle
          cx={last.x}
          cy={last.y}
          r="4"
          fill="var(--meter)"
          stroke="var(--panel)"
          strokeWidth="2"
        />
      )}
    </svg>
  );
}

const LEVEL_FILL: Record<Level, string> = {
  normal: "var(--meter)",
  warning: "var(--status-warning)",
  critical: "var(--status-critical)",
};

/** How much of a plan window is used. */
export function Meter({
  used,
  level,
  label,
  stale = false,
}: {
  used: number;
  level: Level;
  label: string;
  /** Draws an old reading hatched and muted. */
  stale?: boolean;
}) {
  const value = Math.min(100, Math.max(0, used));
  return (
    <div
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(value)}
      className="meter"
    >
      <div
        className="meter-fill"
        style={{
          width: `${String(value)}%`,
          background: stale ? "var(--stale-fill)" : LEVEL_FILL[level],
        }}
      />
    </div>
  );
}

/** A thin bar for a share of the largest value beside it. */
export function ShareBar({ share, fill = "var(--bar)" }: { share: number; fill?: string }) {
  return (
    <div className="share" aria-hidden="true">
      <div
        className="share-fill"
        style={{ width: `${String(Math.min(100, Math.max(0, share * 100)))}%`, background: fill }}
      />
    </div>
  );
}
