// A synthetic Claude Code data folder for the dashboard's previews and browser tests: ten weeks of
// sessions across a dozen projects, shaped like the design's examples, and the plan limit readings
// that a status line and the usage endpoint would have recorded. Fixed seeds make every run write
// the same folder.

import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { DatabaseSync } from "node:sqlite";

import { NOW_MS, TIME_ZONE } from "./moment.ts";

export { NOW_MS, TIME_ZONE };

const TODAY = "2026-10-10";
const FIRST_DAY = "2026-07-31";

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

const FABLE = "claude-fable-5-1";
const OPUS = "claude-opus-5-5";
const SONNET = "claude-sonnet-5-5";
const HAIKU = "claude-haiku-5-5";
const UNPRICED = "glm-4.6";

/** About what one request costs on each model, in US dollars. */
const PER_REQUEST: Record<string, number> = {
  [FABLE]: 0.1,
  [OPUS]: 0.05,
  [SONNET]: 0.025,
  [HAIKU]: 0.0015,
};
const VERSION = "2.1.295";

/** What a variant of the folder changes. */
export interface Options {
  /** Claude Code's figure for every Opus session is 6% above the estimate. */
  opusDrift?: boolean;
  /** Leaves out the status line's connection and every plan limit reading. */
  noLimits?: boolean;
  /** Ends the plan limit readings at 09:05, so that they are more than three hours old. */
  staleLimits?: boolean;
  /** The usage endpoint's last turn found Claude Code's login expired. */
  loginExpired?: boolean;
  /** Adds a data folder that is a file, so that it cannot be read. */
  unreadableFolder?: boolean;
  /** Leaves the history at a newer schema than this version knows, as a newer version would. */
  newerSchema?: boolean;
  /** Leaves the history at a newer schema that this version cannot read, as a newer version whose
   * change breaks older readers would. */
  tooNewSchema?: boolean;
  /** Puts a claude-usage dashboard database, with July's requests, in the data folder. */
  claudeUsage?: boolean;
}

/** One plan limit reading, as the history keeps it. */
export interface LimitReading {
  window: string;
  source: "status-line" | "usage-endpoint";
  observedMs: number;
  usedPercentage: number;
  resetsAtMs: number;
}

export interface Synthetic {
  readings: LimitReading[];
  /** The usage endpoint's state; `null` leaves it off and never asked. */
  endpoint: { askedMs: number; outcome: string; nextAskMs: number } | null;
  /** Data folders the person added. */
  folders: string[];
  /** The schema version to leave the history at, when newer than this version's. */
  schema: number | null;
  /** The oldest schema version that can still read the history, when this version cannot. */
  readableFrom: number | null;
}

interface Project {
  cwd: string;
  /** Its share of each day's cost. */
  share: number;
}

const PROJECTS: Record<string, Project> = {
  "claude-dashboard": { cwd: "/home/you/code/claude-dashboard", share: 0.4 },
  "api-gateway": { cwd: "/home/you/code/api-gateway", share: 0.21 },
  website: { cwd: "/home/you/code/website", share: 0.14 },
  infra: { cwd: "/home/you/code/infra", share: 0.12 },
  notes: { cwd: "/home/you/notes", share: 0.05 },
  dotfiles: { cwd: "/home/you/dotfiles", share: 0.016 },
  blog: { cwd: "/home/you/code/blog", share: 0.014 },
  "cli-tools": { cwd: "/home/you/code/cli-tools", share: 0.012 },
  recipes: { cwd: "/home/you/code/recipes", share: 0.011 },
  slides: { cwd: "/home/you/slides", share: 0.01 },
  scratch: { cwd: "/home/you/scratch", share: 0.009 },
  "terraform-modules": { cwd: "/home/you/code/terraform-modules", share: 0.008 },
};

interface Subagent {
  type: string;
  model: string;
  /** Minutes after the session starts. */
  after: number;
  minutes: number;
  requests: number;
  usd: number;
}

