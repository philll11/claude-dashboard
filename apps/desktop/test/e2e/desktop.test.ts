// The installed desktop app, driven through tauri-driver: its sections, its numbers against the
// command line's, the status line's connection, and a second start. scripts/e2e.mjs installs the
// app and sets the environment these tests read.

import { execFileSync, spawn } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { strict as assert } from "node:assert";

import { Builder, By, until, type WebDriver, type WebElement } from "selenium-webdriver";

const app = process.env.E2E_APP ?? "";
const program = process.env.E2E_PROGRAM ?? "";
const folder = process.env.E2E_FOLDER ?? "";
const root = process.env.E2E_ROOT ?? "";
const screenshots = process.env.E2E_SCREENSHOTS ?? "";
const env = JSON.parse(process.env.E2E_ENV ?? "{}") as NodeJS.ProcessEnv;

const SECTIONS = ["Overview", "Sessions", "Projects", "Models", "Limits", "Settings"];

let driver: WebDriver | undefined;

/** The session, which `before` starts. */
function session(): WebDriver {
  if (!driver) throw new Error("No WebDriver session started.");
  return driver;
}

async function shoot(name: string) {
  writeFileSync(join(screenshots, `${name}.png`), await session().takeScreenshot(), "base64");
}

function nav(): Promise<WebElement> {
  return session().wait(
    until.elementLocated(By.css('nav[aria-label="Dashboard sections"]')),
    60_000,
  );
}

async function heading(): Promise<string> {
  const h1 = await session().wait(until.elementLocated(By.css("main h1")), 30_000);
  return h1.getText();
}

async function open(section: string) {
  const link = await (await nav()).findElement(By.xpath(`.//a[normalize-space()="${section}"]`));
  await link.click();
  await session().wait(async () => (await heading()) === section, 15_000);
}

/** Clicks the button whose text is `name`, inside `scope` or anywhere. */
async function press(name: string, scope?: WebElement) {
  const button = await (scope ?? session()).findElement(
    By.xpath(`.//button[normalize-space()="${name}"]`),
  );
  await button.click();
}

function statusLine(): string | undefined {
  const settings = JSON.parse(readFileSync(join(root, "settings.json"), "utf8")) as {
    statusLine?: { command?: string };
  };
  return settings.statusLine?.command;
}

describe("the desktop app", () => {
  before(async () => {
    driver = await new Builder()
      .usingServer("http://127.0.0.1:4444/")
      .withCapabilities({ browserName: "wry", "tauri:options": { application: app } })
      .build();
  });

  after(async () => {
    await driver?.quit();
  });

  it("opens the dashboard beside its sections", async () => {
    const links = await (await nav()).findElements(By.css("a"));
    assert.deepEqual(await Promise.all(links.map((link) => link.getText())), SECTIONS);
    assert.equal(await heading(), "Overview");
    await session().wait(until.elementLocated(By.css("section.tile .tile-value")), 60_000);
    await shoot("overview");
  });

  it("shows the same numbers as the command line", async () => {
    const today = JSON.parse(
      execFileSync(program, ["today", "--json"], { env, encoding: "utf8" }),
    ) as { cost: { usd: number }; limits: { window: string; used_percentage: number }[] };
    const tile = await session().findElement(
      By.xpath(
        '//section[contains(@class, "tile")][h2[normalize-space()="Today"]]//*[contains(@class, "tile-value")]',
      ),
    );
    assert.equal(await tile.getText(), `$${today.cost.usd.toFixed(2)}`);
    const fiveHour = today.limits.find((limit) => limit.window === "five_hour");
    assert.ok(fiveHour, "the command line knows the five-hour window");
    const body = await session().findElement(By.css("main")).getText();
    assert.ok(body.includes(`${String(Math.round(fiveHour.used_percentage))}%`));
  });

  it("opens every section", async () => {
    for (const section of SECTIONS) {
      await open(section);
      await shoot(section.toLowerCase());
    }
  });

  it("connects Claude Code's status line to a copy of claude-dashboard next to the history", async () => {
    await open("Settings");
    assert.equal(statusLine(), "claude-dashboard statusline");

    await press("Disconnect");
    let dialog = await session().wait(until.elementLocated(By.css("dialog[open]")), 10_000);
    await press("Make this change", dialog);
    await session().wait(() => statusLine() === undefined, 10_000);

    await session().wait(
      until.elementLocated(By.xpath('//button[normalize-space()="Connect…"]')),
      10_000,
    );
    await press("Connect…");
    dialog = await session().wait(until.elementLocated(By.css("dialog[open]")), 10_000);
    await press("Make this change", dialog);
    await session().wait(() => statusLine()?.endsWith(" statusline") === true, 10_000);

    const copy = join(
      folder,
      "bin",
      process.platform === "win32" ? "claude-dashboard.exe" : "claude-dashboard",
    );
    assert.ok(existsSync(copy), `the status line's copy of claude-dashboard is at ${copy}`);
    assert.match(statusLine() ?? "", /\/bin\/claude-dashboard(\.exe)? statusline$/);
    await shoot("status-line-connected");
  });

  it("brings its window forward at the section a second start asks for", async () => {
    await open("Overview");
    const second = spawn(app, ["--section", "limits"], { env, stdio: "ignore" });
    const ended = new Promise<number | null>((resolve) => second.on("exit", resolve));
    try {
      await session().wait(async () => (await heading()) === "Limits", 20_000);
      const running = new Promise((resolve) => {
        setTimeout(() => {
          resolve("still running");
        }, 20_000);
      });
      assert.equal(await Promise.race([ended, running]), 0);
    } finally {
      // A second start that opened a window of its own is not left behind.
      second.kill();
    }
  });

  it("opens from the command line, where it last ran, at the section asked for", async () => {
    await open("Overview");
    execFileSync(program, ["app", "sessions"], { env, stdio: "ignore" });
    await session().wait(async () => (await heading()) === "Sessions", 20_000);
  });
});
