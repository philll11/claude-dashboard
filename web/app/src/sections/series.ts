// The series of a cost chart: one per model family, in the design's fixed order, and one for every
// other model together.

import type { Day, ModelRow } from "@claude-dashboard/protocol";

import type { Column, SeriesInfo } from "../charts/columns";
import { dayLabel, daysBetween, monthYear, shortDate, startOfWeek } from "../format";
import { type Series, seriesFill, seriesRank } from "../models";

export interface CostSeries extends SeriesInfo {
  series: Series;
  total: number;
}

export interface CostChart {
  series: CostSeries[];
  /** The series of each model id. */
  seriesOfModel: Map<string, Series>;
}

/** The series of the models that have a cost, bottom to top, coloured as `seriesOfModel` says. */
export function costSeries(
  models: readonly ModelRow[],
  seriesOfModel: ReadonlyMap<string, Series>,
): CostChart {
  const priced = models.filter((row) => row.totals.cost.total > 0);
  const chosen = new Map<string, Series>();
  const bySeries = new Map<Series, CostSeries>();
  for (const row of priced) {
    const series = seriesOfModel.get(row.model.id) ?? "other";
    chosen.set(row.model.id, series);
    const existing = bySeries.get(series);
    if (existing) {
      existing.total += row.totals.cost.total;
      existing.name = "Other models";
    } else {
      bySeries.set(series, {
        id: series,
        series,
        name: row.model.name,
        fill: seriesFill(series),
        total: row.totals.cost.total,
      });
    }
  }
  return {
    series: [...bySeries.values()].sort((a, b) => seriesRank(a.series) - seriesRank(b.series)),
    seriesOfModel: chosen,
  };
}

export type Bucket = "day" | "week" | "month";

/** How finely a chart of `days` days can show them. */
export function bucketOf(dayCount: number): Bucket {
  if (dayCount <= 62) return "day";
  if (dayCount <= 434) return "week";
  return "month";
}

/** Columns of cost by series from `first` to `last`, one per day, week or month. */
export function costColumns(
  byDay: readonly Day[],
  chart: CostChart,
  first: string,
  last: string,
  weekStart: string,
): { bucket: Bucket; columns: Column[] } {
  const days = daysBetween(first, last);
  const bucket = bucketOf(days.length);
  const keyOf = (day: string) =>
    bucket === "day"
      ? day
      : bucket === "week"
        ? startOfWeek(day, weekStart)
        : `${day.slice(0, 7)}-01`;
  const columns = new Map<string, Column>();
  for (const day of days) {
    const key = keyOf(day);
    if (columns.has(key)) continue;
    columns.set(key, {
      key,
      label:
        bucket === "day"
          ? String(Number(day.slice(8)))
          : bucket === "week"
            ? shortDate(key).replace(/ \d{4}$/, "")
            : monthYear(key).slice(0, 3),
      title:
        bucket === "day"
          ? dayLabel(day)
          : bucket === "week"
            ? `Week of ${dayLabel(key)}`
            : monthYear(key),
      values: new Map(),
    });
  }
  for (const day of byDay) {
    const column = columns.get(keyOf(day.date));
    if (!column) continue;
    for (const part of day.models) {
      const series = chart.seriesOfModel.get(part.model);
      if (!series) continue;
      column.values.set(series, (column.values.get(series) ?? 0) + part.totals.cost.total);
    }
  }
  return { bucket, columns: [...columns.values()] };
}

/** Each day's total cost from `first` to `last`, with nothing on days without requests. */
export function dailyCost(byDay: readonly Day[], first: string, last: string): number[] {
  return costPerDay(
    byDay.map((day) => [day.date, day.totals.cost.total] as const),
    first,
    last,
  );
}

/** Each day's cost from `first` to `last`, from days and their costs, with nothing on other days. */
export function costPerDay(
  costs: Iterable<readonly [string, number]>,
  first: string,
  last: string,
): number[] {
  const byDay = new Map(costs);
  return daysBetween(first, last).map((day) => byDay.get(day) ?? 0);
}