interface Session {
  title: string;
  project: string;
  day: string;
  start: string;
  minutes: number;
  /** The models the session itself uses, with each one's share of its own cost. */
  models: [model: string, share: number][];
  requests: number;
  usd: number;
  subagents?: Subagent[];
  /** Still running: Claude Code has written no cost for it yet. */
  running?: boolean;
  /** Claude Code's own figure is this much above the estimate. */
  drift?: number;
  entrypoint?: string;
  skill?: string;
  mcp?: [server: string, tool: string];
}

/** The sessions the design's boards show, by name. */
const NAMED: Session[] = [
  {
    title: "Review the runbook",
    project: "claude-dashboard",
    day: TODAY,
    start: "11:30",
    minutes: 44,
    models: [[OPUS, 1]],
    requests: 31,
    usd: 0.74,
    running: true,
  },
  {
    title: "Fix flaky date test",
    project: "website",
    day: TODAY,
    start: "10:30",
    minutes: 40,
    models: [[SONNET, 1]],
    requests: 44,
    usd: 1.11,
    entrypoint: "claude-vscode",
  },
  {
    title: "Design the ingest lock",
    project: "claude-dashboard",
    day: TODAY,
    start: "10:05",
    minutes: 85,
    models: [[OPUS, 1]],
    requests: 40,
    usd: 1.2,
    subagents: [{ type: "Explore", model: OPUS, after: 12, minutes: 9, requests: 7, usd: 0.05 }],
  },
  {
    title: "Morning triage",
    project: "notes",
    day: TODAY,
    start: "09:25",
    minutes: 25,
    models: [[HAIKU, 1]],
    requests: 9,
    usd: 0.02,
  },
  {
    title: "Price table for Claude 5 models",
    project: "claude-dashboard",
    day: TODAY,
    start: "09:20",
    minutes: 150,
    models: [[FABLE, 1]],
    requests: 41,
    usd: 1.83,
    drift: 0.001,
    skill: "research",
    mcp: ["github", "search_code"],
    subagents: [
      { type: "Explore", model: HAIKU, after: 22, minutes: 9, requests: 11, usd: 0.03 },
      { type: "price-checker", model: HAIKU, after: 95, minutes: 3, requests: 6, usd: 0.01 },
    ],
  },
  {
    title: "Rate limiter for the gateway",
    project: "api-gateway",
    day: "2026-10-09",
    start: "16:05",
    minutes: 190,
    models: [[OPUS, 1]],
    requests: 150,
    usd: 4.4,
    drift: 0.064,
    subagents: [
      { type: "general-purpose", model: SONNET, after: 20, minutes: 14, requests: 12, usd: 0.32 },
      { type: "Explore", model: HAIKU, after: 41, minutes: 6, requests: 9, usd: 0.02 },
      { type: "general-purpose", model: SONNET, after: 96, minutes: 12, requests: 11, usd: 0.3 },
      { type: "Explore", model: HAIKU, after: 150, minutes: 5, requests: 6, usd: 0.01 },
    ],
  },
  {
    title: "Terraform module for the queue",
    project: "infra",
    day: "2026-10-09",
    start: "13:40",
    minutes: 110,
    models: [[SONNET, 1]],
    requests: 81,
    usd: 2.05,
  },
  {
    title: "Rewrite the pricing page",
    project: "website",
    day: "2026-10-09",
    start: "10:15",
    minutes: 125,
    models: [
      [FABLE, 0.6],
      [OPUS, 0.4],
    ],
    requests: 155,
    usd: 5.4,
    subagents: [
      { type: "Explore", model: OPUS, after: 15, minutes: 8, requests: 7, usd: 0.11 },
      { type: "general-purpose", model: OPUS, after: 70, minutes: 6, requests: 5, usd: 0.08 },
    ],
  },
  {
    title: "Investigate slow builds",
    project: "claude-dashboard",
    day: "2026-10-08",
    start: "15:30",
    minutes: 55,
    models: [[OPUS, 1]],
    requests: 40,
    usd: 1.4,
    subagents: [
      { type: "general-purpose", model: UNPRICED, after: 18, minutes: 10, requests: 12, usd: 0 },
    ],
  },
  {
    title: "Incident write-up",
    project: "notes",
    day: "2026-10-08",
    start: "12:10",
    minutes: 35,
    models: [[HAIKU, 1]],
    requests: 14,
    usd: 0.03,
  },
  {
    title: "Schema for limit snapshots",
    project: "claude-dashboard",
    day: "2026-10-08",
    start: "09:05",
    minutes: 160,
    models: [
      [FABLE, 0.45],
      [OPUS, 0.55],
    ],
    requests: 320,
    usd: 11.2,
    subagents: [
      { type: "Explore", model: SONNET, after: 10, minutes: 9, requests: 8, usd: 0.14 },
      { type: "Plan", model: SONNET, after: 52, minutes: 7, requests: 7, usd: 0.12 },
      { type: "general-purpose", model: SONNET, after: 120, minutes: 8, requests: 7, usd: 0.12 },
    ],
  },
  {
    title: "Gateway load test",
    project: "api-gateway",
    day: "2026-10-07",
    start: "14:20",
    minutes: 75,
    models: [[OPUS, 1]],
    requests: 96,
    usd: 2.62,
  },
];

