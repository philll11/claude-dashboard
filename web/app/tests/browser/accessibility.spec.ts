// The dashboard against WCAG 2.2 AA beyond each screenshot's own check: every state and dialog in
// every theme, the keyboard through every section, Escape out of every menu and dialog, Windows
// contrast themes, reduced motion, each section's accessible structure, and reflow in a narrow
// VS Code tab.

import type { Locator, Page } from "@playwright/test";

import type { ThemeName } from "../bridge/themes.ts";
import { type Dashboard, expect, test } from "./fixtures.ts";

const THEMES: ThemeName[] = [
  "vscode-dark",
  "vscode-light",
  "vscode-high-contrast",
  "vscode-high-contrast-light",
  "desktop-dark",
  "desktop-light",
];

const SECTIONS = ["Overview", "Sessions", "Projects", "Models", "Limits", "Settings"] as const;

/** The sections, and the view of one session, which opens from Sessions. */
const VIEWS = [...SECTIONS, "Session detail"] as const;

/** A view's name in a file name. */
function slug(view: string): string {
  return view.toLowerCase().replaceAll(" ", "-");
}

/** The states the design asks for, as the test bridge's scenarios. */
const STATES = [
  "first-run",
  "empty",
  "no-limits",
  "stale-limits",
  "prices-drift",
  "login-expired",
  "unreadable-folder",
  "newer-schema",
  "too-new-schema",
  "claude-usage",
];

interface Opened {
  /** What the dialog or menu is, once open. */
  shown: Locator;
  /** What opened it, which has the focus again once it closes; none for what the app opens. */
  opener?: Locator;
}

interface Dialog {
  scenario?: string;
  /** Only the desktop app shows it. */
  desktop?: boolean;
  open: (page: Page, dashboard: Dashboard) => Promise<Opened>;
}

