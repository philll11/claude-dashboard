// Numbers, money, days and times as the design writes them, the same way the CLI does:
// design/formats.json holds the examples that both are tested against. Days are `YYYY-MM-DD`
// strings and times are milliseconds, shown in the time zone the server reports.

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
] as const;
const WEEKDAYS = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
] as const;

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** A count with thousands separators: `2,775`. */
export function grouped(count: number): string {
  return Math.round(count)
    .toString()
    .replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/** A count of tokens in a few characters: `640`, `3K`, `166K`, `1.0M`, `150M`, `11.5B`. */
export function compact(count: number): string {
  const scaled = (divisor: number, unit: string) => {
    const value = count / divisor;
    return value >= 100
      ? `${String(Math.round(value))}${unit}`
      : `${(Math.round(value * 10) / 10).toFixed(1)}${unit}`;
  };
  if (count >= 999_500_000) return scaled(1e9, "B");
  if (count >= 999_500) return scaled(1e6, "M");
  if (count >= 1_000) return `${String(Math.round(count / 1e3))}K`;
  return String(Math.round(count));
}

/** US dollars to the cent, with thousands separators: `$5.05`, `$1,380.57`. */
export function money(usd: number): string {
  const cents = Math.max(0, Math.round(usd * 100));
  return `$${grouped(Math.floor(cents / 100))}.${String(cents % 100).padStart(2, "0")}`;
}

/** A share as a whole percentage: `42%`. */
export function percent(share: number): string {
  return `${String(Math.round(share))}%`;
}

/** How long ago something happened: `just now`, `2 min ago`, `3 h ago`, `4 days ago`. */
export function ago(thenMs: number, nowMs: number): string {
  const minutes = Math.floor(
    Math.max(0, Math.floor(nowMs / 1000) - Math.floor(thenMs / 1000)) / 60,
  );
  if (minutes === 0) return "just now";
  if (minutes < 60) return `${String(minutes)} min ago`;
  if (minutes < 1440) return `${String(Math.floor(minutes / 60))} h ago`;
  const days = Math.floor(minutes / 1440);
  return `${String(days)} ${days === 1 ? "day" : "days"} ago`;
}

/** A length of time: `45 min`, `2 h 05 min`, `4 days`. */
export function duration(ms: number): string {
  const minutes = Math.max(0, Math.round(ms / MINUTE));
  if (minutes < 60) return `${String(minutes)} min`;
  if (minutes < 1440) {
    const rest = minutes % 60;
    return `${String(Math.floor(minutes / 60))} h ${String(rest).padStart(2, "0")} min`;
  }
  const days = Math.round(minutes / 1440);
  return `${String(days)} ${days === 1 ? "day" : "days"}`;
}

interface CalendarDay {
  year: number;
  month: number;
  day: number;
}

function parseDay(day: string): CalendarDay {
  const [year, month, date] = day.split("-").map(Number);
  return { year: year ?? 1970, month: month ?? 1, day: date ?? 1 };
}

function weekday(day: CalendarDay): string {
  return WEEKDAYS[new Date(Date.UTC(day.year, day.month - 1, day.day)).getUTCDay()] ?? "";
}

function monthName(day: CalendarDay): string {
  return MONTHS[day.month - 1] ?? "";
}

/** A day as the design writes it: `Saturday 10 October`. */
export function longDay(day: string): string {
  const parsed = parseDay(day);
  return `${weekday(parsed)} ${String(parsed.day)} ${monthName(parsed)}`;
}

/** A date with the year: `1 Oct 2026`. */
export function shortDate(day: string): string {
  const parsed = parseDay(day);
  return `${String(parsed.day)} ${monthName(parsed).slice(0, 3)} ${String(parsed.year)}`;
}

/** A day as a chart or a tooltip writes it: `Sun 6 Oct`. */
export function dayLabel(day: string): string {
  const parsed = parseDay(day);
  return `${weekday(parsed).slice(0, 3)} ${String(parsed.day)} ${monthName(parsed).slice(0, 3)}`;
}

/** The days from `first` to `last`, both included. */
export function daysBetween(first: string, last: string): string[] {
  const start = parseDay(first);
  const end = parseDay(last);
  const days: string[] = [];
  for (
    let at = Date.UTC(start.year, start.month - 1, start.day);
    at <= Date.UTC(end.year, end.month - 1, end.day);
    at += DAY
  ) {
    days.push(new Date(at).toISOString().slice(0, 10));
  }
  return days;
}

/** The day `count` days after `day`, or before it when `count` is negative. */
export function addDays(day: string, count: number): string {
  const parsed = parseDay(day);
  return new Date(Date.UTC(parsed.year, parsed.month - 1, parsed.day) + count * DAY)
    .toISOString()
    .slice(0, 10);
}

/** The first day of the week `day` is in, for weeks starting on `weekStart`, such as `monday`. */
export function startOfWeek(day: string, weekStart: string): string {
  const weekday = new Date(`${day}T00:00:00Z`).getUTCDay();
  const start = Math.max(
    0,
    WEEKDAYS.findIndex((name) => name.toLowerCase() === weekStart.toLowerCase()),
  );
  return addDays(day, -((weekday - start + 7) % 7));
}

interface Local {
  day: string;
  time: string;
}

const formatters = new Map<string, Intl.DateTimeFormat>();

/** A moment's day and clock time in `timeZone`. */
export function local(ms: number, timeZone: string): Local {
  let formatter = formatters.get(timeZone);
  if (formatter === undefined) {
    formatter = new Intl.DateTimeFormat("en-GB", {
      timeZone,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      hourCycle: "h23",
    });
    formatters.set(timeZone, formatter);
  }
  const parts = Object.fromEntries(
    formatter.formatToParts(new Date(ms)).map((part) => [part.type, part.value]),
  );
  return {
    day: `${parts.year ?? ""}-${parts.month ?? ""}-${parts.day ?? ""}`,
    time: `${parts.hour ?? ""}:${parts.minute ?? ""}`,
  };
}

function dayNumber(day: string): number {
  const parsed = parseDay(day);
  return Date.UTC(parsed.year, parsed.month - 1, parsed.day) / DAY;
}

/** When something happens, from `nowMs`'s point of view: `14:20` today, `Thu 09:00` within a
 * week, and `20 Oct 09:00` after that. */
export function when(atMs: number, nowMs: number, timeZone: string): string {
  const at = local(atMs, timeZone);
  const days = dayNumber(at.day) - dayNumber(local(nowMs, timeZone).day);
  if (days === 0) return at.time;
  const parsed = parseDay(at.day);
  if (days >= 1 && days < 7) return `${weekday(parsed).slice(0, 3)} ${at.time}`;
  return `${String(parsed.day)} ${monthName(parsed).slice(0, 3)} ${at.time}`;
}

/** A date in full: `31 July 2026`. */
export function fullDate(day: string): string {
  const parsed = parseDay(day);
  return `${String(parsed.day)} ${monthName(parsed)} ${String(parsed.year)}`;
}

/** A month and year: `October 2026`. */
export function monthYear(day: string): string {
  const parsed = parseDay(day);
  return `${monthName(parsed)} ${String(parsed.year)}`;
}

/** The day of a moment in the past, for a sentence: `today`, `yesterday`, or `on 12 October
 * 2026`. */
export function dayWords(atMs: number, nowMs: number, timeZone: string): string {
  const days = dayNumber(local(nowMs, timeZone).day) - dayNumber(local(atMs, timeZone).day);
  if (days === 0) return "today";
  if (days === 1) return "yesterday";
  return `on ${fullDate(local(atMs, timeZone).day)}`;
}

/** A moment in the past as a day and time, for a sentence: `today at 08:00`. */
export function dayWordsAt(atMs: number, nowMs: number, timeZone: string): string {
  return `${dayWords(atMs, nowMs, timeZone)} at ${local(atMs, timeZone).time}`;
}

/** A moment as a day and time: `Today 11:30`, or `Fri 9 Oct 16:05` on another day. */
export function dayAndTime(atMs: number, nowMs: number, timeZone: string): string {
  const at = local(atMs, timeZone);
  if (at.day === local(nowMs, timeZone).day) return `Today ${at.time}`;
  return `${dayLabel(at.day)} ${at.time}`;
}

/** When a plan window resets: `Resets 14:20, in 2 h 05 min` within a day, and
 * `Resets Thu 15 Oct, 09:00` after that. */
export function resets(atMs: number, nowMs: number, timeZone: string): string {
  const at = local(atMs, timeZone);
  const sameDay = at.day === local(nowMs, timeZone).day;
  const when = sameDay ? at.time : `${dayLabel(at.day)}, ${at.time}`;
  return atMs - nowMs < DAY && atMs > nowMs
    ? `Resets ${when}, in ${duration(atMs - nowMs)}`
    : `Resets ${when}`;
}

/** A price in US dollars per million tokens, to the cent or to a tenth of a cent: `$12.50`,
 * `$0.125`. */
export function price(usd: number): string {
  const tenths = Math.round(usd * 1000);
  return tenths % 10 === 0 ? money(usd) : `$${(tenths / 1000).toFixed(3)}`;
}
