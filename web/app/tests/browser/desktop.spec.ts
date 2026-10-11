// The desktop app's own states: an update ready to install with a restart, one installing, and
// updates that cannot be checked.

import { expect, test } from "./fixtures.ts";

test("an update that is ready waits for a restart", async ({ dashboard, page }) => {
  await dashboard.open({ theme: "desktop-dark", update: { kind: "ready", version: "0.2.1" } });
  await dashboard.section("Settings");
  await expect(page.getByText("Version 0.2.1 is ready", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Restart to update" })).toHaveCount(2);
  await dashboard.check("desktop-update-ready");
});

test("an update found at start says it is installing", async ({ dashboard, page }) => {
  await dashboard.open({
    theme: "desktop-light",
    update: { kind: "installing", version: "0.2.1" },
  });
  await expect(page.getByText("Updating to version 0.2.1…")).toBeVisible();
  await dashboard.check("desktop-update-installing");
});

test("updates that cannot be checked say why", async ({ dashboard, page }) => {
  await dashboard.open({
    theme: "desktop-dark",
    update: { kind: "failed", reason: "The update server could not be reached." },
  });
  await dashboard.section("Settings");
  await expect(page.getByText("cannot check for updates now")).toBeVisible();
  await expect(page.getByRole("button", { name: "Check now" })).toBeVisible();
  await dashboard.check("desktop-update-failed");
});
