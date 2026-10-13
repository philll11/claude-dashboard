// The labels along a chart's axis: as many as fit without touching, at a steady rhythm, with the
// first and the last kept, as Recharts' `preserveStartEnd` shows them, so that a narrow chart keeps
// readable labels rather than overlapping ones.

import { type RefObject, useLayoutEffect, useState } from "react";

/** The least space between two labels, in CSS pixels. */
const GAP = 8;

/** Where a label's text starts and ends along the axis, in CSS pixels. */
export interface Extent {
  start: number;
  end: number;
}

interface Label {
  index: number;
  extent: Extent;
}

/**
 * Which labels show so that no two come closer than `gap`: every one, or failing that every second,
 * every third and so on, counted from the first; and the last, in place of those before it that it
 * would come too close to. A label without text (`null`) never shows.
 */
export function fitting(extents: readonly (Extent | null)[], gap = GAP): boolean[] {
  const labels = extents.flatMap((extent, index) => (extent ? [{ index, extent }] : []));
  const clear = (before: Label | undefined, after: Label) =>
    !before || after.extent.start >= before.extent.end + gap;
  const last = labels.at(-1);
  const body = labels.slice(0, -1);
  let kept = body;
  for (let stride = 1; stride <= body.length; stride += 1) {
    const every = body.filter((_, position) => position % stride === 0);
    kept = every;
    if (every.every((label, position) => clear(every[position - 1], label))) break;
  }
  if (last) {
    while (kept.length > 1 && !clear(kept.at(-1), last)) kept.pop();
    if (clear(kept.at(-1), last)) kept.push(last);
  }
  const shown = new Set(kept.map((label) => label.index));
  return extents.map((_, index) => shown.has(index));
}

/**
 * Which of the labels in `axis`, one child element each, show, as `fitting` decides from where their
 * text falls. Every label shows until the axis is measured, and the axis is measured again whenever
 * its size or its labels change. A label that does not show keeps its place, hidden, so that it can
 * show again once the axis is wider.
 */
export function useFitting(
  axis: RefObject<HTMLElement | null>,
  labels: readonly string[],
): readonly boolean[] | undefined {
  const [shown, setShown] = useState<readonly boolean[]>();
  const texts = labels.join("\n");
  useLayoutEffect(() => {
    const element = axis.current;
    if (!element) return;
    const measure = () => {
      const range = document.createRange();
      const extents = [...element.children].map((child) => {
        if (!child.textContent) return null;
        range.selectNodeContents(child);
        const box = range.getBoundingClientRect();
        return box.width > 0 ? { start: box.left, end: box.right } : null;
      });
      const next = fitting(extents);
      setShown((current) =>
        current?.length === next.length && current.every((value, index) => value === next[index])
          ? current
          : next,
      );
    };
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => {
      observer.disconnect();
    };
  }, [axis, texts]);
  return shown;
}