const TITLES = [
  "Tidy the logging setup",
  "Upgrade the test runner",
  "Draft the release notes",
  "Fix the date picker",
  "Explain the retry policy",
  "Add caching to the API client",
  "Review the open pull requests",
  "Speed up the build",
  "Write the migration guide",
  "Refactor the config loader",
  "Investigate the memory spike",
  "Add an integration test",
  "Update the dependencies",
  "Design the settings page",
  "Clean up the old scripts",
  "Document the data model",
  "Fix the flaky upload test",
  "Prototype the export",
  "Check the accessibility audit",
  "Plan the next sprint",
];

/** A pseudo-random sequence: the Park-Miller generator the design's boards use. */
function random(seed: number): () => number {
  let state = seed;
  return () => {
    state = (state * 16807) % 2147483647;
    return (state - 1) / 2147483646;
  };
}

const offsetFormat = new Intl.DateTimeFormat("en-GB", {
  timeZone: TIME_ZONE,
  timeZoneName: "longOffset",
});

function offsetMs(ms: number): number {
  const name =
    offsetFormat.formatToParts(new Date(ms)).find((part) => part.type === "timeZoneName")?.value ??
    "";
  const match = /GMT([+-])(\d{2}):(\d{2})/.exec(name);
  if (!match) return 0;
  const minutes = Number(match[2]) * 60 + Number(match[3]);
  return (match[1] === "-" ? -minutes : minutes) * MINUTE;
}

/** The moment a day and clock time in Auckland stand for. */
export function at(day: string, time: string): number {
  const naive = Date.parse(`${day}T${time}:00Z`);
  return naive - offsetMs(naive - offsetMs(naive));
}

function addDays(day: string, count: number): string {
  return new Date(Date.parse(`${day}T00:00:00Z`) + count * DAY).toISOString().slice(0, 10);
}

function weekday(day: string): number {
  return new Date(`${day}T00:00:00Z`).getUTCDay();
}

interface Rates {
  input: number;
  output: number;
  cache_read: number;
  cache_write_5m: number;
  cache_write_1h: number;
  long_prompt?: Rates & { above: number };
}

const priceTable = JSON.parse(
  readFileSync(new URL("../../../../pricing/prices.json", import.meta.url), "utf8"),
) as { models: { ids: string[]; prices: Rates[] }[] };

function rates(model: string): Rates | undefined {
  return priceTable.models.find((entry) => entry.ids.includes(model))?.prices[0];
}

interface Tokens {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite5m: number;
  cacheWrite1h: number;
}

function cost(model: string, tokens: Tokens): number {
  const standard = rates(model);
  if (!standard) return 0;
  const prompt = tokens.input + tokens.cacheRead + tokens.cacheWrite5m + tokens.cacheWrite1h;
  const long = standard.long_prompt;
  const price = long && prompt > long.above ? long : standard;
  return (
    (tokens.input * price.input +
      tokens.output * price.output +
      tokens.cacheRead * price.cache_read +
      tokens.cacheWrite5m * price.cache_write_5m +
      tokens.cacheWrite1h * price.cache_write_1h) /
    1e6
  );
}

