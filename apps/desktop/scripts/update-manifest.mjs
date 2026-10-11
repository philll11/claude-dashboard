// Writes latest.json, which the desktop app reads from the latest release to find an update: for
// each platform and kind of installer, where its update bundle is and the bundle's signature.
// The updater looks for `{os}-{arch}-{installer}` first, then `{os}-{arch}`.
//
//   node scripts/update-manifest.mjs --folder <update bundles> --repository <owner/name> --tag v0.2.0
//     --out latest.json

import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";

const { values } = parseArgs({
  options: {
    folder: { type: "string" },
    repository: { type: "string" },
    tag: { type: "string" },
    out: { type: "string" },
  },
});
const { folder, repository, tag, out } = values;
if (!folder || !repository || !tag || !out) {
  throw new Error("--folder, --repository, --tag and --out are needed.");
}

/** Each update bundle, by the end of its name, and the updater targets it serves. */
const BUNDLES = [
  { ending: "-windows-x64-setup.exe", targets: ["windows-x86_64", "windows-x86_64-nsis"] },
  { ending: "-windows-arm64-setup.exe", targets: ["windows-aarch64", "windows-aarch64-nsis"] },
  { ending: "-macos-x64.app.tar.gz", targets: ["darwin-x86_64", "darwin-x86_64-app"] },
  { ending: "-macos-arm64.app.tar.gz", targets: ["darwin-aarch64", "darwin-aarch64-app"] },
  { ending: "-linux-x64.AppImage", targets: ["linux-x86_64", "linux-x86_64-appimage"] },
  { ending: "-linux-x64.deb", targets: ["linux-x86_64-deb"] },
  { ending: "-linux-arm64.AppImage", targets: ["linux-aarch64", "linux-aarch64-appimage"] },
  { ending: "-linux-arm64.deb", targets: ["linux-aarch64-deb"] },
];

const version = tag.replace(/^v/, "");
const names = readdirSync(folder);
const platforms = {};
for (const { ending, targets } of BUNDLES) {
  const name = names.find((file) => file.endsWith(ending));
  if (!name || !existsSync(join(folder, `${name}.sig`))) {
    throw new Error(`There is no signed update bundle ending ${ending} in ${folder}.`);
  }
  const entry = {
    signature: readFileSync(join(folder, `${name}.sig`), "utf8").trim(),
    url: `https://github.com/${repository}/releases/download/${tag}/${name}`,
  };
  for (const target of targets) platforms[target] = entry;
}

const manifest = {
  version,
  notes: `https://github.com/${repository}/releases/tag/${tag}`,
  pub_date: new Date().toISOString(),
  platforms,
};
writeFileSync(out, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`${out}: ${Object.keys(platforms).length} targets for version ${version}`);
