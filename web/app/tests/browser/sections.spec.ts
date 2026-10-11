// Every section in VS Code's Dark Modern, Light Modern and High Contrast themes, and in the desktop
// app's dark and light themes, against its screenshot and an accessibility check.

import { expect, test } from "./fixtures.ts";

const THEMES = [
  "vscode-dark",
  "vscode-light",
  "vscode-high-contrast",
  "desktop-dark",
  "desktop-light",
] as const;

for (const theme of THEMES) {
  test.describe(theme, () => {
    test("overview", async ({ dashboard, page }) => {
      await dashboard.open({ theme });
      await expect(
        page.getByRole("heading", { name: "API-equivalent cost per day" }),
      ).toBeVisible();
      await expect(page.getByText("Resets 14:20, in 2 h 05 min")).toBeVisible();
      await dashboard.check(`${theme}-overview`);
    });

    test("sessions", async ({ dashboard, page }) => {
      await dashboard.open({ theme });
      await dashboard.section("Sessions");
      await expect(page.getByText("Showing 1 to 12 of")).toBeVisible();
      await dashboard.check(`${theme}-sessions`);
    });

    test("session detail", async ({ dashboard, page }) => {
      await dashboard.open({ theme });
      await dashboard.section("Sessions");
      await page.getByRole("link", { name: /Price table for Claude 5 models/ }).click();
      await expect(page.getByRole("heading", { name: "Models and subagents" })).toBeVisible();
      await expect(page.getByText("Explore subagent", { exact: true }).first()).toBeVisible();
      await dashboard.check(`${theme}-session`);
    });

    test("projects", async ({ dashboard, page }) => {
      await dashboard.open({ theme });
      await dashboard.section("Projects");
      await expect(page.getByText("Other projects", { exact: true })).toBeVisible();
      await dashboard.check(`${theme}-projects`);
    });

    test("models", async ({ dashboard, page }) => {
      await dashboard.open({ theme });
      await dashboard.section("Models");
      await expect(page.getByText("Haiku 5.5, longer prompts")).toBeVisible();
      await dashboard.check(`${theme}-models`);
    });

    test("limits", async ({ dashboard, page }) => {
      await dashboard.open({ theme });
      await dashboard.section("Limits");
      await expect(page.getByRole("heading", { name: "Recent five-hour windows" })).toBeVisible();
      await dashboard.check(`${theme}-limits`);
    });

    test("settings", async ({ dashboard, page }) => {
      await dashboard.open({ theme });
      await dashboard.section("Settings");
      await expect(page.getByText("Within 0.4%", { exact: false })).toBeVisible();
      await dashboard.check(`${theme}-settings`);
    });
  });
}