/** Requests whose tokens look like a coding session's, scaled to cost `usd` between them. */
function requestTokens(
  model: string,
  count: number,
  usd: number,
  oneHourCache: boolean,
  next: () => number,
): Tokens[] {
  const context = model === HAIKU ? 30_000 : 60_000;
  const raw = Array.from({ length: count }, (_, index) => {
    const written = Math.round(600 + next() * 5_000);
    return {
      input: Math.round(3 + next() * 40),
      output: Math.round(150 + next() * 2_200),
      cacheRead: Math.round(context * (0.3 + (index / Math.max(1, count)) * 0.9 + next() * 0.2)),
      cacheWrite5m: oneHourCache ? 0 : written,
      cacheWrite1h: oneHourCache ? written : 0,
    };
  });
  const total = raw.reduce((sum, tokens) => sum + cost(model, tokens), 0);
  const scale = usd > 0 && total > 0 ? usd / total : 1;
  return raw.map((tokens) => ({
    input: Math.max(1, Math.round(tokens.input * scale)),
    output: Math.max(1, Math.round(tokens.output * scale)),
    cacheRead: Math.round(tokens.cacheRead * scale),
    cacheWrite5m: Math.round(tokens.cacheWrite5m * scale),
    cacheWrite1h: Math.round(tokens.cacheWrite1h * scale),
  }));
}

/** Claude Code's name for the folder that holds a working folder's transcripts. */
function transcriptFolder(cwd: string): string {
  return cwd.replace(/[^A-Za-z0-9]/g, "-");
}

