// The installed desktop app, driven through tauri-driver: its sections, its numbers against the
// command line's, an export, the status line's connection, and a second start. scripts/e2e.mjs
// installs the app and sets the environment these tests read.

import { execFileSync, spawn } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { strict as assert } from "node:assert";

import { Builder, By, error, until, type WebDriver, type WebElement } from "selenium-webdriver";

const app = process.env.E2E_APP ?? "";
const program = process.env.E2E_PROGRAM ?? "";
const folder = process.env.E2E_FOLDER ?? "";
const root = process.env.E2E_ROOT ?? "";
const screenshots = process.env.E2E_SCREENSHOTS ?? "";
const exportFolder = process.env.E2E_EXPORTS ?? "";
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

/** The section's heading. The dashboard replaces it when the section changes, so a heading
 * replaced while it is read is read again. */
async function heading(): Promise<string> {
  for (;;) {
    const h1 = await session().wait(until.elementLocated(By.css("main h1")), 30_000);
    try {
      return await h1.getText();
    } catch (problem) {
      if (!(problem instanceof error.StaleElementReferenceError)) throw problem;
    }
  }
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

  it("saves the sessions it lists, as the command line reports them", async () => {
    await open("Sessions");
    await press("Export");
    const csv = await session().wait(
      until.elementLocated(
        By.xpath('//*[@role="menuitem"][contains(normalize-space(), "Export as CSV")]'),
      ),
      10_000,
    );
    await csv.click();
    const saved = await session().wait(
      until.elementLocated(By.css('[role="status"] .toast-title')),
      60_000,
    );
    const path = await session().findElement(By.css('[role="status"] .toast-path')).getText();
    const range =
      /^claude-dashboard-sessions-(\d{4}-\d{2}-\d{2})-to-(\d{4}-\d{2}-\d{2})\.csv$/.exec(
        basename(path),
      );
    assert.ok(range, `the file has the suggested name: ${path}`);
    assert.equal(dirname(path), exportFolder);
    const [, first = "", last = ""] = range;
    const report = (format: string) =>
      execFileSync(program, ["sessions", "--since", first, "--until", last, format], {
        env,
        encoding: "utf8",
      });
    const sessions = JSON.parse(report("--json")) as unknown[];
    assert.equal(await saved.getText(), `Saved ${String(sessions.length)} sessions`);
    // A saved file starts with the byte order mark that spreadsheets need to read UTF-8.
    assert.equal(readFileSync(path, "utf8"), `\uFEFF${report("--csv")}`);
    await shoot("export-saved");
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

  it("keeps running in the notification area when its window closes", async function () {
    // Closing the window as a person does takes the system's own close message, which only
    // Windows can send here.
    if (process.platform !== "win32") this.skip();
    await open("Settings");
    const keepRunning = await session().findElement(
      By.xpath(
        '//*[@role="switch"][@aria-labelledby=//*[normalize-space()="Keep running when the window closes"]/@id]',
      ),
    );
    if ((await keepRunning.getAttribute("aria-checked")) !== "true") await keepRunning.click();
    await session().wait(
      async () => (await keepRunning.getAttribute("aria-checked")) === "true",
      10_000,
    );

    closeWindow("Dashboard for Claude Code");
    await session().wait(() => !windowShown("Dashboard for Claude Code"), 10_000);
    const second = spawn(app, ["--section", "overview"], { env, stdio: "ignore" });
    await new Promise((resolve) => second.on("exit", resolve));
    await session().wait(() => windowShown("Dashboard for Claude Code"), 20_000);
    await session().wait(async () => (await heading()) === "Overview", 20_000);

    await open("Settings");
    const again = await session().findElement(
      By.xpath(
        '//*[@role="switch"][@aria-labelledby=//*[normalize-space()="Keep running when the window closes"]/@id]',
      ),
    );
    await again.click();
    await session().wait(
      async () => (await again.getAttribute("aria-checked")) === "false",
      10_000,
    );
  });
});

/** Asks the app's window to close, as its close button does: Windows' WM_CLOSE. */
function closeWindow(title: string) {
  powershell(
    `$window = [Win32]::FindWindow([NullString]::Value, '${title}'); if ($window -eq [IntPtr]::Zero) { exit 1 }; [void][Win32]::PostMessage($window, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)`,
  );
}

/** Whether the app's window shows. */
function windowShown(title: string): boolean {
  return (
    powershell(
      `$window = [Win32]::FindWindow([NullString]::Value, '${title}'); [Win32]::IsWindowVisible($window)`,
    ).trim() === "True"
  );
}

function powershell(script: string): string {
  const user32 = `Add-Type -Namespace '' -Name Win32 -MemberDefinition '
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string title);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);'`;
  return execFileSync("powershell", ["-NoProfile", "-Command", `${user32}; ${script}`], {
    encoding: "utf8",
  });
}
