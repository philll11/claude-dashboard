// Starts the server of every scenario before the tests, so that no test waits for one to read
// its synthetic Claude Code folder.

import { SCENARIOS } from "../bridge/scenarios.ts";

export default async function warm(): Promise<void> {
  await Promise.all(
    Object.entries(SCENARIOS).map(async ([scenario, { unreadable }]) => {
      const response = await fetch(
        `http://127.0.0.1:4173/__bridge/request?scenario=${encodeURIComponent(scenario)}`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ method: "status", params: null }),
        },
      );
      const text = await response.text();
      let answer: { error?: { message: string } };
      try {
        answer = JSON.parse(text) as typeof answer;
      } catch {
        throw new Error(
          `${scenario}: the bridge answered ${String(response.status)} with ${text ? `"${text.slice(0, 200)}"` : "nothing"}`,
        );
      }
      if (answer.error && !unreadable) throw new Error(`${scenario}: ${answer.error.message}`);
    }),
  );
}
