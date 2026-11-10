#!/usr/bin/env node
// Fail when the places Reverb's version is written down disagree.
//
// The version lives in package.json, package-lock.json, the Cargo workspace
// version and both workspace crates in Cargo.lock. In the repository they
// all hold the placeholder 0.0.0; each release build stamps its tag's
// version into all of them with set-version.mjs, then runs this. A file that
// was missed would build a release whose parts disagree about what they are,
// so CI also runs it on every push and PR. The Tauri config has no version
// of its own; it inherits the Cargo one.
//
//     node scripts/check-versions.mjs

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

/** `version` in the `[workspace.package]` table of a Cargo manifest. */
function workspaceVersion(manifest) {
  const table = /^\[workspace\.package\]\s*$([\s\S]*?)(?=^\[|(?![\s\S]))/m.exec(manifest);
  return table && /^version\s*=\s*"([^"]+)"/m.exec(table[1])?.[1];
}

/** The locked version of the crate `name`, from Cargo.lock's `[[package]]` entries. */
function lockedVersion(lockfile, name) {
  for (const entry of lockfile.split(/^\[\[package\]\]\s*$/m)) {
    if (new RegExp(`^name = "${name}"$`, "m").test(entry)) {
      return /^version = "([^"]+)"$/m.exec(entry)?.[1];
    }
  }
  return undefined;
}

/**
 * Read every version through `read(path)` (paths relative to the repo root).
 * `versions` pairs a label with the version found there, `null` when missing.
 */
export function checkVersions(read) {
  const pkg = JSON.parse(read("package.json"));
  const lock = JSON.parse(read("package-lock.json"));
  const cargoToml = read("Cargo.toml");
  const cargoLock = read("Cargo.lock");

  const versions = [
    ["package.json", pkg.version],
    ["package-lock.json", lock.version],
    ['package-lock.json: packages[""]', lock.packages?.[""]?.version],
    ["Cargo.toml: [workspace.package]", workspaceVersion(cargoToml)],
    ["Cargo.lock: reverb", lockedVersion(cargoLock, "reverb")],
    ["Cargo.lock: reverb-core", lockedVersion(cargoLock, "reverb-core")],
  ].map(([label, version]) => [label, version ?? null]);

  const distinct = new Set(versions.map(([, version]) => version));
  return { ok: distinct.size === 1 && !distinct.has(null), versions };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const root = join(dirname(fileURLToPath(import.meta.url)), "..");
  const { ok, versions } = checkVersions((path) => readFileSync(join(root, path), "utf8"));
  const width = Math.max(...versions.map(([label]) => label.length));
  for (const [label, version] of versions) {
    console.log(`${label.padEnd(width)}  ${version ?? "(missing)"}`);
  }
  if (!ok) {
    console.error("\nVersions disagree. They hold the placeholder 0.0.0 and only a release");
    console.error("build changes them, all together (scripts/set-version.mjs). See RELEASING.md.");
    process.exit(1);
  }
  console.log("\nAll versions agree.");
}
