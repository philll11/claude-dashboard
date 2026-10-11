import { defineConfig } from "vitest/config";

// The unit tests; the integration and end-to-end tests run in VS Code instead.
export default defineConfig({
  test: {
    include: ["src/**/*.test.ts"],
  },
});
