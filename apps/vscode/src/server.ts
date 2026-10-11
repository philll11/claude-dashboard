// `claude-dashboard serve --stdio` as a child process, spoken to in JSON-RPC (ADR 0006). The
// server does the reading and the work, so the extension host stays idle. It is restarted after
// a crash unless it keeps crashing, and ends with the extension, since it stops once its standard
// input closes.

import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import * as os from "node:os";

import type { Method, Notifications, Params, Result } from "@claude-dashboard/protocol";
import * as vscode from "vscode";
import {
  createMessageConnection,
  type MessageConnection,
  StreamMessageReader,
  StreamMessageWriter,
} from "vscode-jsonrpc/node";

import { findProgram } from "./program";

export type ServerState =
  | { kind: "starting" }
  | { kind: "running" }
  /** It stopped and is not restarted, for the reason given. */
  | { kind: "stopped"; reason: string }
  /** There is no program to run, for the reason given. */
  | { kind: "noProgram"; reason: string };

export interface Notification {
  method: keyof Notifications;
  params: unknown;
}

const NOTIFICATIONS: (keyof Notifications)[] = [
  "ingest/progress",
  "history/changed",
  "limits/changed",
];

/** Crashes within this long count towards giving up. */
const CRASH_WINDOW_MS = 3 * 60_000;
/** It is restarted after this many crashes within the window, and not after one more. */
const RESTARTS = 4;
/** How long it waits before a restart, for each crash within the window. */
const RESTART_DELAY_MS = 1_000;
/** How long it has to end after its input closes before it is stopped by force. */
const END_TIMEOUT_MS = 5_000;

/** A request that cannot reach the server. */
export class Unreachable extends Error {}

export class Server implements vscode.Disposable {
  #state: ServerState = { kind: "starting" };
  #child: ChildProcessWithoutNullStreams | undefined;
  #connection: MessageConnection | undefined;
  #crashes: number[] = [];
  #disposed = false;
  readonly #stateChanged = new vscode.EventEmitter<ServerState>();
  readonly #notified = new vscode.EventEmitter<Notification>();

  /** Fires when the server starts, stops or cannot start. */
  readonly onDidChangeState = this.#stateChanged.event;
  /** Fires for each notification the server sends. */
  readonly onNotification = this.#notified.event;

  constructor(
    private readonly extensionPath: string,
    private readonly log: vscode.LogOutputChannel,
  ) {}

  get state(): ServerState {
    return this.#state;
  }

  async start(): Promise<void> {
    if (this.#disposed) return;
    this.#set({ kind: "starting" });
    const found = await findProgram(this.extensionPath);
    if (found.kind === "missing") {
      this.log.error(found.reason);
      this.#set({ kind: "noProgram", reason: found.reason });
      return;
    }
    const args = ["serve", "--stdio", ...(found.bundled ? ["--status-line-copy"] : [])];
    this.log.info(`Starting ${found.path} ${args.join(" ")}`);
    const child = spawn(found.path, args, { cwd: os.homedir(), windowsHide: true });
    const connection = createMessageConnection(
      new StreamMessageReader(child.stdout),
      new StreamMessageWriter(child.stdin),
    );
    for (const method of NOTIFICATIONS) {
      connection.onNotification(method, (params: unknown) => {
        this.#notified.fire({ method, params });
      });
    }
    connection.onError(([error]) => {
      this.log.error(`The connection to claude-dashboard failed: ${error.message}`);
    });
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (text: string) => {
      for (const line of text.split(/\r?\n/)) if (line.trim()) this.log.warn(line);
    });
    child.on("error", (error) => {
      this.#ended(child, `claude-dashboard could not start: ${error.message}`);
    });
    child.on("exit", (code, signal) => {
      this.#ended(
        child,
        signal
          ? `claude-dashboard ended on ${signal}`
          : `claude-dashboard ended with code ${String(code)}`,
      );
    });
    connection.listen();
    this.#child = child;
    this.#connection = connection;
    this.#set({ kind: "running" });
  }

  /** Starts the server again, for instance after a change of program, forgetting past crashes. */
  async restart(): Promise<void> {
    this.#crashes = [];
    await this.#stop();
    await this.start();
  }

  request<M extends Method>(method: M, params: Params<M>): Promise<Result<M>> {
    const connection = this.#connection;
    if (!connection || this.#state.kind !== "running") {
      return Promise.reject(new Unreachable("claude-dashboard is not running."));
    }
    return connection.sendRequest<Result<M>>(method, params);
  }

  dispose(): void {
    this.#disposed = true;
    void this.#stop();
    this.#stateChanged.dispose();
    this.#notified.dispose();
  }

  #set(state: ServerState): void {
    this.#state = state;
    this.#stateChanged.fire(state);
  }

  /** What follows the child ending: nothing when it was asked to, and otherwise a restart, unless
   * it keeps crashing. */
  #ended(child: ChildProcessWithoutNullStreams, reason: string): void {
    if (child !== this.#child) return;
    this.#connection?.dispose();
    this.#connection = undefined;
    this.#child = undefined;
    if (this.#disposed) return;
    this.log.error(reason);
    const now = Date.now();
    this.#crashes = [...this.#crashes.filter((at) => now - at < CRASH_WINDOW_MS), now];
    if (this.#crashes.length > RESTARTS) {
      this.#set({ kind: "stopped", reason });
      return;
    }
    this.#set({ kind: "starting" });
    setTimeout(() => void this.start(), RESTART_DELAY_MS * this.#crashes.length);
  }

  /** Ends the server by closing its input, and by force if it does not end in time. */
  async #stop(): Promise<void> {
    const child = this.#child;
    const connection = this.#connection;
    this.#child = undefined;
    this.#connection = undefined;
    if (!child) return;
    connection?.dispose();
    const ended = new Promise<void>((resolve) => {
      if (child.exitCode !== null || child.signalCode !== null) resolve();
      else
        child.once("exit", () => {
          resolve();
        });
    });
    child.stdin.end();
    const timer = setTimeout(() => child.kill(), END_TIMEOUT_MS);
    await ended;
    clearTimeout(timer);
  }
}
