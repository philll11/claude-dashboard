// Gives the page a dashboard host that reaches the bridge's server over HTTP. A browser test adds
// it as an init script after setting `window.claudeDashboardBridge`; a preview imports it.
(() => {
  const settings = (window.claudeDashboardBridge ??= {});
  const scenario = encodeURIComponent(settings.scenario ?? "default");
  const listeners = new Map();
  const events = new EventSource(`/__bridge/events?scenario=${scenario}`);
  events.onmessage = (event) => {
    const { method, params } = JSON.parse(event.data);
    for (const listener of listeners.get(method) ?? []) listener(params);
  };
  let state = settings.state;
  // How many requests are on their way, for a test to wait until the dashboard has settled.
  settings.pending = 0;
  window.claudeDashboardHost = {
    surface: settings.surface ?? "vscode",
    backend: {
      async request(method, params) {
        settings.pending += 1;
        let answer;
        try {
          const response = await fetch(`/__bridge/request?scenario=${scenario}`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ method, params }),
          });
          answer = await response.json();
        } finally {
          settings.pending -= 1;
        }
        if (answer.error) {
          throw Object.assign(new Error(answer.error.message), { code: answer.error.code });
        }
        return answer.result;
      },
      onNotification(method, listener) {
        const set = listeners.get(method) ?? new Set();
        listeners.set(method, set);
        set.add(listener);
        return () => set.delete(listener);
      },
    },
    memory: {
      read: () => state,
      write: (next) => {
        state = next;
      },
    },
    reveal: (path) => {
      settings.revealed = path;
    },
    pickFolder: () => Promise.resolve(settings.pickedFolder),
    async exportTo(params) {
      if (settings.exportHeld) {
        // An export that stays under way until the test lets it go, at the progress it sets.
        for (const listener of listeners.get("export/progress") ?? []) {
          listener(settings.exportHeld);
        }
        await new Promise((resolve, reject) => {
          settings.releaseExport = resolve;
          settings.cancelHeld = () => {
            reject(Object.assign(new Error("The export was cancelled."), { code: -32800 }));
          };
        });
        return undefined;
      }
      const response = await fetch(`/__bridge/export?scenario=${scenario}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ params }),
      });
      const answer = await response.json();
      if (answer.error) {
        throw Object.assign(new Error(answer.error.message), { code: answer.error.code });
      }
      return answer.result ?? undefined;
    },
    cancelExport() {
      if (settings.cancelHeld) {
        settings.cancelHeld();
        return;
      }
      void fetch(`/__bridge/cancel?scenario=${scenario}`, { method: "POST" });
    },
  };
  if (window.claudeDashboardHost.surface === "desktop") {
    // The desktop app's item in the notification area, and its question when the window first
    // closes, which a test asks with `askClose`.
    const closeAsked = new Set();
    settings.askClose = () => {
      for (const listener of closeAsked) listener();
    };
    window.claudeDashboardHost.desktop = {
      tray: true,
      trayPlace: "notification area",
      autostart: () => Promise.resolve(settings.autostart ?? false),
      setAutostart: (on) => {
        settings.autostart = on;
        return Promise.resolve();
      },
      onCloseAsked: (listener) => {
        closeAsked.add(listener);
        return () => closeAsked.delete(listener);
      },
      closeChoice: (keepRunning) => {
        settings.closeChoice = keepRunning;
      },
    };
    // The desktop app's updates, standing where the test puts them.
    const update = settings.update ?? { kind: "current" };
    let automatic = true;
    window.claudeDashboardHost.updates = {
      state: () => update,
      subscribe: () => () => undefined,
      automatic: () => automatic,
      setAutomatic: (on) => {
        automatic = on;
      },
      check: () => undefined,
      install: () => undefined,
    };
  }
})();
