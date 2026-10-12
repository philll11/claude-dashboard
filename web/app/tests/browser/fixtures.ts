// The dashboard as a browser test sees it: the built app in one of VS Code's themes, talking to
// the real server over the bridge, at the fixtures' moment.

import { fileURLToPath } from "node:url";

import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test as base } from "@playwright/test";

import type { UpdateState } from "../../src/host.ts";
import { NOW_MS } from "../bridge/moment.ts";
import { applyTheme, type ThemeName, VSCODE_THEMES, type VsCodeTheme } from "../bridge/themes.ts";

export { expect };

export interface Open {
  theme?: ThemeName;
  scenario?: string;
  /** A folder the folder picker answers with. */
  pickedFolder?: string;
  /** Where the desktop app's updates stand. */
  update?: UpdateState;
}

export class Dashboard {
  constructor(readonly page: Page) {}

  async open({ theme = "vscode-dark", scenario = "default", pickedFolder, update }: Open = {}) {
    const themes: Record<string, VsCodeTheme | undefined> = VSCODE_THEMES;
    await this.page.clock.setFixedTime(NOW_MS);
    await this.page.addInitScript(applyTheme, { theme, vscode: themes[theme] });
    await this.page.addInitScript(
      (settings) => {
        window.claudeDashboardBridge = settings;
      },
      {
        scenario,
        surface: theme.startsWith("desktop") ? "desktop" : "vscode",
        ...(pickedFolder === undefined ? {} : { pickedFolder }),
        ...(update === undefined ? {} : { update }),
      },
    );
    await this.page.addInitScript({
      path: fileURLToPath(new URL("../bridge/client.js", import.meta.url)),
    });
    await this.page.goto("/");
    await expect(this.page.getByRole("navigation", { name: "Dashboard sections" })).toBeVisible();
    await expect(this.page.locator("main > *").first()).toBeVisible();
  }

  /** Opens a section the way a person does, from the navigation. */
  async section(name: string) {
    await this.page
      .getByRole("navigation", { name: "Dashboard sections" })
      .getByRole("link", { name, exact: true })
      .click();
    await expect(this.page.getByRole("heading", { level: 1, name })).toBeAttached();
  }

  /** Waits until no request is on its way and the page has drawn what came back. */
  async settled() {
    await this.page.waitForFunction(() => window.claudeDashboardBridge?.pending === 0);
    await this.page.evaluate(
      () =>
        new Promise<void>((resolve) => {
          requestAnimationFrame(() => {
            requestAnimationFrame(() => {
              resolve();
            });
          });
        }),
    );
    await this.page.waitForFunction(() => window.claudeDashboardBridge?.pending === 0);
  }

  /**
   * Checks the page against its screenshot and for accessibility problems. A page with a modal
   * dialog open is captured as the window shows it, since the dialog's backdrop covers the window
   * rather than the whole page.
   */
  async check(name: string) {
    await this.settled();
    await this.page.evaluate(() => document.fonts.ready);
    const modal = await this.page.evaluate(() => document.querySelector("dialog:modal") !== null);
    await expect(this.page).toHaveScreenshot(`${name}.png`, { fullPage: !modal });
    const results = await new AxeBuilder({ page: this.page })
      .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"])
      .analyze();
    expect(
      results.violations.map((violation) => ({
        id: violation.id,
        nodes: violation.nodes.map((node) => node.target.join(" ")),
      })),
    ).toEqual([]);
  }
}

export const test = base.extend<{ dashboard: Dashboard }>({
  dashboard: async ({ page }, use) => {
    await use(new Dashboard(page));
  },
});
