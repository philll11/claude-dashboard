// The states the design asks for: a first read, no history, no plan limits, old plan limits, an
// expired login, a folder that cannot be read, a read-only history, an import on offer, prices
// that may be out of date, and new activity being read.

import { expect, test } from "./fixtures.ts";

test("a first read shows its progress and what is ready", async ({ dashboard, page }) => {
  await dashboard.open({ scenario: "first-run" });
  await expect(
    page.getByRole("progressbar", { name: "Reading your Claude Code history" }),
  ).toHaveAttribute("aria-valuenow", "41");
  await expect(page.getByText("Reading your history…")).toBeVisible();
  await dashboard.check("state-first-run");
});

test("no history says where it looked", async ({ dashboard, page }) => {
  await dashboard.open({ scenario: "empty" });
  await expect(page.getByRole("heading", { name: "No Claude Code history found" })).toBeVisible();
  await expect(page.getByText("/home/you/.claude", { exact: true })).toBeVisible();
  await dashboard.check("state-empty");
});

test("without plan limits, connecting the status line is offered", async ({ dashboard, page }) => {
  await dashboard.open({ scenario: "no-limits" });
  await expect(page.getByRole("button", { name: "Connect the status line…" })).toBeVisible();
  await dashboard.check("state-no-limits");
});

test("old plan limits say how old they are and why", async ({ dashboard, page }) => {
  await dashboard.open({ scenario: "stale-limits" });
  await expect(page.getByText(/hasn't run in a terminal since/)).toBeVisible();
  await dashboard.check("state-stale-limits");
});

test("prices that drift from Claude Code's own figures are flagged", async ({
  dashboard,
  page,
}) => {
  await dashboard.open({ scenario: "prices-drift" });
  await expect(
    page.getByText("Our estimate is 6% lower than Claude Code's own figure"),
  ).toBeVisible();
  await dashboard.check("state-prices-drift");
});

test("new activity being read keeps the numbers in place", async ({ dashboard, page }) => {
  await dashboard.open({ scenario: "reading" });
  await expect(page.getByText("Reading 2 changed files…")).toBeVisible();
  await expect(page.getByRole("heading", { name: "API-equivalent cost per day" })).toBeVisible();
});

test("an expired login for the usage endpoint says what to do", async ({ dashboard, page }) => {
  await dashboard.open({ scenario: "login-expired" });
  await expect(page.getByText(/Open Claude Code once to sign in again/)).toBeVisible();
  await dashboard.check("state-login-expired");
});

test("a folder that cannot be read is named, with the reason", async ({ dashboard, page }) => {
  await dashboard.open({ scenario: "unreadable-folder" });
  await expect(page.getByRole("alert")).toContainText(
    "Couldn't read //wsl.localhost/Ubuntu/home/you/.claude",
  );
  await dashboard.check("state-unreadable-folder");
});

test("a history a newer version upgraded is read-only", async ({ dashboard, page }) => {
  await dashboard.open({ scenario: "newer-schema" });
  await expect(page.getByText("Read-only until you update")).toBeVisible();
  await dashboard.check("state-newer-schema");
});

test("the claude-usage dashboard's history is offered for import", async ({ dashboard, page }) => {
  await dashboard.open({ scenario: "claude-usage" });
  const offer = page.getByRole("region", { name: "History from the claude-usage dashboard" });
  await expect(offer).toContainText("from 1 July to 30 July");
  await dashboard.check("state-claude-usage");
});
