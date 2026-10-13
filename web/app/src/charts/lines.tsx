// Lines over time, drawn in SVG stretched to the panel's width, with dots, labels and the tooltip
// laid over them in HTML so that they keep their shape.

import { type KeyboardEvent, type PointerEvent, type ReactNode, useRef, useState } from "react";

import { useFitting } from "./axis";

export interface Line {
  id: string;
  /** A CSS colour. */
  colour: string;
  /** In data units, in order of `x`. */
  points: readonly { x: number; y: number }[];
  /** Shades the area under the line. */
  area?: boolean;
  /** Draws a projection rather than a reading. */
  projected?: boolean;
}

export interface Dot {
  x: number;
  y: number;
  colour: string;
  /** Text above the dot. */
  label?: string;
}

export interface Guide {
  y: number;
  colour: string;
  /** Text at the right end of the guide. */
  label: string;
}

const WIDTH = 1000;

/** Where along the axis a value falls, as a share of the axis. */
function share(value: number, from: number, to: number): number {
  return to === from ? 0 : (value - from) / (to - from);
}

export function LineChart({
  lines,
  x,
  yTop,
  height,
  dots = [],
  guides = [],
  ticks,
  xLabels,
  label,
  probe,
}: {
  lines: readonly Line[];
  /** The first and last value along the x axis. */
  x: readonly [number, number];
  yTop: number;
  height: number;
  dots?: readonly Dot[];
  guides?: readonly Guide[];
  /** Gridlines, with their labels on the left when they have text. */
  ticks: readonly { y: number; text?: string }[];
  /** Labels under the axis, at their places along it. */
  xLabels: readonly { x: number; text: string }[];
  /** What the chart shows, for screen readers. */
  label: string;
  /** What the tooltip says at a point along the x axis; leaves out the tooltip without it. */
  probe?: (
    at: number,
  ) => { x: number; title: string; rows: ReactNode; summary: string } | undefined;
}) {
  const [active, setActive] = useState<number | null>(null);
  const plot = useRef<HTMLDivElement>(null);
  const axis = useRef<HTMLDivElement>(null);
  const fits = useFitting(
    axis,
    xLabels.map((tick) => tick.text),
  );
  const [x0, x1] = x;
  const toX = (value: number) => share(value, x0, x1) * WIDTH;
  const toY = (value: number) => height - share(value, 0, yTop) * height;
  const shown = active === null || !probe ? undefined : probe(active);
  const labelled = ticks.some((tick) => tick.text !== undefined);

  const fromPointer = (event: PointerEvent) => {
    const box = plot.current?.getBoundingClientRect();
    if (!box || box.width === 0 || !probe) return;
    setActive(x0 + Math.min(1, Math.max(0, (event.clientX - box.left) / box.width)) * (x1 - x0));
  };

  const onKey = (event: KeyboardEvent) => {
    if (!probe) return;
    const step = (x1 - x0) / 20;
    const moves: Record<string, number> = {
      ArrowLeft: (active ?? x1) - step,
      ArrowRight: (active ?? x0 - step) + step,
      Home: x0,
      End: x1,
    };
    const next = moves[event.key];
    if (next === undefined) {
      if (event.key === "Escape") setActive(null);
      return;
    }
    event.preventDefault();
    setActive(Math.min(x1, Math.max(x0, next)));
  };

  const at = shown ? share(shown.x, x0, x1) * 100 : 0;

  return (
    <div className="lines">
      {labelled && (
        <div className="axis-labels" aria-hidden="true" style={{ height: height + 4 }}>
          {ticks.map((tick) =>
            tick.text === undefined ? null : (
              <span key={tick.y} style={{ top: toY(tick.y) - 7 }}>
                {tick.text}
              </span>
            ),
          )}
        </div>
      )}
      <div
        ref={plot}
        className="plot"
        tabIndex={probe ? 0 : undefined}
        role={probe ? "group" : "img"}
        aria-roledescription={probe ? "chart" : undefined}
        aria-label={probe ? `${label} The left and right arrow keys read along it.` : label}
        onPointerMove={fromPointer}
        onPointerLeave={() => {
          setActive(null);
        }}
        onFocus={() => {
          if (probe) setActive((current) => current ?? x1);
        }}
        onBlur={() => {
          setActive(null);
        }}
        onKeyDown={onKey}
      >
        <svg
          aria-hidden="true"
          viewBox={`0 0 ${String(WIDTH)} ${String(height)}`}
          preserveAspectRatio="none"
          width="100%"
          height={height}
          className="line-svg"
        >
          {ticks.map((tick, index) => (
            <line
              key={tick.y}
              x1="0"
              x2={WIDTH}
              y1={toY(tick.y)}
              y2={toY(tick.y)}
              className={index === 0 ? "axis-line" : "grid-line"}
            />
          ))}
          {guides.map((guide) => (
            <line
              key={guide.label}
              x1="0"
              x2={WIDTH}
              y1={toY(guide.y)}
              y2={toY(guide.y)}
              stroke={guide.colour}
              className="guide-line"
            />
          ))}
          {lines.map((line) => {
            const points = line.points.map(
              (point) => `${toX(point.x).toFixed(1)},${toY(point.y).toFixed(1)}`,
            );
            const first = line.points[0];
            const last = line.points.at(-1);
            return (
              <g key={line.id}>
                {line.area && first && last && (
                  <polygon
                    points={`${toX(first.x).toFixed(1)},${String(height)} ${points.join(" ")} ${toX(last.x).toFixed(1)},${String(height)}`}
                    fill={line.colour}
                    className="line-area"
                  />
                )}
                <polyline
                  points={points.join(" ")}
                  fill="none"
                  stroke={line.colour}
                  className={line.projected ? "line projected" : "line"}
                />
              </g>
            );
          })}
          {shown && (
            <line x1={toX(shown.x)} x2={toX(shown.x)} y1="0" y2={height} className="crosshair" />
          )}
        </svg>
        {guides.map((guide) => (
          <span
            key={guide.label}
            className="guide-label"
            aria-hidden="true"
            style={{ top: toY(guide.y) - 18 }}
          >
            {guide.label}
          </span>
        ))}
        {dots.map((dot, index) => (
          <div key={`${String(dot.x)}-${String(index)}`} aria-hidden="true">
            <span
              className="point"
              style={{
                left: `${String(share(dot.x, x0, x1) * 100)}%`,
                top: toY(dot.y),
                background: dot.colour,
              }}
            />
            {dot.label && (
              <span
                className="point-label"
                style={{ left: `${String(share(dot.x, x0, x1) * 100)}%`, top: toY(dot.y) - 26 }}
              >
                {dot.label}
              </span>
            )}
          </div>
        ))}
        <div ref={axis} className="axis-times" aria-hidden="true">
          {xLabels.map((tick, index) => {
            const place = share(tick.x, x0, x1) * 100;
            const edge = index === 0 && place < 1 ? "start" : place > 99 ? "end" : "middle";
            const hidden = fits?.[index] === false;
            return (
              <span
                key={`${tick.text}-${String(index)}`}
                className={hidden ? `${edge} hidden-label` : edge}
                style={edge === "end" ? { right: 0 } : { left: `${String(place)}%` }}
              >
                {tick.text}
              </span>
            );
          })}
        </div>
        {probe && (
          <div className="visually-hidden" aria-live="polite">
            {shown?.summary ?? ""}
          </div>
        )}
        {shown && (
          <div
            className="tooltip"
            aria-hidden="true"
            style={
              at > 50
                ? { left: `calc(${String(at)}% - 192px)` }
                : { left: `calc(${String(at)}% + 8px)` }
            }
          >
            <span className="tooltip-title">{shown.title}</span>
            {shown.rows}
          </div>
        )}
      </div>
    </div>
  );
}
