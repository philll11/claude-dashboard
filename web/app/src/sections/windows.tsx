// Plan windows as every section names, orders and labels them.

import type { Level, WindowReading } from "@claude-dashboard/protocol";

import { ago } from "../format";
import { CheckCircle, Clock, Warning } from "../ui/icons";

/** What a window is called: `Five-hour window`, `Weekly, all models`, `Weekly, Fable`. */
export function windowName(reading: WindowReading): string {
  if (reading.window === "five_hour") return "Five-hour window";
  if (reading.window === "seven_day") return "Weekly, all models";
  if (reading.model) return `Weekly, ${reading.model}`;
  if (reading.window === "spend_limit") return "Spend limit";
  return reading.window;
}

/** What a screen reader calls a window's meter: `Weekly Fable window`. */
export function meterName(reading: WindowReading): string {
  if (reading.window === "five_hour") return "Five-hour window";
  if (reading.window === "seven_day") return "Weekly window";
  if (reading.model) return `Weekly ${reading.model} window`;
  return windowName(reading);
}

function rank(reading: WindowReading): number {
  if (reading.window === "five_hour") return 0;
  if (reading.window === "seven_day") return 1;
  if (reading.model) return 2;
  return 3;
}

/** Five-hour first, then weekly, then each model's weekly window, then any other. */
export function inOrder(readings: readonly WindowReading[]): WindowReading[] {
  return [...readings].sort(
    (a, b) => rank(a) - rank(b) || (a.model ?? a.window).localeCompare(b.model ?? b.window),
  );
}

export const LEVEL_TEXT: Record<Level, string | undefined> = {
  normal: undefined,
  warning: "Over 70%",
  critical: "Over 90%",
};

/** A warning mark in the colour of its level. */
export function LevelIcon({ level, size = 13 }: { level: Level; size?: number }) {
  return (
    <span className={`level-icon ${level}`}>
      <Warning size={size} />
    </span>
  );
}

/** Where the newest reading comes from, and how old it is: `Official · status line · 2 min ago`. */
export function Provenance({
  readings,
  nowMs,
}: {
  readings: readonly WindowReading[];
  nowMs: number;
}) {
  const newest = readings.reduce<WindowReading | undefined>(
    (best, reading) => (best && best.observedMs >= reading.observedMs ? best : reading),
    undefined,
  );
  if (!newest) return null;
  const source =
    newest.provenance === "official" ? "Official · status line" : "Experimental · usage endpoint";
  if (newest.stale) {
    return (
      <span className="badge caution">
        <Clock size={12} />
        {source} · {ago(newest.observedMs, nowMs)}
      </span>
    );
  }
  return (
    <span className="badge">
      <CheckCircle size={12} />
      {source} · {ago(newest.observedMs, nowMs)}
    </span>
  );
}
