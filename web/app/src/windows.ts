// Plan windows as every surface names, orders and labels them: the dashboard, and VS Code's
// status bar and sidebar.

import type { Level, WindowReading } from "@claude-dashboard/protocol";

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
