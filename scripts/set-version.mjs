#!/usr/bin/env node
// Write a release's version into every file that holds Reverb's version.
//
// In the repository these files all hold the placeholder 0.0.0, because a
// release's version lives only in its git tag (docs/adr/0002). Each release
// build runs this with the tag's version before it builds, so the app, its
// bundles and latest.json agree on what they are. The files are the ones
// check-versions.mjs reads: package.json, package-lock.json, the Cargo
// workspace version, and both workspace crates in Cargo.lock. The Tauri
// config has no version of its own; it inherits the Cargo one.
//
//     node scripts/set-version.mjs 1.4.2

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;
const CRATES = ["reverb", "reverb-core"];

/** npm writes its JSON files with two-space indents and a trailing newline. */
function json(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

/** `manifest` with the `version` in its `[workspace.package]` table set to `version`. */
function withWorkspaceVersion(manifest, version) {
  const table = /^\[workspace\.package\]\s*$([\s\S]*?)(?=^\[|(?![\s\S]))/m.exec(manifest);
  const line = /^version\s*=\s*"[^"]*"/m;
  if (!table || !line.test(table[1])) {
    throw new Error("Cargo.toml has no version in [workspace.package].");
  }
  const start = table.index + table[0].length - table[1].length;
  const body = table[1].replace(line, `version = "${version}"`);
  return manifest.slice(0, start) + body + manifest.slice(start + table[1].length);
}

/** `lockfile` with the workspace crates' `[[package]]` entries at `version`. */
function withLockedVersions(lockfile, version) {
  const found = new Set();
  const entries = lockfile.split(/^(?=\[\[package\]\]\s*$)/m).map((entry) => {
    const name = /^name = "([^"]+)"$/m.exec(entry)?.[1];
    if (!CRATES.includes(name)) return entry;
    found.add(name);
    return entry.replace(/^version = "[^"]*"$/m, `version = "${version}"`);
  });
  const missing = CRATES.filter((name) => !found.has(name));
  if (missing.length) throw new Error(`Cargo.lock has no entry for ${missing.join(" or ")}.`);
  return entries.join("");
}

/**
 * Set every version to `version`, reading and writing through `read(path)`
 * and `write(path, contents)` (paths relative to the repo root).
 */
export function setVersion(read, write, version) {
  if (!SEMVER.test(version)) throw new Error(`"${version}" isn't a version like 1.4.2 or 1.5.0-rc.1.`);

  const pkg = JSON.parse(read("package.json"));
  const lock = JSON.parse(read("package-lock.json"));
  if (!lock.packages?.[""]) throw new Error('package-lock.json has no packages[""] entry.');
  pkg.version = version;
  lock.version = version;
  lock.packages[""].version = version;

  // Work everything out before writing anything, so a failure leaves no file half-done.
  const files = {
    "package.json": json(pkg),
    "package-lock.json": json(lock),
    "Cargo.toml": withWorkspaceVersion(read("Cargo.toml"), version),
    "Cargo.lock": withLockedVersions(read("Cargo.lock"), version),
  };
  for (const [path, contents] of Object.entries(files)) write(path, contents);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const root = join(dirname(fileURLToPath(import.meta.url)), "..");
  const version = process.argv[2] ?? "";
  try {
    setVersion(
      (path) => readFileSync(join(root, path), "utf8"),
      (path, contents) => writeFileSync(join(root, path), contents),
      version,
    );
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
  console.log(`Set every version to ${version}.`);
}
