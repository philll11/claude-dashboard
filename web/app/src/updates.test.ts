import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  AUTOMATIC_KEY,
  CHECK_EVERY_MS,
  createUpdates,
  RETRY_AFTER_MS,
  type Update,
  type UpdateApi,
} from "./updates";

function update(version: string): Update {
  return {
    version,
    download: vi.fn(() => Promise.resolve()),
    install: vi.fn(() => Promise.resolve()),
    downloadAndInstall: vi.fn(() => Promise.resolve()),
  };
}

function storage(entries: Record<string, string> = {}) {
  const map = new Map(Object.entries(entries));
  return {
    getItem: (key: string) => map.get(key) ?? null,
    setItem: (key: string, value: string) => {
      map.set(key, value);
    },
    map,
  };
}

function api(answers: (Update | null | Error)[], store = storage()) {
  const check = vi.fn(() => {
    const answer = answers.shift() ?? null;
    return answer instanceof Error ? Promise.reject(answer) : Promise.resolve(answer);
  });
  const relaunch = vi.fn(() => Promise.resolve());
  return { check, relaunch, storage: store } satisfies UpdateApi;
}

describe("updates", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("installs an update found at start and restarts", async () => {
    const newer = update("0.2.1");
    const fake = api([newer]);
    const updates = createUpdates(fake);
    expect(updates.state()).toEqual({ kind: "checking" });
    await vi.waitFor(() => {
      expect(fake.relaunch).toHaveBeenCalled();
    });
    expect(newer.downloadAndInstall).toHaveBeenCalled();
    expect(updates.state()).toEqual({ kind: "installing", version: "0.2.1" });
  });

  it("downloads an update found later and waits for a restart", async () => {
    const newer = update("0.2.1");
    const fake = api([null, newer]);
    const updates = createUpdates(fake);
    await vi.waitFor(() => {
      expect(updates.state()).toEqual({ kind: "current" });
    });
    await vi.advanceTimersByTimeAsync(CHECK_EVERY_MS);
    await vi.waitFor(() => {
      expect(updates.state()).toEqual({ kind: "ready", version: "0.2.1" });
    });
    expect(newer.download).toHaveBeenCalled();
    expect(newer.install).not.toHaveBeenCalled();
    expect(fake.relaunch).not.toHaveBeenCalled();

    updates.install();
    expect(updates.state()).toEqual({ kind: "installing", version: "0.2.1" });
    await vi.waitFor(() => {
      expect(fake.relaunch).toHaveBeenCalled();
    });
    expect(newer.install).toHaveBeenCalled();
  });

  it("checks nothing while updates are not automatic, until asked", async () => {
    const fake = api([null], storage({ [AUTOMATIC_KEY]: "off" }));
    const updates = createUpdates(fake);
    expect(updates.automatic()).toBe(false);
    expect(updates.state()).toEqual({ kind: "idle" });
    await vi.advanceTimersByTimeAsync(CHECK_EVERY_MS * 2);
    expect(fake.check).not.toHaveBeenCalled();

    updates.check();
    await vi.waitFor(() => {
      expect(updates.state()).toEqual({ kind: "current" });
    });
    expect(fake.check).toHaveBeenCalledTimes(1);
  });

  it("starts with the choice it is given until the person makes one", async () => {
    const quiet = api([null]);
    const updates = createUpdates({ ...quiet, automaticAtFirst: false });
    expect(updates.automatic()).toBe(false);
    await vi.advanceTimersByTimeAsync(CHECK_EVERY_MS);
    expect(quiet.check).not.toHaveBeenCalled();

    const chosen = api([null], storage({ [AUTOMATIC_KEY]: "on" }));
    expect(createUpdates({ ...chosen, automaticAtFirst: false }).automatic()).toBe(true);
    await vi.waitFor(() => {
      expect(chosen.check).toHaveBeenCalledTimes(1);
    });
  });

  it("says why a check failed", async () => {
    const updates = createUpdates(api([new Error("The update server could not be reached.")]));
    await vi.waitFor(() => {
      expect(updates.state()).toEqual({
        kind: "failed",
        reason: "The update server could not be reached.",
      });
    });
  });

  it("checks again an hour after a check that failed", async () => {
    const newer = update("0.2.1");
    const fake = api([new Error("The update server could not be reached."), newer]);
    const updates = createUpdates(fake);
    await vi.waitFor(() => {
      expect(updates.state().kind).toBe("failed");
    });
    await vi.advanceTimersByTimeAsync(RETRY_AFTER_MS);
    await vi.waitFor(() => {
      expect(updates.state()).toEqual({ kind: "ready", version: "0.2.1" });
    });
    expect(fake.relaunch).not.toHaveBeenCalled();
  });

  it("checks no more after a failure once updates are no longer automatic", async () => {
    const fake = api([new Error("The update server could not be reached."), update("0.2.1")]);
    const updates = createUpdates(fake);
    await vi.waitFor(() => {
      expect(updates.state().kind).toBe("failed");
    });
    updates.setAutomatic(false);
    await vi.advanceTimersByTimeAsync(RETRY_AFTER_MS);
    expect(fake.check).toHaveBeenCalledTimes(1);
  });

  it("keeps the choice, and checks once updates become automatic", async () => {
    const store = storage({ [AUTOMATIC_KEY]: "off" });
    const fake = api([null], store);
    const updates = createUpdates(fake);
    const changed = vi.fn();
    updates.subscribe(changed);

    updates.setAutomatic(true);
    expect(store.map.get(AUTOMATIC_KEY)).toBe("on");
    expect(updates.automatic()).toBe(true);
    expect(changed).toHaveBeenCalled();
    await vi.waitFor(() => {
      expect(updates.state()).toEqual({ kind: "current" });
    });

    updates.setAutomatic(false);
    expect(store.map.get(AUTOMATIC_KEY)).toBe("off");
    await vi.advanceTimersByTimeAsync(CHECK_EVERY_MS * 2);
    expect(fake.check).toHaveBeenCalledTimes(1);
  });
});
