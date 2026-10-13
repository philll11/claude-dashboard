// Stacked columns: one per day (or week, or month), one segment per model, with a tooltip for the
// column under the pointer or the keyboard focus.

import { type KeyboardEvent, type PointerEvent, useRef, useState } from "react";

import { useFitting } from "./axis";
import { moneyTick, niceScale } from "./scale";

export interface SeriesInfo {
  id: string;
  name: string;
  /** A CSS background: the series colour, or its high contrast pattern. */
  fill: string;
}

export interface Column {
  key: string;
  /** The label under the column, where there is room for one. */
  label: string;
  /** What the tooltip calls the column, such as `Sun 6 Oct`. */
  title: string;
  /** Each series' value, by series id. */
  values: Map<string, number>;
}

const HEIGHT = 176;
const TOOLTIP = 184;

export function StackedColumns({
  columns,
  series,
  label,
  format,
}: {
  columns: readonly Column[];
  /** Bottom to top. */
  series: readonly SeriesInfo[];
  /** What the chart shows, for screen readers. */
  label: string;
  format: (value: number) => string;
}) {
  const [active, setActive] = useState<number | null>(null);
  const plot = useRef<HTMLDivElement>(null);
  const totals = columns.map((column) =>
    series.reduce((sum, item) => sum + (column.values.get(item.id) ?? 0), 0),
  );
  const scale = niceScale(Math.max(0, ...totals));
  const count = Math.max(1, columns.length);
  const labelEvery = count > 20 ? 5 : count > 10 ? 2 : 1;
  const labels = columns.map((column, index) =>
    index % labelEvery === 0 || index === count - 1 ? column.label : "",
  );
  const axis = useRef<HTMLDivElement>(null);
  const fits = useFitting(axis, labels);

  const fromPointer = (event: PointerEvent) => {
    const box = plot.current?.getBoundingClientRect();
    if (!box || box.width === 0) return;
    const index = Math.floor(((event.clientX - box.left) / box.width) * count);
    setActive(Math.min(count - 1, Math.max(0, index)));
  };

  const onKey = (event: KeyboardEvent) => {
    const moves: Record<string, number> = {
      ArrowLeft: (active ?? count) - 1,
      ArrowRight: (active ?? -1) + 1,
      Home: 0,
      End: count - 1,
    };
    const next = moves[event.key];
    if (next === undefined) {
      if (event.key === "Escape") setActive(null);
      return;
    }
    event.preventDefault();
    setActive(Math.min(count - 1, Math.max(0, next)));
  };

  const shown = active === null ? undefined : columns[active];
  const left = active === null ? 0 : (active / count) * 100;

  return (
    <div className="columns">
      <div className="axis-labels" aria-hidden="true">
        {scale.ticks.map((tick) => (
          <span key={tick} style={{ top: HEIGHT - (tick / scale.top) * HEIGHT - 7 }}>
            {moneyTick(tick, scale.ticks)}
          </span>
        ))}
      </div>
      <div
        ref={plot}
        className="plot"
        tabIndex={0}
        role="group"
        aria-roledescription="chart"
        aria-label={`${label} The left and right arrow keys read each column.`}
        onPointerMove={fromPointer}
        onPointerLeave={() => {
          setActive(null);
        }}
        onFocus={() => {
          setActive((current) => current ?? count - 1);
        }}
        onBlur={() => {
          setActive(null);
        }}
        onKeyDown={onKey}
      >
        <div className="gridlines" aria-hidden="true">
          {scale.ticks.map((tick, index) => (
            <div key={tick} className={index === 0 ? "baseline" : undefined} />
          ))}
        </div>
        {shown && (
          <div
            className="band"
            aria-hidden="true"
            style={{ left: `${String(left)}%`, width: `${String(100 / count)}%` }}
          />
        )}
        <div
          className="stacks"
          style={{ gridTemplateColumns: `repeat(${String(count)}, minmax(0, 1fr))` }}
        >
          {columns.map((column) => {
            const parts = series
              .map((item) => ({ item, value: column.values.get(item.id) ?? 0 }))
              .filter((part) => part.value > 0);
            return (
              <div key={column.key} className="stack" aria-hidden="true">
                {parts.reverse().map(({ item, value }, index) => (
                  <div
                    key={item.id}
                    className={index === 0 ? "segment top" : "segment"}
                    style={{
                      height: Math.max(1, Math.round((value / scale.top) * HEIGHT)),
                      background: item.fill,
                    }}
                  />
                ))}
              </div>
            );
          })}
        </div>
        <div
          ref={axis}
          className="column-labels"
          aria-hidden="true"
          style={{ gridTemplateColumns: `repeat(${String(count)}, minmax(0, 1fr))` }}
        >
          {columns.map((column, index) => (
            <span key={column.key} className={fits?.[index] === false ? "hidden-label" : undefined}>
              {labels[index]}
            </span>
          ))}
        </div>
        <div className="visually-hidden" aria-live="polite">
          {shown && active !== null
            ? `${shown.title}: ${series
                .filter((item) => (shown.values.get(item.id) ?? 0) > 0)
                .map((item) => `${item.name} ${format(shown.values.get(item.id) ?? 0)}`)
                .join(", ")}; total ${format(totals[active] ?? 0)}.`
            : ""}
        </div>
        {shown && active !== null && (
          <div
            className="tooltip"
            aria-hidden="true"
            style={
              active >= count / 2
                ? { left: `calc(${String(left)}% - ${String(TOOLTIP + 8)}px)` }
                : { left: `calc(${String(left + 100 / count)}% + 8px)` }
            }
          >
            <span className="tooltip-title">{shown.title}</span>
            {series
              .filter((item) => (shown.values.get(item.id) ?? 0) > 0)
              .map((item) => (
                <div key={item.id} className="tooltip-row">
                  <span className="swatch-name">
                    <span className="swatch small" style={{ background: item.fill }} />
                    {item.name}
                  </span>
                  <span className="figure">{format(shown.values.get(item.id) ?? 0)}</span>
                </div>
              ))}
            <div className="tooltip-total">
              <span>Total</span>
              <span className="figure">{format(totals[active] ?? 0)}</span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
