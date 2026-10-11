// Starts the server of every scenario before the tests, so that no test waits for one to read
// its synthetic Claude Code folder.

import { SCENARIOS } from "../bridge/scenarios.ts";

export default async function warm(): Promise<void> {
  await Promise.all(
    Object.keys(SCENARIOS).map(async (scenario) => {
      const response = await fetch(
        `http://127.0.0.1:4173/__bridge/request?scenario=${encodeURIComponent(scenario)}`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ method: "status", params: null }),
        },
      );
      const answer = (await response.json()) as { error?: { message: string } };
      if (answer.error) throw new Error(`${scenario}: ${answer.error.message}`);
    }),
  );
}
