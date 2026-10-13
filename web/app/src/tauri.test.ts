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

  it("tells the app once the dashboard has drawn, which shows the window", async () => {
    const tauri = fakeTauri();
    const host = await desktopHost(tauri.api);
    expect(tauri.api.invoke).not.toHaveBeenCalledWith("ready");
    host.ready?.();
    expect(tauri.api.invoke).toHaveBeenCalledWith("ready");
  });

  it("exports through the app's own save dialog", async () => {
    const saved = { rows: 214, what: "214 sessions", path: "C:\\Users\\you\\Downloads\\s.csv" };
    const tauri = fakeTauri({ export: saved });
    const host = await desktopHost(tauri.api);
    const params = { dataset: "sessions", format: "csv", filters: {} } as const;
    await expect(host.exportTo?.(params)).resolves.toEqual(saved);
    expect(tauri.api.invoke).toHaveBeenCalledWith("export", { params });
    host.cancelExport?.();
    expect(tauri.api.invoke).toHaveBeenCalledWith("export_cancel");
  });

  it("knows a closed save dialog from a failed export", async () => {
    const closed = await desktopHost(fakeTauri().api);
    await expect(
      closed.exportTo?.({ dataset: "days", format: "json", filters: {} }),
    ).resolves.toBeUndefined();
    const failing = await desktopHost(
      fakeTauri({ export: { code: -32800, message: "The export was cancelled." } }).api,
    );
    await expect(
      failing.exportTo?.({ dataset: "days", format: "json", filters: {} }),
    ).rejects.toMatchObject({ code: -32800 });
  });

  it("passes the first close's question to the dashboard and its answer back", async () => {
    const tauri = fakeTauri({
      desktop: { tray: true, trayPlace: "notification area", hidden: false },
    });
    const host = await desktopHost(tauri.api);
    expect(host.desktop?.tray).toBe(true);
    const asked = vi.fn();
    host.desktop?.onCloseAsked(asked);
    tauri.emit("closeAsked", null);
    expect(asked).toHaveBeenCalled();
    host.desktop?.closeChoice(true);
    expect(tauri.api.invoke).toHaveBeenCalledWith("close_choice", { keepRunning: true });
  });

  it("offers an update that waits in the app's item and installs it from there", async () => {
    const newer = {
      version: "0.4.1",
      download: vi.fn(() => Promise.resolve()),
      install: vi.fn(() => Promise.resolve()),
      downloadAndInstall: vi.fn(() => Promise.resolve()),
    };
    const tauri = fakeTauri({
      desktop: { tray: true, trayPlace: "notification area", hidden: true },
    });
    tauri.store.set(AUTOMATIC_KEY, "on");
    tauri.api.check.mockImplementation(() => Promise.resolve(newer as never));
    await desktopHost(tauri.api);
    await vi.waitFor(() => {
      expect(tauri.api.invoke).toHaveBeenCalledWith("update_waiting", { version: "0.4.1" });
    });
    expect(newer.downloadAndInstall).not.toHaveBeenCalled();
    tauri.emit("installUpdate", null);
    await vi.waitFor(() => {
      expect(tauri.api.relaunch).toHaveBeenCalled();
    });
    expect(newer.install).toHaveBeenCalled();
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
