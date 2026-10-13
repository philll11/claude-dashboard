// What a person does with the dashboard, from the keyboard as well as the pointer. The tests that
// change something each have a scenario of their own, so that no other test sees the change.

import { resolve } from "node:path";

import { expect, test } from "./fixtures.ts";

test("the sections and the date range work from the keyboard", async ({ dashboard, page }) => {
  await dashboard.open();
  const nav = page.getByRole("navigation", { name: "Dashboard sections" });
  await nav.getByRole("link", { name: "Overview" }).focus();
  await page.keyboard.press("Tab");
  await expect(nav.getByRole("link", { name: "Sessions" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(nav.getByRole("link", { name: "Sessions" })).toHaveAttribute("aria-current", "page");

  await nav.getByRole("link", { name: "Overview" }).click();
  await page
    .getByRole("group", { name: "Date range" })
    .getByRole("button", { name: "7 days" })
    .click();
  await expect(page.getByText("Estimate at Anthropic's list prices · last 7 days")).toBeVisible();
  await expect(page.getByRole("button", { name: "7 days" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
});

test("a filter menu opens, moves and chooses from the keyboard", async ({ dashboard, page }) => {
  await dashboard.open();
  const models = page.getByRole("button", { name: "Model: All models" });
  await models.focus();
  await page.keyboard.press("ArrowDown");
  const menu = page.getByRole("menu", { name: "Model" });
  await expect(menu).toBeVisible();
  await expect(menu.getByRole("menuitemradio", { name: "All models" })).toBeFocused();
  await expect(menu.getByRole("menuitemradio", { name: "Opus 5.5" })).toBeAttached();
  await page.keyboard.press("o");
  await expect(menu.getByRole("menuitemradio", { name: "Opus 5.5" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(menu).toBeHidden();
  await expect(page.getByRole("button", { name: "Model: Opus 5.5" })).toBeFocused();
  await expect(page.getByRole("heading", { name: "API-equivalent cost per day" })).toBeVisible();
  await expect(page.getByRole("list", { name: "Legend" })).toBeHidden();

  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menu", { name: "Model" })).toBeHidden();
  await expect(page.getByRole("button", { name: "Model: Opus 5.5" })).toBeFocused();
});

test("the cost chart reads out each day from the keyboard", async ({ dashboard, page }) => {
  await dashboard.open();
  const chart = page.getByRole("group", { name: /API-equivalent cost per day by model/ });
  await chart.focus();
  await expect(page.locator(".tooltip")).toContainText("Sat 10 Oct");
  await page.keyboard.press("ArrowLeft");
  await expect(page.locator(".tooltip")).toContainText("Fri 9 Oct");
  await page.keyboard.press("Home");
  await expect(page.locator(".tooltip")).toContainText("Fri 11 Sep");
});

test("every chart has a table", async ({ dashboard, page }) => {
  await dashboard.open();
  await page.getByRole("button", { name: "Show as table" }).click();
  const table = page.getByRole("table").first();
  await expect(table.getByRole("rowheader", { name: "Sat 10 Oct" })).toBeVisible();
  await page.getByRole("button", { name: "Show as chart" }).click();
  await expect(
    page.getByRole("group", { name: /API-equivalent cost per day by model/ }),
  ).toBeVisible();

  await dashboard.section("Models");
  await page.getByRole("button", { name: "Show as table" }).click();
  await expect(page.getByRole("rowheader", { name: "Sat 10 Oct" }).first()).toBeVisible();

  await dashboard.section("Limits");
  const show = page.getByRole("button", { name: "Show as table" });
  await expect(show).toHaveCount(2);
  await show.first().click();
  await show.first().click();
  await expect(page.getByRole("columnheader", { name: "Reading" })).toHaveCount(2);
  await expect(page.getByRole("rowheader", { name: "Today 12:13" })).toHaveCount(2);
  await dashboard.check("limits-as-tables");

  await dashboard.section("Sessions");
  await page.getByRole("link", { name: /Price table for Claude 5 models/ }).click();
  await page.getByRole("button", { name: "Show requests as a table" }).click();
  await expect(page.getByRole("button", { name: "Show as chart" })).toBeVisible();
  await expect(page.getByRole("group", { name: /Cumulative cost rises/ })).toBeHidden();
});

test("sessions can be searched, and one opened and left", async ({ dashboard, page }) => {
  await dashboard.open();
  await dashboard.section("Sessions");
  await page.getByRole("searchbox", { name: "Search sessions" }).fill("gateway");
  await expect(page.getByText(/sessions? match/)).toBeVisible();
  await expect(page.getByRole("link", { name: /Rate limiter for the gateway/ })).toBeVisible();
  await page.getByRole("link", { name: /Rate limiter for the gateway/ }).click();
  await expect(page.getByText("6% lower, prices may be out of date")).toBeVisible();
  await page
    .getByRole("navigation", { name: "Breadcrumb" })
    .getByRole("link", { name: "Sessions" })
    .click();
  await expect(page.getByRole("searchbox", { name: "Search sessions" })).toHaveValue("gateway");
});

test("a figure that counts other requests is not compared", async ({ dashboard, page }) => {
  await dashboard.open({ scenario: "other-requests" });
  await dashboard.section("Sessions");
  const row = page.getByRole("row", { name: /Rate limiter for the gateway/ });
  await expect(row).toContainText("Not comparable");
  await row.getByRole("link").click();
  await expect(page.getByText("Counts other requests, so not compared")).toBeVisible();
});

test("connecting the status line shows the change first", async ({ dashboard, page }) => {
  await dashboard.open({ scenario: "change-status-line" });
  await page.getByRole("button", { name: "Connect the status line…" }).click();
  const dialog = page.getByRole("dialog", { name: "Connect Claude Code's status line?" });
  await expect(dialog).toContainText(
    "This creates /home/you/.claude/settings.json with one setting:",
  );
  await expect(dialog).toContainText('+ "statusLine"');
  await dashboard.check("dialog-status-line");
  await dialog.getByRole("button", { name: "Make this change" }).click();
  await expect(dialog).toBeHidden();
  await dashboard.section("Limits");
  await expect(page.getByText("Connected", { exact: true })).toBeVisible();
});

test("the usage endpoint turns on only after its consent", async ({ dashboard, page }) => {
  await dashboard.open({ scenario: "change-endpoint" });
  await dashboard.section("Settings");
  await page.getByRole("switch", { name: /Anthropic's usage endpoint/ }).click();
  const dialog = page.getByRole("dialog", { name: "Use Anthropic's usage endpoint?" });
  await expect(dialog).toContainText(
    "Anthropic's terms say Claude Code's login is for Claude Code",
  );
  const turnOn = dialog.getByRole("button", { name: "Turn on" });
  await expect(turnOn).toBeDisabled();
  await dashboard.check("dialog-endpoint");
  await dialog
    .getByRole("checkbox", { name: "I understand this uses an undocumented endpoint" })
    .check();
  await turnOn.click();
  await expect(dialog).toBeHidden();
  await expect(page.getByRole("switch", { name: /Anthropic's usage endpoint/ })).toHaveAttribute(
    "aria-checked",
    "true",
  );
});

test("a data folder can be added and removed", async ({ dashboard, page }) => {
  // A full path on whichever system runs the server; the folder need not exist.
  const folder = resolve("/home/you/wsl/.claude");
  await dashboard.open({ scenario: "change-folders", pickedFolder: folder });
  await dashboard.section("Settings");
  await page.getByRole("button", { name: "Add folder" }).click();
  const added = page.getByRole("listitem").filter({ hasText: folder });
  await expect(added).toContainText("Added by you");
  await added.getByRole("button", { name: "Remove" }).click();
  await expect(added).toBeHidden();
});

test("the first day of the week is shared with the server", async ({ dashboard, page }) => {
  await dashboard.open({ scenario: "change-settings" });
  await dashboard.section("Settings");
  await page.getByLabel("First day of the week").selectOption("sunday");
  await expect(page.getByLabel("First day of the week")).toHaveValue("sunday");
  await dashboard.section("Overview");
  await expect(page.getByText("This week", { exact: true })).toBeVisible();
});