/** Every dialog and menu, opened as a person opens it. */
const DIALOGS: Record<string, Dialog> = {
  "the status line's connection": {
    scenario: "change-status-line",
    async open(page) {
      const opener = page.getByRole("button", { name: "Connect the status line…" });
      await opener.click();
      return { shown: page.getByRole("dialog"), opener };
    },
  },
  "the usage endpoint's consent": {
    scenario: "change-endpoint",
    async open(page, dashboard) {
      await dashboard.section("Settings");
      const opener = page.getByRole("switch", { name: /Anthropic's usage endpoint/ });
      await opener.click();
      return { shown: page.getByRole("dialog"), opener };
    },
  },
  "the export menu": {
    async open(page, dashboard) {
      await dashboard.section("Sessions");
      const opener = page.getByRole("button", { name: "Export" });
      await opener.click();
      return { shown: page.getByRole("menu", { name: "Export" }), opener };
    },
  },
  "a filter menu": {
    async open(page) {
      const opener = page.getByRole("button", { name: "Model: All models" });
      await opener.click();
      return { shown: page.getByRole("menu", { name: "Model" }), opener };
    },
  },
  "the first close": {
    desktop: true,
    async open(page) {
      await page.evaluate(() => {
        window.claudeDashboardBridge?.askClose?.();
      });
      return { shown: page.getByRole("dialog") };
    },
  },
};

/** Opens the dashboard at a section, ready to look at. */
async function at(dashboard: Dashboard, view: string, theme: ThemeName = "vscode-dark") {
  await dashboard.open({ theme });
  if (view === "Session detail") {
    await dashboard.section("Sessions");
    await dashboard.page.getByRole("link", { name: /Price table for Claude 5 models/ }).click();
    await expect(
      dashboard.page.getByRole("heading", { name: "Models and subagents" }),
    ).toBeVisible();
  } else if (view !== "Overview") {
    await dashboard.section(view);
  }
  await dashboard.settled();
}

/** What a keyboard stop is, as the walk sees it. */
interface Stop {
  html: string;
  /** Whether it comes after the stop before it in the page. */
  inOrder: boolean;
  /** Whether it is on screen and not wholly covered by anything else. */
  seen: boolean;
  /** Whether it shows that it has the focus, by an outline or a shadow. */
  ring: boolean;
}

/**
 * Tabs through the page from its start until the focus comes round again. Every stop has a name,
 * shows that it has the focus, can be seen, and comes in the order of the page.
 */
async function walk(page: Page) {
  const positive = await page
    .locator("[tabindex]")
    .evaluateAll(
      (elements) =>
        elements.filter((element) => Number(element.getAttribute("tabindex")) > 0).length,
    );
  expect(positive, "no element changes the order of the page").toBe(0);
  await page.evaluate(() => {
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
    window.previousStop = null;
  });
  const seen = new Set<string>();
  for (let step = 0; step < 300; step += 1) {
    await page.keyboard.press("Tab");
    const stop = await page.evaluate((): Stop | null => {
      const element = document.activeElement;
      if (!element || element === document.body) return null;
      const box = element.getBoundingClientRect();
      const points = [
        [box.left + box.width / 2, box.top + box.height / 2],
        [box.left + 2, box.top + 2],
        [box.right - 2, box.top + 2],
        [box.left + 2, box.bottom - 2],
        [box.right - 2, box.bottom - 2],
      ];
      const shown = points.some(([x = 0, y = 0]) => {
        const hit = document.elementFromPoint(x, y);
        return hit !== null && (hit === element || element.contains(hit));
      });
      const ringed = (node: Element) => {
        const style = getComputedStyle(node);
        return (
          (style.outlineStyle !== "none" && Number.parseFloat(style.outlineWidth) > 0) ||
          style.boxShadow !== "none"
        );
      };
      // A control's ring is its own, or its wrapper's, as a search box draws it around the field.
      let ring = ringed(element);
      for (
        let wrapper = element.parentElement, depth = 0;
        !ring && wrapper && depth < 2;
        depth += 1
      ) {
        ring = ringed(wrapper);
        wrapper = wrapper.parentElement;
      }
      const previous = window.previousStop;
      window.previousStop = element;
      return {
        html: element.outerHTML.slice(0, 160),
        inOrder:
          previous === null ||
          (previous.compareDocumentPosition(element) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0,
        seen: box.width > 0 && box.height > 0 && shown,
        ring,
      };
    });
    if (stop === null || seen.has(stop.html)) return;
    seen.add(stop.html);
    const role = await page.locator(":focus").ariaSnapshot();
    expect(role, `a name for ${stop.html}`).toMatch(/"[^"]+"/);
    expect(stop.inOrder, `${stop.html} in the order of the page`).toBe(true);
    expect(stop.seen, `${stop.html} can be seen`).toBe(true);
    expect(stop.ring, `${stop.html} shows its focus`).toBe(true);
  }
  throw new Error("The focus never came round again.");
}

declare global {
  interface Window {
    previousStop: Element | null;
  }
}

for (const theme of THEMES) {
  test.describe(theme, () => {
    for (const scenario of STATES) {
      test(`the ${scenario} state passes the rules`, async ({ dashboard }) => {
        await dashboard.open({ theme, scenario });
        await dashboard.audit();
      });
    }

    for (const [name, dialog] of Object.entries(DIALOGS)) {
      if (dialog.desktop && !theme.startsWith("desktop")) continue;
      test(`${name} passes the rules`, async ({ dashboard, page }) => {
        await dashboard.open({ theme, ...(dialog.scenario ? { scenario: dialog.scenario } : {}) });
        const { shown } = await dialog.open(page, dashboard);
        await expect(shown).toBeVisible();
        await dashboard.audit();
      });
    }

    for (const view of VIEWS) {
      test(`${view} works from the keyboard`, async ({ dashboard, page }) => {
        await at(dashboard, view, theme);
        await walk(page);
      });
    }
  });
}

for (const [name, dialog] of Object.entries(DIALOGS)) {
  test(`Escape leaves ${name}`, async ({ dashboard, page }) => {
    await dashboard.open({
      ...(dialog.desktop ? { theme: "desktop-dark" as const } : {}),
      ...(dialog.scenario ? { scenario: dialog.scenario } : {}),
    });
    const { shown, opener } = await dialog.open(page, dashboard);
    await expect(shown).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(shown).toBeHidden();
    if (opener) await expect(opener).toBeFocused();
  });
}

for (const theme of ["desktop-dark", "desktop-light"] as const) {
  test.describe(`${theme} in a Windows contrast theme`, () => {
    test.beforeEach(async ({ page }) => {
      await page.emulateMedia({ forcedColors: "active" });
    });
    for (const section of ["Overview", "Limits", "Settings"]) {
      test(section, async ({ dashboard }) => {
        await at(dashboard, section, theme);
        // The system sets every colour in a contrast theme, which axe's contrast rule cannot see.
        await dashboard.check(`${theme}-contrast-theme-${section.toLowerCase()}`, {
          skip: ["color-contrast"],
        });
      });
    }
  });
}

test("nothing moves when the system asks for less motion", async ({ dashboard, page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await dashboard.open({ scenario: "reading" });
  await dashboard.settled();
  const moving = await page.evaluate(() =>
    [...document.querySelectorAll("*")]
      .filter((element) => {
        const style = getComputedStyle(element);
        const animated =
          style.animationName !== "none" && Number.parseFloat(style.animationDuration) > 0;
        return animated || Number.parseFloat(style.transitionDuration) > 0;
      })
      .map((element) => element.outerHTML.slice(0, 120)),
  );
  expect(moving).toEqual([]);
});

for (const view of VIEWS) {
  test(`${view} keeps its accessible structure`, async ({ dashboard, page }) => {
    await at(dashboard, view);
    await expect(page.locator("body")).toMatchAriaSnapshot({ name: `${slug(view)}.aria.yml` });
  });
}

test.describe("in a narrow VS Code tab", () => {
  test.use({ viewport: { width: 320, height: 720 } });
  for (const view of VIEWS) {
    test(`${view} reflows without scrolling sideways`, async ({ dashboard, page }) => {
      await at(dashboard, view);
      const sideways = await page.evaluate(() => {
        const page = document.scrollingElement ?? document.documentElement;
        return page.scrollWidth - page.clientWidth;
      });
      expect(sideways).toBeLessThanOrEqual(0);
      await dashboard.check(`narrow-${slug(view)}`);
    });
  }
});
