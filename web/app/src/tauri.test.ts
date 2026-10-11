import { describe, expect, it, vi } from "vitest";

import { RequestError } from "./host";
import { AUTOMATIC_KEY } from "./updates";
import { type DesktopApi, desktopHost, VIEW_KEY } from "./tauri";

type Handler = (payload: unknown) => void;

/** A stand-in for Tauri, which records what the host asks of it. */
function fakeTauri(answers: Record<string, unknown> = {}) {
  const events = new Map<string, Handler>();
  let themeChanged: ((theme: "light" | "dark") => void) | undefined;
  const store = new Map<string, string>([[AUTOMATIC_KEY, "off"]]);
  const api = {
    invoke: vi.fn((command: string, args?: Record<string, unknown>) => {
      const answer = command === "request" ? answers[String(args?.method)] : answers[command];
      return answer instanceof Error || (typeof answer === "object" && answer && "code" in answer)
        ? // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- Tauri rejects with a command's error as the command gives it, a plain object
          Promise.reject(answer)
        : Promise.resolve(answer ?? null);
    }),
    listen: vi.fn((event: string, handler: Handler) => {
      events.set(event, handler);
      return Promise.resolve(() => {
        events.delete(event);
      });
    }),
    theme: vi.fn(() => Promise.resolve<"light" | "dark" | null>("light")),
    onThemeChanged: vi.fn((handler: (theme: "light" | "dark") => void) => {
      themeChanged = handler;
      return Promise.resolve(() => undefined);
    }),
    show: vi.fn(() => Promise.resolve()),
    pickFolder: vi.fn(() => Promise.resolve<string | undefined>("/home/you/work/.claude")),
    reveal: vi.fn(() => Promise.resolve()),
    applyTheme: vi.fn(),
    check: vi.fn(() => Promise.resolve(null)),
    relaunch: vi.fn(() => Promise.resolve()),
    storage: {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => {
        store.set(key, value);
      },
    },
  } satisfies DesktopApi;
  return {
    api,
    store,
    emit: (event: string, payload: unknown) => events.get(event)?.(payload),
    changeTheme: (theme: "light" | "dark") => themeChanged?.(theme),
  };
}

describe("the desktop host", () => {
  it("asks the server through the request command", async () => {
    const tauri = fakeTauri({ status: { version: "0.2.0" } });
    const host = await desktopHost(tauri.api);
    await expect(host.backend.request("status", null)).resolves.toEqual({ version: "0.2.0" });
    expect(tauri.api.invoke).toHaveBeenCalledWith("request", { method: "status", params: null });
  });

  it("passes on what the server refused, with its code", async () => {
    const tauri = fakeTauri({ session: { code: -32602, message: "No such session" } });
    const host = await desktopHost(tauri.api);
    const refused = host.backend.request("session", { id: "missing" });
    await expect(refused).rejects.toBeInstanceOf(RequestError);
    await expect(refused).rejects.toMatchObject({ code: -32602, message: "No such session" });
  });

  it("hands each notification to the listeners for its method", async () => {
    const tauri = fakeTauri();
    const host = await desktopHost(tauri.api);
    const changed = vi.fn();
    const stop = host.backend.onNotification("limits/changed", changed);
    tauri.emit("notification", { method: "limits/changed", params: null });
    tauri.emit("notification", { method: "history/changed", params: { days: null } });
    expect(changed).toHaveBeenCalledTimes(1);
    stop();
    tauri.emit("notification", { method: "limits/changed", params: null });
    expect(changed).toHaveBeenCalledTimes(1);
  });

  it("follows the system's theme", async () => {
    const tauri = fakeTauri();
    await desktopHost(tauri.api);
    expect(tauri.api.applyTheme).toHaveBeenLastCalledWith("light");
    tauri.changeTheme("dark");
    expect(tauri.api.applyTheme).toHaveBeenLastCalledWith("dark");
  });

  it("shows the section the app was started with once the dashboard listens", async () => {
    const tauri = fakeTauri({ take_show: { section: "limits" } });
    const host = await desktopHost(tauri.api);
    const shown = vi.fn();
    host.onShow?.(shown);
    expect(shown).toHaveBeenCalledWith({ section: "limits" });
    tauri.emit("show", { section: "settings" });
    expect(shown).toHaveBeenLastCalledWith({ section: "settings" });
  });

  it("remembers the view between runs", async () => {
    const tauri = fakeTauri();
    const host = await desktopHost(tauri.api);
    expect(host.memory.read()).toBeUndefined();
    host.memory.write({ section: "models" });
    expect(tauri.store.get(VIEW_KEY)).toBe('{"section":"models"}');
    expect(host.memory.read()).toEqual({ section: "models" });
  });

  it("shows the window once the dashboard has drawn", async () => {
    const tauri = fakeTauri();
    const host = await desktopHost(tauri.api);
    expect(tauri.api.show).not.toHaveBeenCalled();
    host.ready?.();
    expect(tauri.api.show).toHaveBeenCalled();
  });

  it("picks folders and shows files with the system's own dialogs", async () => {
    const tauri = fakeTauri();
    const host = await desktopHost(tauri.api);
    await expect(host.pickFolder?.()).resolves.toBe("/home/you/work/.claude");
    host.reveal?.("/home/you/.local/share/claude-dashboard/history.sqlite");
    expect(tauri.api.reveal).toHaveBeenCalledWith(
      "/home/you/.local/share/claude-dashboard/history.sqlite",
    );
  });
});
