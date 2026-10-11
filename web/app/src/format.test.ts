import { describe, expect, it } from "vitest";

import examples from "../../../design/formats.json";
import { ago, compact, duration, grouped, longDay, money, shortDate, when } from "./format";

describe("the formats every surface shares", () => {
  it("groups counts", () => {
    for (const [count, text] of examples.grouped) expect(grouped(Number(count))).toBe(text);
  });

  it("writes token counts compactly", () => {
    for (const [count, text] of examples.compact) expect(compact(Number(count))).toBe(text);
  });

  it("writes money to the cent", () => {
    for (const [usd, text] of examples.money) expect(money(Number(usd))).toBe(text);
  });

  it("writes ages", () => {
    const { nowMs } = examples.ago;
    for (const [seconds, text] of examples.ago.examples) {
      expect(ago(nowMs - Number(seconds) * 1000, nowMs)).toBe(text);
    }
  });

  it("writes days", () => {
    for (const example of examples.days.examples) {
      expect(longDay(example.day)).toBe(example.long);
      expect(shortDate(example.day)).toBe(example.short);
    }
  });

  it("writes times from now's point of view, in the server's time zone", () => {
    const { nowMs, timeZone } = examples.when;
    for (const [atMs, text] of examples.when.examples) {
      expect(when(Number(atMs), nowMs, timeZone)).toBe(text);
    }
  });
});

describe("durations", () => {
  it("read as the design writes them", () => {
    expect(duration(45 * 60_000)).toBe("45 min");
    expect(duration((2 * 60 + 5) * 60_000)).toBe("2 h 05 min");
    expect(duration(4 * 24 * 60 * 60_000)).toBe("4 days");
  });
});