function uuid(next: () => number): string {
  const hex = Array.from({ length: 32 }, () => Math.floor(next() * 16).toString(16)).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-4${hex.slice(13, 16)}-8${hex.slice(17, 20)}-${hex.slice(20, 32)}`;
}

function writeLines(path: string, records: unknown[]): void {
  mkdirSync(dirname(path), { recursive: true });
  appendFileSync(path, records.map((record) => `${JSON.stringify(record)}\n`).join(""));
}

interface Reported {
  usd: number;
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
}

/** Writes one session's transcripts, with the cost Claude Code reports for it once it ends. */
function writeSession(root: string, session: Session, options: Options, next: () => number): void {
  const project = PROJECTS[session.project];
  if (!project) throw new Error(`no project ${session.project}`);
  const id = uuid(next);
  const folder = join(root, "projects", transcriptFolder(project.cwd));
  const startMs = at(session.day, session.start);
  const oneHourCache = next() < 0.5;
  const reported = new Map<string, Reported>();
  let fastShare = 0;

  const records = (
    model: string,
    count: number,
    usd: number,
    fromMs: number,
    minutes: number,
    agent: string | undefined,
  ): unknown[] =>
    requestTokens(model, count, usd, oneHourCache, next).map((tokens, index) => {
      const timestampMs = Math.round(
        fromMs + ((index + 0.5 + (next() - 0.5) * 0.6) / count) * minutes * MINUTE,
      );
      const fast = model === OPUS && (fastShare += 0.06) >= 1;
      if (fast) fastShare -= 1;
      const entry = reported.get(model) ?? {
        usd: 0,
        input: 0,
        output: 0,
        cacheRead: 0,
        cacheWrite: 0,
      };
      entry.usd += cost(model, tokens) * (fast ? 2 : 1);
      entry.input += tokens.input + 40;
      entry.output += tokens.output;
      entry.cacheRead += tokens.cacheRead;
      entry.cacheWrite += tokens.cacheWrite5m + tokens.cacheWrite1h;
      reported.set(model, entry);
      const credited =
        agent === undefined && index % 4 === 1 && session.skill !== undefined
          ? { attributionSkill: session.skill }
          : agent === undefined && index % 7 === 3 && session.mcp !== undefined
            ? { attributionMcpServer: session.mcp[0], attributionMcpTool: session.mcp[1] }
            : {};
      return {
        type: "assistant",
        uuid: uuid(next),
        parentUuid: null,
        isSidechain: agent !== undefined,
        ...(agent === undefined ? {} : { agentId: agent }),
        sessionId: id,
        requestId: `req_${uuid(next).replaceAll("-", "")}`,
        timestamp: new Date(timestampMs).toISOString(),
        cwd: project.cwd,
        version: VERSION,
        entrypoint: session.entrypoint ?? "cli",
        ...credited,
        message: {
          id: `msg_${uuid(next).replaceAll("-", "")}`,
          type: "message",
          role: "assistant",
          model,
          content: [],
          usage: {
            input_tokens: tokens.input,
            output_tokens: tokens.output,
            cache_read_input_tokens: tokens.cacheRead,
            cache_creation_input_tokens: tokens.cacheWrite5m + tokens.cacheWrite1h,
            cache_creation: {
              ephemeral_5m_input_tokens: tokens.cacheWrite5m,
              ephemeral_1h_input_tokens: tokens.cacheWrite1h,
            },
            ...(fast ? { speed: "fast" } : {}),
            service_tier: "standard",
          },
        },
      };
    });

  const own: unknown[] = [];
  let left = session.requests;
  session.models.forEach(([model, share], index) => {
    const count = index === session.models.length - 1 ? left : Math.round(session.requests * share);
    left -= count;
    const from = startMs + (session.minutes * MINUTE * index) / session.models.length;
    own.push(
      ...records(
        model,
        count,
        session.usd * share,
        from,
        session.minutes / session.models.length,
        undefined,
      ),
    );
  });
  writeLines(join(folder, `${id}.jsonl`), [
    { type: "ai-title", sessionId: id, aiTitle: session.title },
    ...own,
  ]);

  for (const subagent of session.subagents ?? []) {
    const agent = `a${uuid(next).replaceAll("-", "").slice(0, 16)}`;
    const path = join(folder, id, "subagents", `agent-${agent}`);
    writeLines(
      `${path}.jsonl`,
      records(
        subagent.model,
        subagent.requests,
        subagent.usd,
        startMs + subagent.after * MINUTE,
        subagent.minutes,
        agent,
      ),
    );
    writeFileSync(
      `${path}.meta.json`,
      JSON.stringify({ agentType: subagent.type, spawnDepth: 1, model: subagent.model }),
    );
  }

  if (!session.running) {
    const drift =
      session.drift ??
      (options.opusDrift && session.models.some(([model]) => model === OPUS)
        ? 0.064
        : next() * 0.006);
    const modelUsage = Object.fromEntries(
      [...reported]
        .filter(([model]) => rates(model) !== undefined)
        .map(([model, entry]) => [
          model,
          {
            inputTokens: entry.input,
            outputTokens: entry.output,
            cacheReadInputTokens: entry.cacheRead,
            cacheCreationInputTokens: entry.cacheWrite,
            costUSD: Math.round(entry.usd * (1 + drift) * 1e6) / 1e6,
          },
        ]),
    );
    writeLines(join(folder, `${id}.jsonl`), [
      {
        type: "cost-state",
        sessionId: id,
        startTime: startMs,
        modelUsage,
        timestamp: new Date(startMs + session.minutes * MINUTE).toISOString(),
      },
    ]);
  }
}

/** The cost of each model on each day, from the design's boards: a busy weekday pattern for the
 * last 30 days, and a quieter one before. */
function dailyCosts(): Map<string, Record<string, number>> {
  const days = new Map<string, Record<string, number>>();
  const recent = random(11);
  const older = random(29);
  for (let day = FIRST_DAY; day <= TODAY; day = addDays(day, 1)) {
    const weekend = weekday(day) === 0 || weekday(day) === 6;
    const isRecent = day >= "2026-09-11";
    const next = isRecent ? recent : older;
    const scale = (weekend ? 0.4 : 1) * (isRecent ? 1 : 0.55);
    days.set(day, {
      [FABLE]: scale * (1.4 + next() * 4.2),
      [OPUS]: scale * (2.8 + next() * 6.4),
      [SONNET]: scale * (0.7 + next() * 2.4),
      [HAIKU]: scale * (0.1 + next() * 0.45),
    });
  }
  return days;
}

function namedCost(day: string): Record<string, number> {
  const spent: Record<string, number> = {};
  for (const session of NAMED.filter((named) => named.day === day)) {
    for (const [model, share] of session.models) {
      spent[model] = (spent[model] ?? 0) + session.usd * share;
    }
    for (const subagent of session.subagents ?? []) {
      spent[subagent.model] = (spent[subagent.model] ?? 0) + subagent.usd;
    }
  }
  return spent;
}

/** Sessions that make up the rest of each day's cost. */
function generatedSessions(): Session[] {
  const next = random(7);
  const projects = Object.entries(PROJECTS);
  const sessions: Session[] = [];
  let title = 0;
  for (const [day, costs] of dailyCosts()) {
    if (day === TODAY) continue;
    const spent = namedCost(day);
    for (const [model, usd] of Object.entries(costs)) {
      let left = usd - (spent[model] ?? 0);
      while (left > 0.02) {
        const usdOfSession = Math.min(
          left,
          model === HAIKU ? 0.05 + next() * 0.15 : 1 + next() * 4,
        );
        left -= usdOfSession;
        let pick = next();
        const [project] = projects.find(([, candidate]) => (pick -= candidate.share) < 0) ??
          projects[0] ?? ["notes"];
        const startMinutes = 8 * 60 + Math.floor(next() * 10 * 60);
        const minutes = 15 + Math.floor(next() * 120);
        const perRequest = PER_REQUEST[model] ?? 0.04;
        sessions.push({
          title: TITLES[title++ % TITLES.length] ?? "Session",
          project,
          day,
          start: `${String(Math.floor(startMinutes / 60)).padStart(2, "0")}:${String(startMinutes % 60).padStart(2, "0")}`,
          minutes,
          models: [[model, 1]],
          requests: Math.max(3, Math.round(usdOfSession / perRequest)),
          usd: usdOfSession,
          entrypoint: next() < 0.3 ? "claude-vscode" : "cli",
        });
      }
    }
  }
  return sessions;
}

/** Readings of a plan window that rise through `points`: hours after `fromMs`, and the share
 * used then. */
function rising(
  window: string,
  source: LimitReading["source"],
  fromMs: number,
  resetsAtMs: number,
  points: [hours: number, used: number][],
): LimitReading[] {
  return points.map(([hours, used]) => ({
    window,
    source,
    observedMs: Math.round(fromMs + hours * HOUR),
    usedPercentage: used,
    resetsAtMs,
  }));
}

function limitReadings(options: Options): Synthetic["readings"] {
  if (options.noLimits) return [];
  const weekStart = at("2026-10-08", "09:00");
  const weekEnd = at("2026-10-15", "09:00");
  const readings = [
    ...rising("five_hour", "status-line", at("2026-10-08", "08:50"), at("2026-10-08", "13:50"), [
      [0.1, 2],
      [1, 18],
      [2, 31],
      [3, 44],
      [4.2, 55],
    ]),
    ...rising("five_hour", "status-line", at("2026-10-08", "13:15"), at("2026-10-08", "18:15"), [
      [0.1, 4],
      [1, 30],
      [2, 58],
      [3, 81],
      [4.5, 97],
    ]),
    ...rising("five_hour", "status-line", at("2026-10-09", "08:30"), at("2026-10-09", "13:30"), [
      [0.2, 5],
      [1.5, 29],
      [3, 47],
      [4.6, 64],
    ]),
    ...rising("five_hour", "status-line", at("2026-10-09", "14:05"), at("2026-10-09", "19:05"), [
      [0.1, 3],
      [1, 25],
      [2, 49],
      [3, 70],
      [4.4, 88],
    ]),
    ...(options.staleLimits
      ? rising("five_hour", "status-line", at(TODAY, "07:40"), at(TODAY, "12:40"), [
          [0.1, 3],
          [0.6, 21],
          [1.42, 38],
        ])
      : rising("five_hour", "status-line", at(TODAY, "09:20"), at(TODAY, "14:20"), [
          [0.02, 0],
          [20 / 60, 4],
          [45 / 60, 9],
          [70 / 60, 15],
          [95 / 60, 20],
          [120 / 60, 27],
          [145 / 60, 34],
          [173 / 60, 42],
        ])),
    ...rising("seven_day", "status-line", weekStart, weekEnd, [
      [0.1, 0],
      [3, 6],
      [6, 13],
      [9, 20],
      [13, 26],
      [16, 27],
      [24, 28],
      [27, 36],
      [30, 44],
      [33, 51],
      [37, 58],
      [40, 61],
      [48, 61],
      [51 + 13 / 60, 71],
    ]),
    ...rising("seven_day:Fable", "usage-endpoint", weekStart, weekEnd, [
      [6, 4],
      [30, 11],
      [51 + 10 / 60, 18],
    ]),
  ];
  const lastMs = options.staleLimits ? at(TODAY, "09:05") : NOW_MS;
  return readings.filter((reading) => reading.observedMs <= lastMs);
}

/** Writes the synthetic Claude Code data folder into `root`, which must be empty. */
export function writeClaudeFolder(root: string, options: Options = {}): Synthetic {
  mkdirSync(join(root, "projects"), { recursive: true });
  if (!options.noLimits) {
    writeFileSync(
      join(root, "settings.json"),
      `${JSON.stringify({ statusLine: { type: "command", command: "claude-dashboard statusline" } }, null, 2)}\n`,
    );
  }
  const next = random(3);
  for (const session of [...NAMED, ...generatedSessions()]) {
    writeSession(root, session, options, next);
  }
  if (options.claudeUsage) writeClaudeUsage(join(root, "usage.db"));
  const folders: string[] = [];
  if (options.unreadableFolder) {
    const file = join(dirname(root), "wsl.localhost-Ubuntu-.claude");
    writeFileSync(file, "");
    folders.push(file);
  }
  return {
    readings: limitReadings(options),
    endpoint: options.noLimits
      ? null
      : {
          askedMs: at(TODAY, "12:10"),
          outcome: options.loginExpired ? "login-expired" : "answered",
          nextAskMs: NOW_MS + 5 * MINUTE,
        },
    folders,
    schema: options.newerSchema || options.tooNewSchema ? 99 : null,
    readableFrom: options.tooNewSchema ? 99 : null,
  };
}

/** A claude-usage dashboard database holding requests from July, before any transcript. */
function writeClaudeUsage(path: string): void {
  const database = new DatabaseSync(path);
  try {
    database.exec(`
      CREATE TABLE sessions (session_id TEXT PRIMARY KEY, project_name TEXT, topic TEXT);
      CREATE TABLE turns (
        id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT, timestamp TEXT, model TEXT,
        input_tokens INTEGER, output_tokens INTEGER, cache_read_tokens INTEGER,
        cache_creation_tokens INTEGER, tool_name TEXT, cwd TEXT, message_id TEXT,
        is_subagent INTEGER DEFAULT 0, agent_id TEXT
      );`);
    const insert = database.prepare(
      `INSERT INTO turns (session_id, timestamp, model, input_tokens, output_tokens,
         cache_read_tokens, cache_creation_tokens, cwd, message_id)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    );
    const next = random(17);
    for (let day = "2026-07-01"; day <= "2026-07-30"; day = addDays(day, 1)) {
      for (let index = 0; index < 129; index += 1) {
        insert.run(
          `july-${day}-${String(index % 4)}`,
          new Date(at(day, "10:00") + index * 3 * MINUTE).toISOString(),
          next() < 0.7 ? OPUS : SONNET,
          Math.round(5 + next() * 30),
          Math.round(200 + next() * 1500),
          Math.round(20_000 + next() * 60_000),
          Math.round(500 + next() * 4_000),
          "/home/you/code/claude-dashboard",
          `msg_july_${day}_${String(index)}`,
        );
      }
    }
  } finally {
    database.close();
  }
}
