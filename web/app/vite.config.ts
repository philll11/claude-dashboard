import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// The build is one script and one stylesheet with fixed names, because the VS Code extension
// and the desktop app load them by name under a strict Content Security Policy.
export default defineConfig({
  plugins: [react()],
  base: "./",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
    rollupOptions: {
      output: {
        entryFileNames: "dashboard.js",
        assetFileNames: "dashboard.[ext]",
        inlineDynamicImports: true,
      },
    },
  },
  test: {
    include: ["src/**/*.test.ts"],
  },
});
