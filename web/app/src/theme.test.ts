import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import tokens from "../../../design/tokens.json";

const css = readFileSync(new URL("./theme.css", import.meta.url), "utf8");

/** The custom properties a rule whose selector starts with `selector` sets. */
function properties(selector: string): Map<string, string> {
  const start = css.indexOf(selector);
  expect(start, selector).toBeGreaterThanOrEqual(0);
  const body = css.slice(css.indexOf("{", start) + 1, css.indexOf("}", start));
  return new Map(
    [...body.matchAll(/(--[\w-]+):\s*([^;]+);/g)].map((match) => [
      match[1] ?? "",
      (match[2] ?? "").trim(),
    ]),
  );
}

const COLOURS = [
  "page",
  "panel",
  "line",
  "grid",
  "axis",
  "ink",
  "ink-2",
  "muted",
  "meter",
  "selected",
  "selected-ink",
  "track",
  "bar",
  "bar-track",
  "tip-bg",
  "tip-line",
  "warning-icon",
  "spark",
] as const;

describe("the stylesheet holds the design tokens", () => {
  it("for the dark desktop theme", () => {
    const dark = properties(":root {");
    for (const name of COLOURS) {
      expect(dark.get(`--${name}`), name).toBe(tokens.themes["desktop-dark"][name]);
    }
    for (const [model, colour] of Object.entries(tokens.series.dark)) {
      expect(dark.get(`--series-${model}`), model).toBe(colour);
    }
    for (const [status, colour] of Object.entries(tokens.status.dark)) {
      expect(dark.get(`--status-${status}`), status).toBe(colour);
    }
  });

  it("for the light desktop theme", () => {
    const light = properties(':root[data-theme="desktop-light"]');
    for (const name of COLOURS) {
      expect(light.get(`--${name}`), name).toBe(tokens.themes["desktop-light"][name]);
    }
    for (const [model, colour] of Object.entries(tokens.series.light)) {
      expect(light.get(`--series-${model}`), model).toBe(colour);
    }
    expect(light.get("--status-warning")).toBe(tokens.status.light.warning);
  });

  it("for VS Code, through its theme variables", () => {
    const vscode = properties("body.vscode-light,");
    for (const [name, variable] of Object.entries(tokens.vscode)) {
      if (name === "series" || name === "selected-outline") continue;
      expect(vscode.get(`--${name}`), name).toMatch(new RegExp(`^var\\(${variable}[,)]`));
    }
  });
});
