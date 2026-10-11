// Checks the licences of the npm packages: what ships in the dashboard and the VS Code extension
// must be permissive, and the tools that build and test them may also be MPL or data licences.
// `deny.toml` holds the same rules for the Rust crates.

import { execFileSync } from "node:child_process";

const PERMISSIVE = ["0BSD", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "MIT"];
const TOOLS_ALSO = ["BlueOak-1.0.0", "CC-BY-4.0", "CC0-1.0", "MPL-2.0"];

function licences(production) {
  // Run as `pnpm check-licences`, which says where pnpm is, so that this works on every system.
  const pnpm = process.env.npm_execpath ?? "pnpm";
  const args = ["licenses", "list", "--json", ...(production ? ["--prod"] : [])];
  // pnpm is a native program, or a script for Node.
  const output = /\.[cm]?js$/.test(pnpm)
    ? execFileSync(process.execPath, [pnpm, ...args], { encoding: "utf8" })
    : execFileSync(pnpm, args, { encoding: "utf8" });
  return JSON.parse(output);
}

function check(production, allowed) {
  const problems = Object.entries(licences(production))
    .filter(([licence]) => !allowed.includes(licence))
    .map(([licence, packages]) => `${licence}: ${packages.map((p) => p.name).join(", ")}`);
  if (problems.length > 0) {
    console.error(
      `${production ? "Shipped" : "Build and test"} packages with licences the project does not allow:`,
    );
    for (const problem of problems) console.error(`  ${problem}`);
  }
  return problems.length === 0;
}

const shipped = check(true, PERMISSIVE);
const tools = check(false, [...PERMISSIVE, ...TOOLS_ALSO]);
if (!shipped || !tools) process.exit(1);
console.log("Every npm package has a licence the project allows.");
