import { describe, expect, it } from "vitest";

import examples from "../../../../design/buckets.json";
import { bucketOf, bucketStart } from "./series";

describe("the buckets every surface shares", () => {
  it("chooses days, weeks or months by how many days a chart shows", () => {
    for (const [count, bucket] of examples.bucketOf) expect(bucketOf(Number(count))).toBe(bucket);
  });

  it("starts each week on the day the settings name", () => {
    for (const [day, weekStart, start] of examples.weekStarts) {
      expect(bucketStart(String(day), "week", String(weekStart))).toBe(start);
    }
  });

  it("starts each month on its first day", () => {
    for (const [day, start] of examples.monthStarts) {
      expect(bucketStart(String(day), "month", "monday")).toBe(start);
    }
  });
});
