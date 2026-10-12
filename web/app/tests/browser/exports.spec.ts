// Exports from the dashboard, the desktop app's question when its window first closes, and the
// daily price check, against their screenshots.

import { expect, test } from "./fixtures.ts";

test("a section exports its table through the save dialog", async ({ dashboard, page }) => {
  await dashboard.open({ theme: "desktop-dark", scenario: "exports" });
  await dashboard.section("Sessions");
  await page.getByRole("button", { name: "Export" }).click();
  const menu = page.getByRole("menu", { name: "Export" });
  await expect(menu).toContainText("All");
  await expect(menu).toContainText("sessions that match, not only this page");
  await dashboard.check("desktop-dark-export-menu");
  await menu.getByRole("menuitem", { name: /Export as CSV/ }).click();
  const saved = page.getByRole("status");
  await expect(saved).toContainText(/Saved [\d,]+ sessions/);
  await expect(saved).toContainText(
    "/home/you/Downloads/claude-dashboard-sessions-2026-09-11-to-2026-10-10.csv",
  );
  await dashboard.check("desktop-dark-export-saved");
  await saved.getByRole("button", { name: "Show in folder" }).click();
  expect(await page.evaluate(() => window.claudeDashboardBridge?.revealed)).toBe(
    "/home/you/Downloads/claude-dashboard-sessions-2026-09-11-to-2026-10-10.csv",
  );
  await saved.getByRole("button", { name: "Dismiss" }).click();
  await expect(saved).toBeHidden();
});

test("the export menu works from the keyboard", async ({ dashboard, page }) => {
  await dashboard.open({ theme: "vscode-light", scenario: "exports" });
  const button = page.getByRole("button", { name: "Export" });
  await button.focus();
  await page.keyboard.press("ArrowDown");
  const menu = page.getByRole("menu", { name: "Export" });
  await expect(menu.getByRole("menuitem", { name: /Export as CSV/ })).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(menu.getByRole("menuitem", { name: /Export as JSON/ })).toBeFocused();
  await dashboard.check("vscode-light-export-menu");
  await page.keyboard.press("Escape");
  await expect(menu).toBeHidden();
  await expect(button).toBeFocused();
});

test("the whole history exports with its progress, which can be cancelled", async ({
  dashboard,
  page,
}) => {
  await dashboard.open({ theme: "desktop-dark", scenario: "exports" });
  await dashboard.section("Settings");
  await page.evaluate(() => {
    const bridge = window.claudeDashboardBridge;
    if (bridge) {
      bridge.exportHeld = {
        written: 83_412,
        total: 150_277,
        path: "/home/you/Downloads/claude-dashboard-history-2026-07-31-to-2026-10-10.csv",
      };
    }
  });
  await page.getByRole("button", { name: "Export all history…" }).click();
  await page.getByRole("menuitem", { name: /Export as CSV/ }).click();
  const progress = page.getByRole("progressbar", { name: "Exporting all history" });
  await expect(progress).toContainText(
    "83,412 of 150,277 requests written to claude-dashboard-history-2026-07-31-to-2026-10-10.csv",
  );
  await expect(page.getByRole("button", { name: "Exporting all history…" })).toBeDisabled();
  await dashboard.check("desktop-dark-export-writing");
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(progress).toBeHidden();
  await expect(page.getByRole("button", { name: "Export all history…" })).toBeEnabled();
});

test("the window's first close asks whether to keep running", async ({ dashboard, page }) => {
  await dashboard.open({ theme: "desktop-dark" });
  await page.evaluate(() => {
    window.claudeDashboardBridge?.askClose?.();
  });
  const dialog = page.getByRole("dialog", { name: "Dashboard for Claude Code keeps running" });
  await expect(dialog).toContainText("Settings can change this later.");
  // Enter keeps the app running, rather than quitting it.
  await expect(dialog.getByRole("button", { name: "Keep running" })).toBeFocused();
  await dashboard.check("desktop-dark-keep-running");
  await page.keyboard.press("Enter");
  await expect(dialog).toBeHidden();
  expect(await page.evaluate(() => window.claudeDashboardBridge?.closeChoice)).toBe(true);
});

test("the daily price check says when it ran and what came of it", async ({ dashboard, page }) => {
  await dashboard.open({ theme: "desktop-light", scenario: "prices-refreshed" });
  await dashboard.section("Settings");
  await expect(page.getByText("downloaded today from this project's repository")).toBeVisible();
  await expect(page.getByText("Last checked today at 08:00: up to date.")).toBeVisible();
  await expect(
    page.getByRole("switch", { name: "Check for a newer price table once a day" }),
  ).toHaveAttribute("aria-checked", "true");
  await dashboard.check("desktop-light-prices-refreshed");
});
