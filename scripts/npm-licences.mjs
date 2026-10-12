// Checks the licences of the npm packages: what ships in the dashboard, the desktop app and the
// VS Code extension must be permissive, apart from the design's fonts, and the tools that build and
// test them may also be weak copyleft or data licences. `deny.toml` holds the same rules for the
// Rust crates.

import { execFileSync } from "node:child_process";

const PERMISSIVE = ["0BSD", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "MIT", "Zlib"];
const TOOLS_ALSO = [
  "Artistic-2.0",
  "BlueOak-1.0.0",
  "CC-BY-3.0",
  "CC-BY-4.0",
  "CC0-1.0",
  "EPL-2.0",
  "MPL-2.0",
  "Python-2.0",
  "WTFPL",
];
/** Fonts that ship under the SIL Open Font Licence, which lets software bundle them. No other
 * package may use it. */
const FONTS = { licence: "OFL-1.1", name: /^@fontsource\/ibm-plex-(sans|mono)$/ };

/** Tools under terms of their own, each with why it is there. None of them ships. */
const TOOL_EXCEPTIONS = [
  {
    name: /^@vscode\/vsce-sign(-.+)?$/,
    reason: "Microsoft's signing tool, which comes with vsce, the extension's packager",
  },
];

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

/** Whether an SPDX licence expression lets the project use a package under one of `allowed`: an
 * OR needs either side, an AND both, and an exception only adds to what its licence allows. */
function permits(expression, allowed) {
  const tokens = expression.match(/\(|\)|[^\s()]+/g) ?? [];
  let index = 0;
  const primary = () => {
    const token = tokens[index++];
    if (token === "(") {
      const value = either();
      index++;
      return value;
    }
    if (tokens[index] === "WITH") index += 2;
    return allowed.includes(token);
  };
  const both = () => {
    let value = primary();
    while (tokens[index] === "AND") {
      index++;
      value = primary() && value;
    }
    return value;
  };
  const either = () => {
    let value = both();
    while (tokens[index] === "OR") {
      index++;
      value = both() || value;
    }
    return value;
  };
  return either() && index === tokens.length;
}

function check(production, allowed) {
  const problems = Object.entries(licences(production))
    .map(([licence, packages]) => [
      licence,
      packages.filter(
        (p) =>
          !permits(licence, allowed) &&
          !(licence === FONTS.licence && FONTS.name.test(p.name)) &&
          (production || !TOOL_EXCEPTIONS.some((exception) => exception.name.test(p.name))),
      ),
    ])
    .filter(([, packages]) => packages.length > 0)
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
