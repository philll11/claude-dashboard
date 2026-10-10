import { readFileSync } from "node:fs";

import { expect, it } from "vitest";

import manifest from "../package.json";

it("has the version every other surface has", () => {
  const cargo = readFileSync(new URL("../../../Cargo.toml", import.meta.url), "utf8");
  const workspace = cargo.slice(cargo.indexOf("[workspace.package]"));
  expect(manifest.version).toBe(/^version = "([^"]+)"$/m.exec(workspace)?.[1]);
});
