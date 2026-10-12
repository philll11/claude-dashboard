import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import tokens from "../../../design/tokens.json";
import { VSCODE_THEMES } from "../tests/bridge/themes";

const css = readFileSync(new URL("./theme.css", import.meta.url), "utf8").replace(
  /\/\*[\s\S]*?\*\//g,
  "",
);

/** The custom properties set by the rule whose selectors are exactly `selectors`. */
function properties(...selectors: string[]): Map<string, string> {
  const rule = [...css.matchAll(/([^{}]+)\{([^}]*)\}/g)].find(
    (match) =>
      (match[1] ?? "")
        .split(",")
        .map((selector) => selector.trim())
        .join(",") === selectors.join(","),
  );
  expect(rule, selectors.join(", ")).toBeDefined();
  return new Map(
    [...(rule?.[2] ?? "").matchAll(/(--[\w-]+):\s*([^;]+);/g)].map((match) => [
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
  "sidebar",
  "sidebar-current",
] as const;

describe("the stylesheet holds the design tokens", () => {
  it("for the dark desktop theme", () => {
    const dark = properties(":root");
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
    const vscode = properties(
      "body.vscode-light",
      "body.vscode-dark",
      "body.vscode-high-contrast",
      "body.vscode-high-contrast-light",
    );
    for (const [name, value] of Object.entries(tokens.vscode)) {
      expect(vscode.get(`--${name}`), name).toBe(value);
    }
  });

  it("for VS Code's high contrast themes", () => {
    const highContrast = properties("body.vscode-high-contrast", "body.vscode-high-contrast-light");
    for (const [name, value] of Object.entries(tokens["vscode-high-contrast"])) {
      expect(highContrast.get(`--${name}`), name).toBe(value);
    }
  });
});

describe("the VS Code themes the browser tests use", () => {
  const stylesheets = ["./theme.css", "./app.css"].map((file) =>
    readFileSync(new URL(file, import.meta.url), "utf8"),
  );
  const used = new Set(stylesheets.flatMap((text) => text.match(/--vscode-[\w-]+/g) ?? []));

  it.each(Object.entries(VSCODE_THEMES))(
    "define every variable the stylesheets use: %s",
    (_, theme) => {
      const defined: Record<string, string> = theme.variables;
      const missing = [...used].filter(
        (name) => defined[name] === undefined && !OPTIONAL_IN_SOME_THEMES.has(name),
      );
      expect(missing).toEqual([]);
    },
  );
});

/** Variables that VS Code leaves out of some themes, which the stylesheets give a fallback. */
const OPTIONAL_IN_SOME_THEMES = new Set([
  "--vscode-contrastActiveBorder",
  "--vscode-contrastBorder",
  "--vscode-widget-shadow",
  "--vscode-button-border",
  "--vscode-testing-iconPassed",
]);
