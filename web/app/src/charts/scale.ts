// Axis scales: round steps that a reader can add up at a glance.

import { grouped, money } from "../format";

export interface Scale {
  /** The value at the top of the axis. */
  top: number;
  /** The values that get a gridline and a label, from the bottom. */
  ticks: number[];
}

/** A scale from 0 with `intervals` equal steps of 1, 2, 2.5 or 5 times a power of ten, the
 * smallest that reaches `max`. */
export function niceScale(max: number, intervals = 4): Scale {
  const target = max > 0 && Number.isFinite(max) ? max / intervals : 1;
  const power = 10 ** Math.floor(Math.log10(target));
  const step =
    [1, 2, 2.5, 5, 10].map((factor) => factor * power).find((candidate) => candidate >= target) ??
    10 * power;
  return {
    top: step * intervals,
    ticks: Array.from({ length: intervals + 1 }, (_, index) => step * index),
  };
}

/** An axis label in US dollars: whole dollars where every step is one, cents otherwise. */
export function moneyTick(value: number, ticks: readonly number[]): string {
  if (value === 0) return "$0";
  return ticks.every((tick) => Number.isInteger(tick)) ? `$${grouped(value)}` : money(value);
}
