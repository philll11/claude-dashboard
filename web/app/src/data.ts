// The server's answers, kept by request and refreshed in place when the history or the plan limits
// change, so that numbers update without blanking or jumping.

import { createContext, useCallback, useContext, useSyncExternalStore } from "react";

import type { Backend, Method, Params, Progress, Result } from "@claude-dashboard/protocol";

export interface Answer<T> {
  /** The latest answer; kept while a newer one is asked for. */
  data: T | undefined;
  error: Error | undefined;
  /** Set while the first answer, or a newer one, is on its way. */
  loading: boolean;
}

interface Entry {
  method: Method;
  params: unknown;
  answer: Answer<unknown>;
  listeners: Set<() => void>;
  /** Set while the entry's answer is out of date. */
  stale: boolean;
  /** Set while a request for the entry is on its way. */
  asking: boolean;
}

const NOTHING: Answer<never> = { data: undefined, error: undefined, loading: true };

/** Requests whose answers depend on the plan limits rather than the requests in the history. */
const LIMITS: readonly Method[] = ["status", "limits"];

/** Requests whose answers depend on the settings every surface shares. */
const SETTINGS: readonly Method[] = ["settings", "prices"];

/** How many answers nothing shows are kept, for going back to a view. */
const KEEP = 40;

export class Store {
  private readonly entries = new Map<string, Entry>();
  private readonly progressListeners = new Set<() => void>();
  private progress: Progress | null = null;

  constructor(private readonly backend: Backend) {
    backend.onNotification("history/changed", () => {
      this.progress = null;
      this.notifyProgress();
      this.refresh(() => true);
    });
    backend.onNotification("limits/changed", () => {
      this.refresh((entry) => LIMITS.includes(entry.method));
    });
    backend.onNotification("settings/changed", () => {
      this.refresh((entry) => SETTINGS.includes(entry.method));
    });
    backend.onNotification("ingest/progress", (progress) => {
      const reading = progress.done < progress.total;
      this.progress = reading ? progress : null;
      this.notifyProgress();
      // A read that ended changes when the history was last read.
      if (!reading) this.refresh((entry) => entry.method === "status");
    });
  }

  answer(method: Method, params: unknown): Answer<unknown> {
    return this.entries.get(key(method, params))?.answer ?? NOTHING;
  }

  subscribe(method: Method, params: unknown, listener: () => void): () => void {
    const entry = this.entry(method, params);
    entry.listeners.add(listener);
    if (entry.answer === NOTHING || entry.stale) this.ask(entry);
    return () => {
      entry.listeners.delete(listener);
      this.forget();
    };
  }

  /** Asks again for every answer `which` picks. */
  refresh(which: (entry: Entry) => boolean): void {
    for (const entry of this.entries.values()) {
      if (!which(entry)) continue;
      entry.stale = true;
      if (entry.listeners.size > 0) this.ask(entry);
    }
  }

  /** Sends a request whose answer is not kept, such as a change to a setting. */
  async send<M extends Method>(method: M, params: Params<M>): Promise<Result<M>> {
    return this.backend.request(method, params);
  }

  readingProgress(): Progress | null {
    return this.progress;
  }

  subscribeProgress(listener: () => void): () => void {
    this.progressListeners.add(listener);
    return () => this.progressListeners.delete(listener);
  }

  private notifyProgress(): void {
    for (const listener of this.progressListeners) listener();
  }

  private entry(method: Method, params: unknown): Entry {
    const k = key(method, params);
    let entry = this.entries.get(k);
    if (!entry) {
      entry = {
        method,
        params,
        answer: NOTHING,
        listeners: new Set(),
        stale: false,
        asking: false,
      };
      this.entries.set(k, entry);
    }
    return entry;
  }

  private ask(entry: Entry): void {
    if (entry.asking) return;
    entry.asking = true;
    entry.stale = false;
    if (!entry.answer.loading) this.update(entry, { ...entry.answer, loading: true });
    this.backend.request(entry.method, entry.params as never).then(
      (data: unknown) => {
        entry.asking = false;
        this.update(entry, { data, error: undefined, loading: entry.stale });
        if (entry.stale && entry.listeners.size > 0) this.ask(entry);
      },
      (error: unknown) => {
        entry.asking = false;
        this.update(entry, {
          data: entry.answer.data,
          error: error instanceof Error ? error : new Error(String(error)),
          loading: false,
        });
      },
    );
  }

  private update(entry: Entry, answer: Answer<unknown>): void {
    entry.answer = answer;
    for (const listener of entry.listeners) listener();
  }

  /** Drops the oldest answers that nothing shows. */
  private forget(): void {
    const unseen = [...this.entries].filter(([, entry]) => entry.listeners.size === 0);
    for (const [k] of unseen.slice(0, Math.max(0, unseen.length - KEEP))) this.entries.delete(k);
  }
}

function key(method: Method, params: unknown): string {
  return JSON.stringify([method, params ?? null]);
}

export const StoreContext = createContext<Store | null>(null);

export function useStore(): Store {
  const store = useContext(StoreContext);
  if (!store) throw new Error("useStore needs a StoreContext");
  return store;
}

/** The server's answer to `method` with `params`, asked for while the component shows and
 * `wanted` holds; nothing while it does not. */
export function useRequest<M extends Method>(
  method: M,
  params: Params<M>,
  wanted = true,
): Answer<Result<M>> {
  const store = useStore();
  const k = key(method, params);
  /* eslint-disable react-hooks/exhaustive-deps -- the key stands for the method and parameters,
     so that a new object with the same content is the same request */
  const subscribe = useCallback(
    (listener: () => void) =>
      wanted ? store.subscribe(method, params, listener) : () => undefined,
    [store, k, wanted],
  );
  const snapshot = useCallback(
    () => (wanted ? store.answer(method, params) : NOTHING),
    [store, k, wanted],
  );
  /* eslint-enable react-hooks/exhaustive-deps */
  return useSyncExternalStore(subscribe, snapshot) as Answer<Result<M>>;
}

/** How far a read of new transcript content has got, while one is under way. */
export function useReadingProgress(): Progress | null {
  const store = useStore();
  return useSyncExternalStore(
    (listener) => store.subscribeProgress(listener),
    () => store.readingProgress(),
  );
}
