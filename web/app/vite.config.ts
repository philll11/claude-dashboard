import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

import { bridge } from "./tests/bridge/plugin.ts";

// The build is one script and one stylesheet with fixed names, because the VS Code extension
// and the desktop app load them by name under a strict Content Security Policy. The fonts the
// stylesheet loads keep names of their own.
export default defineConfig({
  plugins: [react(), bridge()],
  base: "./",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
    rollupOptions: {
      output: {
        entryFileNames: "dashboard.js",
        assetFileNames: ({ names }) =>
          names.some((name) => name.endsWith(".css")) ? "dashboard.[ext]" : "fonts/[name][extname]",
        codeSplitting: false,
      },
    },
  },
  test: {
    include: ["src/**/*.test.ts"],
  },
});
