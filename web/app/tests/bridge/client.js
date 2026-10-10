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
  window.claudeDashboardHost = {
    surface: settings.surface ?? "vscode",
    backend: {
      async request(method, params) {
        const response = await fetch(`/__bridge/request?scenario=${scenario}`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ method, params }),
        });
        const answer = await response.json();
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
  };
})();
