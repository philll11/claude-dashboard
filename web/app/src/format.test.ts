import { describe, expect, it } from "vitest";

import examples from "../../../design/formats.json";
import {
  ago,
  compact,
  dayAndTime,
  dayLabel,
  duration,
  grouped,
  longDay,
  money,
  monthYear,
  percent,
  price,
  resets,
  shortDate,
  when,
} from "./format";

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

  it("writes prices to the cent or a tenth of one", () => {
    for (const [usd, text] of examples.price) expect(price(Number(usd))).toBe(text);
  });

  it("writes shares as whole percentages", () => {
    for (const [share, text] of examples.percent) expect(percent(Number(share))).toBe(text);
  });

  it("writes durations", () => {
    for (const [ms, text] of examples.duration) expect(duration(Number(ms))).toBe(text);
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
    for (const [day, text] of examples.dayLabel) expect(dayLabel(String(day))).toBe(text);
    for (const [day, text] of examples.monthYear) expect(monthYear(String(day))).toBe(text);
  });

  it("writes times from now's point of view, in the server's time zone", () => {
    const { nowMs, timeZone } = examples.when;
    for (const [atMs, text] of examples.when.examples) {
      expect(when(Number(atMs), nowMs, timeZone)).toBe(text);
    }
  });

  it("writes when a plan window resets", () => {
    const { nowMs, timeZone } = examples.resets;
    for (const [atMs, text] of examples.resets.examples) {
      expect(resets(Number(atMs), nowMs, timeZone)).toBe(text);
    }
  });

  it("writes a moment as a day and time", () => {
    const { nowMs, timeZone } = examples.dayAndTime;
    for (const [atMs, text] of examples.dayAndTime.examples) {
      expect(dayAndTime(Number(atMs), nowMs, timeZone)).toBe(text);
    }
  });
});
