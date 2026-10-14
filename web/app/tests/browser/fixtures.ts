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
  /** In VS Code, the editor tab or the view in the panel or a sidebar. */
  placement?: "tab" | "view";
  /** The folders VS Code has open, for the view. */
  folders?: string[];
  /** What the dashboard remembered from an earlier visit. */
  state?: unknown;
  /** Whether to wait for the section, which needs the server's answers. */
  waitForAnswers?: boolean;
}

/** The project the design's running session is in, which the view's tests have open. */
export const PROJECT_FOLDER = "/home/you/code/claude-dashboard";

export class Dashboard {
  constructor(readonly page: Page) {}

  async open({
    theme = "vscode-dark",
    scenario = "default",
    pickedFolder,
    update,
    placement,
    folders = [PROJECT_FOLDER],
    state,
    waitForAnswers = true,
  }: Open = {}) {
    const themes: Record<string, VsCodeTheme | undefined> = VSCODE_THEMES;
    await this.page.clock.setFixedTime(NOW_MS);
    await this.page.addInitScript(applyTheme, { theme, vscode: themes[theme], placement });
    await this.page.addInitScript(
      (settings) => {
        window.claudeDashboardBridge = settings;
      },
      {
        scenario,
        surface: theme.startsWith("desktop") ? "desktop" : "vscode",
        folders: { folders, ignoreCase: false },
        ...(pickedFolder === undefined ? {} : { pickedFolder }),
        ...(update === undefined ? {} : { update }),
        ...(placement === undefined ? {} : { placement }),
        ...(state === undefined ? {} : { state }),
      },
    );
    await this.page.addInitScript({
      path: fileURLToPath(new URL("../bridge/client.js", import.meta.url)),
    });
    await this.page.goto("/");
    await expect(this.page.getByRole("navigation", { name: "Dashboard sections" })).toBeVisible();
    if (waitForAnswers) await expect(this.page.locator("main > *").first()).toBeVisible();
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
  async check(name: string, { skip = [] }: { skip?: string[] } = {}) {
    await this.settled();
    await this.page.evaluate(() => document.fonts.ready);
    const modal = await this.page.evaluate(() => document.querySelector("dialog:modal") !== null);
    await expect(this.page).toHaveScreenshot(`${name}.png`, { fullPage: !modal });
    await this.audit({ skip });
  }

  /** Checks the page, once settled, for problems against WCAG 2.2 AA that rules can find, but for
   * the rules in `skip`. */
  async audit({ skip = [] }: { skip?: string[] } = {}) {
    await this.settled();
    const results = await new AxeBuilder({ page: this.page })
      .disableRules(skip)
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
