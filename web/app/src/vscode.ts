// The dashboard in a VS Code webview: requests and notifications travel as messages to and from
// the extension, which passes them on to `claude-dashboard serve`.

import type { Backend, Method, Notifications, Params, Result } from "@claude-dashboard/protocol";

import type { Host } from "./host";

/** What the webview sends the extension. */
export type ToExtension =
  | { type: "ready" }
  | { type: "request"; id: number; method: Method; params: unknown }
  | { type: "reveal"; path: string };

/** What the extension sends the webview. */
export type FromExtension =
  | { type: "response"; id: number; result?: unknown; error?: { code: number; message: string } }
  | { type: "notification"; method: keyof Notifications; params: unknown };

interface VsCodeApi {
  postMessage(message: ToExtension): void;
  getState(): unknown;
  setState(state: unknown): void;
}

declare global {
  // VS Code defines it in every webview with scripts; it can be called once.

  var acquireVsCodeApi: (() => VsCodeApi) | undefined;
}

/** A request the server refused, with the JSON-RPC error code it gave. */
export class RequestError extends Error {
  readonly code: number;

  constructor(message: string, code: number) {
    super(message);
    this.code = code;
  }
}

/** The host VS Code provides, or `undefined` outside a VS Code webview. */
export function vscodeHost(): Host | undefined {
  const api = globalThis.acquireVsCodeApi?.();
  if (!api) return undefined;
  let nextId = 0;
  const pending = new Map<
    number,
    { resolve: (value: unknown) => void; reject: (error: Error) => void }
  >();
  const listeners = new Map<string, Set<(params: unknown) => void>>();
  window.addEventListener("message", (event: MessageEvent<FromExtension>) => {
    const message = event.data;
    if (message.type === "response") {
      const waiting = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) {
        waiting?.reject(new RequestError(message.error.message, message.error.code));
      } else {
        waiting?.resolve(message.result ?? null);
      }
    } else {
      for (const listener of listeners.get(message.method) ?? []) listener(message.params);
    }
  });
  const backend: Backend = {
    request<M extends Method>(method: M, params: Params<M>): Promise<Result<M>> {
      nextId += 1;
      const id = nextId;
      return new Promise((resolve, reject) => {
        pending.set(id, {
          resolve: (value) => {
            resolve(value as Result<M>);
          },
          reject,
        });
        api.postMessage({ type: "request", id, method, params });
      });
    },
    onNotification(method, listener) {
      const set = listeners.get(method) ?? new Set();
      listeners.set(method, set);
      const wrapped = listener as (params: unknown) => void;
      set.add(wrapped);
      return () => set.delete(wrapped);
    },
  };
  api.postMessage({ type: "ready" });
  return {
    surface: "vscode",
    backend,
    memory: {
      read: () => api.getState(),
      write: (state) => {
        api.setState(state);
      },
    },
    reveal: (path) => {
      api.postMessage({ type: "reveal", path });
    },
  };
}
