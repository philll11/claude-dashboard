// The dashboard in a VS Code webview: requests and notifications travel as messages to and from
// the extension, which passes them on to `claude-dashboard serve`.

import type {
  Backend,
  FromExtension,
  Method,
  Params,
  Result,
  Show,
  ToExtension,
} from "@claude-dashboard/protocol";

import { type Host, RequestError } from "./host";

interface VsCodeApi {
  postMessage(message: ToExtension): void;
  getState(): unknown;
  setState(state: unknown): void;
}

declare global {
  // VS Code defines it in every webview with scripts; it can be called once.

  var acquireVsCodeApi: (() => VsCodeApi) | undefined;
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
  const picking = new Map<number, (path: string | undefined) => void>();
  const showing = new Set<(show: Show) => void>();
  // A section the extension asks for before the dashboard listens is shown once it does.
  let waitingShow: Show | undefined;
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
    } else if (message.type === "picked") {
      picking.get(message.id)?.(message.path);
      picking.delete(message.id);
    } else if (message.type === "show") {
      if (showing.size === 0) waitingShow = message.show;
      for (const listener of showing) listener(message.show);
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
    pickFolder: () => {
      nextId += 1;
      const id = nextId;
      return new Promise((resolve) => {
        picking.set(id, resolve);
        api.postMessage({ type: "pickFolder", id });
      });
    },
    onShow: (listener) => {
      showing.add(listener);
      if (waitingShow) listener(waitingShow);
      waitingShow = undefined;
      return () => {
        showing.delete(listener);
      };
    },
  };
}
