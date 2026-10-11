// Which colour each model gets. Colours follow the model family, in the fixed order the design
// validated (ADR 0007); a family's newest model gets its colour, and every other model shares the
// neutral colour of "other models".

export const SERIES = ["fable", "opus", "sonnet", "haiku"] as const;
export type Family = (typeof SERIES)[number];
export type Series = Family | "other";

interface Parsed {
  family: Family;
  version: number[];
}

function parse(model: string): Parsed | undefined {
  const match = /claude-(fable|opus|sonnet|haiku)-(\d+)(?:-(\d{1,2}))?(?!\d)/.exec(model);
  if (!match) return undefined;
  return {
    family: match[1] as Family,
    version: [Number(match[2]), Number(match[3] ?? 0)],
  };
}

function newer(a: number[], b: number[]): boolean {
  for (let index = 0; index < Math.max(a.length, b.length); index += 1) {
    const difference = (a[index] ?? 0) - (b[index] ?? 0);
    if (difference !== 0) return difference > 0;
  }
  return false;
}

/** The series of each of `models`: each family's newest model gets the family's colour. */
export function seriesOf(models: Iterable<string>): Map<string, Series> {
  const newest = new Map<Family, { model: string; version: number[] }>();
  const all = [...models];
  for (const model of all) {
    const parsed = parse(model);
    if (!parsed) continue;
    const best = newest.get(parsed.family);
    if (!best || newer(parsed.version, best.version)) {
      newest.set(parsed.family, { model, version: parsed.version });
    }
  }
  const series = new Map<string, Series>();
  for (const model of all) {
    const family = parse(model)?.family;
    series.set(model, family && newest.get(family)?.model === model ? family : "other");
  }
  return series;
}

/** The order series are stacked and listed in. */
export function seriesRank(series: Series): number {
  const index = SERIES.indexOf(series as Family);
  return index < 0 ? SERIES.length : index;
}

/** The CSS colour, or high contrast pattern, of a series. */
export function seriesFill(series: Series): string {
  return `var(--fill-${series})`;
}
