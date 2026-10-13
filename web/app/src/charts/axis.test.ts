import { describe, expect, it } from "vitest";

import { type Extent, fitting } from "./axis";

/** Labels `width` wide whose starts are `apart` from each other. */
function labels(count: number, apart: number, width: number): Extent[] {
  return Array.from({ length: count }, (_, index) => ({
    start: index * apart,
    end: index * apart + width,
  }));
}

describe("the labels along an axis", () => {
  it("all show where they fit", () => {
    expect(fitting(labels(4, 50, 30))).toEqual([true, true, true, true]);
  });

  it("show every other one where every one would touch", () => {
    expect(fitting(labels(7, 30, 30))).toEqual([true, false, true, false, true, false, true]);
  });

  it("keep their rhythm, with the last in place of the one before it that it would touch", () => {
    // Six times along a chart 210 pixels wide, the first and the last against its ends.
    const extents = [
      { start: 0, end: 30 },
      ...[1, 2, 3, 4].map((step) => ({ start: step * 42 - 15, end: step * 42 + 15 })),
      { start: 180, end: 210 },
    ];
    expect(fitting(extents)).toEqual([true, false, true, false, false, true]);
  });

  it("keep the last in place of only those it would touch", () => {
    const extents = [...labels(3, 40, 30), { start: 100, end: 130 }];
    expect(fitting(extents)).toEqual([true, true, false, true]);
  });

  it("keep the first alone where the last cannot clear it", () => {
    expect(fitting(labels(3, 10, 30))).toEqual([true, false, false]);
  });

  it("never show a label without text", () => {
    expect(fitting([{ start: 0, end: 30 }, null, { start: 80, end: 110 }, null])).toEqual([
      true,
      false,
      true,
      false,
    ]);
  });

  it("show a label that is alone", () => {
    expect(fitting([null, { start: 10, end: 40 }])).toEqual([false, true]);
  });
});
