// The numbers the status bar item and the sidebar tree show, kept current: read again when the
// server says the history or the plan limits changed, and shown again each minute so that ages and
// reset times stay true, without asking the server.

import { local, startOfWeek } from "@claude-dashboard/app/format";
import * as vscode from "vscode";

import { now } from "./clock";
import type { Server } from "./server";
import type { Summary } from "./shape";

/** How long to wait for more changes before reading again, while Claude Code is busy. */
const QUIET_MS = 1_000;
/** How often ages and reset times are shown again. */
const TICK_MS = 60_000;

export class SummaryModel implements vscode.Disposable {
  #summary: Summary | undefined;
  #importOffered = false;
  #limitsConnected = false;
  #waiting: ReturnType<typeof setTimeout> | undefined;
  #reading: Promise<void> | undefined;
  /** What the next read covers, once the one under way ends. */
  #next: { settings: boolean } | undefined;
  /** Whether the read that waits for changes to stop covers the person's settings too. */
  #soonSettings = false;
  readonly #ticker: ReturnType<typeof setInterval>;
  readonly #changed = new vscode.EventEmitter<void>();
  readonly #subscriptions: vscode.Disposable[];

  /** Fires when there is something new to show, or the same shown at a later moment. */
  readonly onDidChange = this.#changed.event;

  constructor(
    private readonly server: Server,
    private readonly log: vscode.LogOutputChannel,
  ) {
    this.#subscriptions = [
      server.onNotification(({ method }) => {
        if (method === "history/changed") this.#soon(false);
        else if (method === "limits/changed" || method === "settings/changed") this.#soon(true);
      }),
      server.onDidChangeState((state) => {
        if (state.kind === "running") void this.refresh(true);
      }),
    ];
    this.#ticker = setInterval(() => {
      this.#tick();
    }, TICK_MS);
  }

  get summary(): Summary | undefined {
    return this.#summary;
  }

  /** Whether there is claude-usage history to bring in that the person has not turned down. */
  get importOffered(): boolean {
    return this.#importOffered;
  }

  /** Reads everything again; `settings` also reads what changes only with the person's settings,
   * such as whether plan limits are recorded. */
  refresh(settings: boolean): Promise<void> {
    if (this.#reading) {
      this.#next = { settings: settings || (this.#next?.settings ?? false) };
      return this.#reading;
    }
    this.#reading = this.#read(settings).finally(() => {
      this.#reading = undefined;
      const next = this.#next;
      this.#next = undefined;
      if (next) void this.refresh(next.settings);
    });
    return this.#reading;
  }

  dispose(): void {
    clearInterval(this.#ticker);
    clearTimeout(this.#waiting);
    for (const subscription of this.#subscriptions) subscription.dispose();
    this.#changed.dispose();
  }

  async #read(settings: boolean): Promise<void> {
    try {
      const server = this.server;
      const status = await server.request("status", null);
      const filters = { project: null, model: null };
      const [choices, today, week] = await Promise.all([
        server.request("choices", null),
        server.request("overview", { days: { first: status.today, last: status.today }, filters }),
        server.request("overview", {
          days: { first: startOfWeek(status.today, status.weekStart), last: status.today },
          filters,
        }),
      ]);
      if (settings || !this.#summary) {
        const [limits, offer] = await Promise.all([
          server.request("limits", null),
          server.request("import/offer", null),
        ]);
        this.#limitsConnected = limits.statusLine.state === "connected" || limits.endpoint.enabled;
        this.#importOffered = offer !== null && !offer.dismissed && offer.requests > 0;
      }
      this.#summary = { status, choices, today, week, limitsConnected: this.#limitsConnected };
      this.#changed.fire();
    } catch (error) {
      if (this.server.state.kind === "running") {
        this.log.error(`Cannot read the usage summary: ${String(error)}`);
      }
    }
  }

  /** Reads again once changes stop arriving for a moment. */
  #soon(settings: boolean): void {
    const wanted = settings || this.#soonSettings;
    clearTimeout(this.#waiting);
    this.#soonSettings = wanted;
    this.#waiting = setTimeout(() => {
      this.#soonSettings = false;
      void this.refresh(wanted);
    }, QUIET_MS);
  }

  /** Shows the same numbers at the new moment, and reads again once the day has turned. */
  #tick(): void {
    const summary = this.#summary;
    if (summary && local(now(), summary.status.timeZone).day !== summary.status.today) {
      void this.refresh(false);
      return;
    }
    this.#changed.fire();
  }
}
