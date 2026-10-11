// The extension as a person meets it: installed from its package into a fresh VS Code, on the
// synthetic Claude Code folder, whose moment and numbers are the design's. Each theme leaves a
// screenshot in E2E_SCREENSHOTS to compare with the design's boards.

import * as assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import {
  ActivityBar,
  By,
  type CustomTreeSection,
  EditorView,
  type TreeItem,
  until,
  VSBrowser,
  WebDriver,
  WebView,
  Workbench,
} from "vscode-extension-tester";

const SETTINGS = process.env.E2E_SETTINGS ?? "";
const SCREENSHOTS = process.env.E2E_SCREENSHOTS ?? "";
const NAME = "Dashboard for Claude Code";
const THEMES = ["Default Dark Modern", "Default Light Modern", "Default High Contrast"];

let driver: WebDriver;

async function screenshot(name: string): Promise<void> {
  mkdirSync(SCREENSHOTS, { recursive: true });
  writeFileSync(join(SCREENSHOTS, `${name}.png`), await driver.takeScreenshot(), "base64");
}

/** Changes a user setting the way editing settings.json does; VS Code applies it at once. */
async function setSetting(key: string, value: unknown): Promise<void> {
  const settings = JSON.parse(readFileSync(SETTINGS, "utf8")) as Record<string, unknown>;
  settings[key] = value;
  writeFileSync(SETTINGS, JSON.stringify(settings, null, 2));
  await driver.sleep(1_500);
}

async function rows(items: TreeItem[]): Promise<string[]> {
  return Promise.all(
    items.map(async (item) => `${await item.getLabel()} | ${(await item.getDescription()) ?? ""}`),
  );
}

async function summary(): Promise<CustomTreeSection> {
  const control = await new ActivityBar().getViewControl(NAME);
  assert.ok(control, "the activity bar has the extension's view");
  const view = await control.openView();
  return view.getContent().getSection(NAME);
}

/** The section the dashboard shows, by its link's name. */
async function shownSection(): Promise<string> {
  const webview = new WebView();
  await webview.switchToFrame(20_000);
  try {
    const current = await driver.wait(
      until.elementLocated(By.css('nav[aria-label="Dashboard sections"] [aria-current="page"]')),
      20_000,
    );
    return await current.getText();
  } finally {
    await webview.switchBack();
  }
}

describe(`${NAME}, installed from its package`, function () {
  this.timeout(180_000);

  before(async function () {
    driver = VSBrowser.instance.driver;
    await VSBrowser.instance.waitForWorkbench();
  });

  it("shows the five-hour window in the status bar", async () => {
    const item = await driver.wait(
      until.elementLocated(By.css('[id$="claudeDashboard.status"]')),
      30_000,
    );
    await driver.wait(async () => (await item.getText()).includes("5h"), 30_000);
    assert.equal((await item.getText()).trim(), "5h 42% · 14:20");
    const link = await item.findElement(By.css("[aria-label]"));
    assert.equal(
      await link.getAttribute("aria-label"),
      "Five-hour window 42% used, resets at 14:20. Open the usage dashboard.",
    );
  });

  it("summarises the usage in the sidebar", async () => {
    const section = await summary();
    const items = await driver.wait(async () => {
      const visible = await section.getVisibleItems();
      return visible.length > 4 ? visible : undefined;
    }, 30_000);
    const shown = await rows(items ?? []);
    assert.deepEqual(shown.slice(0, 10), [
      "Plan limits | official · 2 min ago",
      "Five-hour window | 42% · resets 14:20",
      "Weekly, all models | 71% · resets Thu 09:00",
      "Weekly, Fable | 18% · resets Thu 09:00",
      "Today | $5.10 estimate",
      "Opus 5.5 | $2.10",
      "Fable 5.1 | $1.83",
      "Sonnet 5.5 | $1.11",
      "Haiku 5.5 | $0.06",
      "This week | $69.46 + unpriced estimate",
    ]);
    assert.equal(shown[10], "Top projects | this week");
    assert.equal(shown.length, 16, "five projects follow");
  });

  it("opens the matching section of the dashboard from the sidebar", async () => {
    const section = await summary();
    const weekly = await section.findItem("Weekly, all models");
    assert.ok(weekly);
    await weekly.click();
    assert.equal(await shownSection(), "Limits");
    const today = await section.findItem("Today");
    assert.ok(today);
    await today.select();
    assert.equal(await shownSection(), "Overview");
    // A dashboard behind another editor has lost its page, and shows the section once back.
    await new Workbench().executeCommand("File: New Untitled Text File");
    const limits = await section.findItem("Plan limits");
    assert.ok(limits);
    await limits.select();
    assert.equal(await shownSection(), "Limits");
  });

  for (const theme of THEMES) {
    it(`shows the dashboard in ${theme}`, async () => {
      await setSetting("workbench.colorTheme", theme);
      const week = await (await summary()).findItem("This week");
      assert.ok(week);
      await week.select();
      assert.equal(await shownSection(), "Overview");
      await driver.sleep(1_000);
      await screenshot(theme.replace(/^Default /, "").replaceAll(" ", "-"));
    });
  }

  it("reopens the dashboard where it was after the window reloads", async () => {
    await setSetting("workbench.colorTheme", THEMES[0]);
    const section = await summary();
    const projects = await section.findItem("Top projects");
    assert.ok(projects);
    await projects.click();
    assert.equal(await shownSection(), "Projects");
    await new Workbench().executeCommand("Developer: Reload Window");
    await VSBrowser.instance.waitForWorkbench();
    const titles = await new EditorView().getOpenEditorTitles();
    assert.ok(
      titles.includes("Usage dashboard"),
      `the dashboard tab is back: ${titles.join(", ")}`,
    );
    await new EditorView().openEditor("Usage dashboard");
    assert.equal(await shownSection(), "Projects");
  });
});
