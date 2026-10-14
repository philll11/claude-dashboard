// VS Code's view in the panel or a sidebar: the whole dashboard under the strip, which keeps the
// five-hour and weekly windows and the latest session in this project in sight. Each of VS Code's
// default themes, wide and low as in the panel, beside the terminal and in a sidebar, and each
// state the strip has, as on the panel boards.

import type { Page } from "@playwright/test";

import { Dashboard, expect, PROJECT_FOLDER, test } from "./fixtures.ts";

/** The panel under the editor of a 1440 by 900 window, beside the terminal, and in the secondary
 * sidebar, as the boards draw them. */
const PANEL = { width: 1132, height: 295 };
const BESIDE = { width: 540, height: 295 };
const SIDEBAR = { width: 372, height: 811 };

const THEMES = [
  "vscode-dark",
  "vscode-light",
  "vscode-high-contrast",
  "vscode-high-contrast-light",
] as const;

function strip(page: Page) {
  return page.getByRole("region", { name: "Plan windows and the latest session" });
}

test.describe("in the panel", () => {
  test.use({ viewport: PANEL });

  for (const theme of THEMES) {
    test(`shows the strip above the dashboard in ${theme}`, async ({ dashboard, page }) => {
      await dashboard.open({ theme, placement: "view" });
      await expect(strip(page).getByText("Latest session in this project")).toBeVisible();
      await expect(strip(page).getByRole("link", { name: "Review the runbook" })).toBeVisible();
      await dashboard.check(`view-${theme}`);
    });
  }

  test("tells the windows and the session running in this project", async ({ dashboard, page }) => {
    await dashboard.open({ placement: "view" });
    await dashboard.settled();
    await expect(strip(page)).toMatchAriaSnapshot({ name: "view-strip.aria.yml" });
  });

  test("tells a session that has ended", async ({ dashboard, page }) => {
    await dashboard.open({ placement: "view", folders: ["/home/you/code/website"] });
    await expect(strip(page).getByRole("link", { name: "Fix flaky date test" })).toBeVisible();
    await expect(strip(page).getByText(/^Ended 1 h ago · ran \d+ min$/)).toBeVisible();
    await dashboard.check("view-ended");
  });

  test("names the project of the latest session elsewhere", async ({ dashboard, page }) => {
    await dashboard.open({ placement: "view", folders: ["/home/you/code/elsewhere"] });
    await expect(strip(page).getByText("Latest session · claude-dashboard")).toBeVisible();
    await dashboard.check("view-elsewhere");
  });

  test("says when there is no session yet", async ({ dashboard, page }) => {
    await dashboard.open({ placement: "view", scenario: "empty" });
    await expect(strip(page).getByText("No Claude Code session today yet.")).toBeVisible();
    await expect(
      strip(page).getByRole("button", { name: "Connect the status line…" }),
    ).toBeVisible();
    await dashboard.check("view-none");
  });

  test("shows a first read's progress", async ({ dashboard, page }) => {
    await dashboard.open({ placement: "view", scenario: "first-run" });
    await expect(
      strip(page).getByRole("progressbar", { name: "Reading your history" }),
    ).toHaveAttribute("aria-valuenow", "41");
    await expect(strip(page).getByText("41 of 254 files")).toBeVisible();
    await dashboard.check("view-first-run");
  });

  test("never prices an unpriced model at nothing", async ({ dashboard, page }) => {
    await dashboard.open({ placement: "view", scenario: "unpriced-session" });
    await expect(strip(page).getByText("Unpriced", { exact: true })).toBeVisible();
    await expect(strip(page).getByText("$0.00")).toHaveCount(0);
    await dashboard.check("view-unpriced");
  });

  test("offers to connect the status line without plan limits", async ({ dashboard, page }) => {
    await dashboard.open({ placement: "view", scenario: "no-limits" });
    await expect(
      strip(page).getByRole("button", { name: "Connect the status line…" }),
    ).toBeVisible();
    await dashboard.check("view-no-limits");
  });

  test("says how old old plan limits are and why", async ({ dashboard, page }) => {
    await dashboard.open({ placement: "view", scenario: "stale-limits" });
    await expect(
      strip(page).getByText("Claude Code hasn't run in a terminal since 09:00"),
    ).toBeVisible();
    await expect(strip(page).getByText(/^at \d\d:\d\d$/)).toHaveCount(2);
    await expect(strip(page).getByText(/^\d h \d\d min old$/)).toHaveCount(2);
    await dashboard.check("view-stale-limits");
  });

  test("leaves the plan limits out of the Overview below", async ({ dashboard, page }) => {
    await dashboard.open({ placement: "view" });
    await expect(page.getByRole("heading", { name: "API-equivalent cost per day" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Plan limits" })).toHaveCount(0);
  });

  test("opens the session from the strip", async ({ dashboard, page }) => {
    await dashboard.open({ placement: "view" });
    await strip(page).getByRole("link", { name: "Review the runbook" }).click();
    await expect(
      page.getByRole("navigation", { name: "Breadcrumb" }).getByText("Review the runbook"),
    ).toBeVisible();
    await expect(
      page.getByRole("navigation", { name: "Dashboard sections" }).getByRole("link", {
        name: "Sessions",
      }),
    ).toHaveAttribute("aria-current", "page");
    await expect(strip(page)).toBeVisible();
  });

  test("follows the folders VS Code has open", async ({ dashboard, page }) => {
    await dashboard.open({ placement: "view" });
    await expect(strip(page).getByRole("link", { name: "Review the runbook" })).toBeVisible();
    await page.evaluate(() => {
      window.claudeDashboardBridge?.setFolders?.({
        folders: ["/home/you/code/website"],
        ignoreCase: false,
      });
    });
    await expect(strip(page).getByRole("link", { name: "Fix flaky date test" })).toBeVisible();
  });

  test("keeps the strip in sight while the rest scrolls", async ({ dashboard, page }) => {
    await dashboard.open({ placement: "view" });
    await dashboard.settled();
    const before = await strip(page).boundingBox();
    await page.locator("#view-scroll").evaluate((scroller) => {
      scroller.scrollTo(0, scroller.scrollHeight);
    });
    expect(await strip(page).boundingBox()).toEqual(before);
    await expect(page.getByRole("navigation", { name: "Dashboard sections" })).not.toBeInViewport();
  });

  test("never hides what has focus under the strip", async ({ dashboard, page }) => {
    await dashboard.open({ placement: "view" });
    await dashboard.settled();
    const bottom = await strip(page).evaluate((element) => element.getBoundingClientRect().bottom);
    for (let stop = 0; stop < 30; stop += 1) {
      await page.keyboard.press("Tab");
      const focused = await page.evaluate(() => {
        const element = document.activeElement;
        if (!element || element === document.body) return undefined;
        const rect = element.getBoundingClientRect();
        return { inStrip: element.closest(".strip") !== null, top: rect.top };
      });
      if (!focused || focused.inStrip) continue;
      // Less a pixel's rounding.
      expect(focused.top).toBeGreaterThanOrEqual(bottom - 1);
    }
  });

  test("paints its last figures at once when shown again", async ({ dashboard, page, browser }) => {
    await dashboard.open({ placement: "view" });
    await dashboard.settled();
    const state = await page.evaluate(() => window.claudeDashboardBridge?.memory?.());
    expect(state).toMatchObject({ section: "overview", strip: { as: 1 } });

    // Shown again, before the server has answered anything.
    const again = await browser.newPage({ viewport: PANEL });
    await again.route("**/__bridge/request**", () => undefined);
    await new Dashboard(again).open({ placement: "view", state, waitForAnswers: false });
    await expect(strip(again).getByRole("link", { name: "Review the runbook" })).toBeVisible();
    await expect(strip(again).getByText("42%")).toBeVisible();
    await again.close();
  });
});

test.describe("beside the terminal", () => {
  test.use({ viewport: BESIDE });

  for (const theme of ["vscode-dark", "vscode-light"] as const) {
    test(`fits its width in ${theme}`, async ({ dashboard, page }) => {
      await dashboard.open({ theme, placement: "view" });
      await expect(strip(page).getByRole("link", { name: "Review the runbook" })).toBeVisible();
      await dashboard.check(`view-beside-${theme}`);
    });
  }
});

test.describe("in a sidebar", () => {
  test.use({ viewport: SIDEBAR });

  for (const theme of ["vscode-dark", "vscode-light"] as const) {
    test(`stacks the strip in ${theme}`, async ({ dashboard, page }) => {
      await dashboard.open({ theme, placement: "view" });
      await expect(strip(page).getByRole("link", { name: "Review the runbook" })).toBeVisible();
      await dashboard.check(`view-sidebar-${theme}`);
    });
  }
});

test.describe("320 pixels wide", () => {
  test.use({ viewport: { width: 320, height: 720 } });

  test("reflows without scrolling sideways", async ({ dashboard, page }) => {
    await dashboard.open({ placement: "view", folders: [PROJECT_FOLDER] });
    await dashboard.settled();
    const sideways = await page.evaluate(() => {
      const scroller = document.getElementById("view-scroll");
      const page = document.scrollingElement ?? document.documentElement;
      return Math.max(
        page.scrollWidth - page.clientWidth,
        (scroller?.scrollWidth ?? 0) - (scroller?.clientWidth ?? 0),
      );
    });
    expect(sideways).toBeLessThanOrEqual(0);
    await dashboard.check("view-narrow");
  });
});

test.describe("in a low panel", () => {
  test.use({ viewport: { width: 1132, height: 200 } });

  test("tightens around the strip, its targets still large enough", async ({ dashboard, page }) => {
    await dashboard.open({ placement: "view" });
    await dashboard.settled();
    const heights = await page
      .locator(".section-links a, .strip a, .strip button")
      .evaluateAll((targets) => targets.map((target) => target.getBoundingClientRect().height));
    expect(Math.min(...heights)).toBeGreaterThanOrEqual(24);
    await dashboard.check("view-low");
  });
});
