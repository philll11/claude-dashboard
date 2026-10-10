// Serves the bridge to the dashboard while Vite runs it for a preview or a browser test:
//
//   POST /__bridge/request?scenario=NAME   {"method", "params"} → {"result"} or {"error"}
//   GET  /__bridge/events?scenario=NAME    the server's notifications, as server-sent events
//
// Each scenario gets its own server the first time a page asks for it.

import type { IncomingMessage, ServerResponse } from "node:http";

import type { Plugin } from "vite";

import type { Bridge } from "./server.ts";

async function body(request: IncomingMessage): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of request) chunks.push(chunk as Buffer);
  return Buffer.concat(chunks).toString("utf8");
}

function send(response: ServerResponse, status: number, value: unknown): void {
  response.statusCode = status;
  response.setHeader("Content-Type", "application/json");
  response.end(JSON.stringify(value));
}

export function bridge(): Plugin {
  const running = new Map<string, Promise<Bridge>>();

  const bridgeFor = async (name: string): Promise<Bridge> => {
    let started = running.get(name);
    if (!started) {
      const [{ Bridge }, { SCENARIOS }] = await Promise.all([
        import("./server.ts"),
        import("./scenarios.ts"),
      ]);
      const scenario = SCENARIOS[name];
      if (!scenario) throw new Error(`There is no scenario called ${name}.`);
      started = Bridge.start(scenario);
      running.set(name, started);
    }
    return started;
  };

  const stopAll = async () => {
    const bridges = [...running.values()];
    running.clear();
    await Promise.all(bridges.map(async (started) => (await started).stop()));
  };

  const handle = (
    request: IncomingMessage,
    response: ServerResponse,
    next: (error?: unknown) => void,
  ): void => {
    const url = new URL(request.url ?? "/", "http://localhost");
    if (!url.pathname.startsWith("/__bridge/")) {
      next();
      return;
    }
    const scenario = url.searchParams.get("scenario") ?? "default";
    if (url.pathname === "/__bridge/request" && request.method === "POST") {
      void (async () => {
        try {
          const { method, params } = JSON.parse(await body(request)) as {
            method: string;
            params: unknown;
          };
          const result = await (await bridgeFor(scenario)).request(method, params);
          send(response, 200, { result: result ?? null });
        } catch (error) {
          const { code, message } = error as { code?: unknown; message?: unknown };
          send(response, 200, {
            error: {
              code: typeof code === "number" ? code : -32603,
              message: typeof message === "string" ? message : String(error),
            },
          });
        }
      })();
      return;
    }
    if (url.pathname === "/__bridge/events" && request.method === "GET") {
      response.writeHead(200, {
        "Content-Type": "text/event-stream",
        "Cache-Control": "no-cache",
        Connection: "keep-alive",
      });
      response.write(": connected\n\n");
      void bridgeFor(scenario).then((started) => {
        const stop = started.onNotification((method, params) => {
          response.write(`data: ${JSON.stringify({ method, params })}\n\n`);
        });
        request.on("close", stop);
      }, next);
      return;
    }
    send(response, 404, { error: { code: -32601, message: `No ${url.pathname}` } });
  };

  return {
    name: "claude-dashboard-bridge",
    configureServer(server) {
      server.middlewares.use(handle);
      server.httpServer?.once("close", () => void stopAll());
    },
    configurePreviewServer(server) {
      server.middlewares.use(handle);
      server.httpServer.once("close", () => void stopAll());
    },
  };
}
