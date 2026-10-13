import { defineConfig, devices } from "@playwright/test";

// The built dashboard, served by Vite's preview with the bridge to the real server, in Chromium:
// the engine of VS Code's webviews and of the desktop app on Windows. WebKit, the engine of the
// desktop app on macOS and Linux, checks every section in the desktop app's themes. Screenshots are
// compared with baselines made in the Playwright container that CI also runs in, so that fonts and
// rendering match.
export default defineConfig({
  testDir: "tests/browser",
  globalSetup: "./tests/browser/warm.ts",
  timeout: 60_000,
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  expect: {
    timeout: 15_000,
    toHaveScreenshot: { animations: "disabled", caret: "hide" },
  },
  use: {
    baseURL: "http://127.0.0.1:4173",
    viewport: { width: 1280, height: 900 },
    locale: "en-GB",
    timezoneId: "Pacific/Auckland",
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 900 } },
    },
    {
      name: "webkit",
      testMatch: "sections.spec.ts",
      use: { ...devices["Desktop Safari"], viewport: { width: 1280, height: 900 } },
    },
  ],
  webServer: {
    command: "node node_modules/vite/bin/vite.js preview --host 127.0.0.1 --port 4173 --strictPort",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: !process.env.CI,
  },
});
