import { describe, expect, it } from "vitest";
import { checkVersions } from "./check-versions.mjs";
import { setVersion } from "./set-version.mjs";

const CARGO_TOML = [
  "[workspace]",
  'members = ["src-tauri", "crates/reverb-core"]',
  "",
  "[workspace.package]",
  'version = "0.0.0"',
  'edition = "2021"',
  "",
  "[workspace.dependencies]",
  'serde = { version = "1", features = ["derive"] }',
  "",
].join("\n");

const CARGO_LOCK = [
  "[[package]]",
  'name = "reqwest"',
  'version = "0.13.1"',
  "",
  "[[package]]",
  'name = "reverb"',
  'version = "0.0.0"',
  "dependencies = [",
  ' "reverb-core",',
  "]",
  "",
  "[[package]]",
  'name = "reverb-core"',
  'version = "0.0.0"',
  "",
].join("\n");

/** An in-memory copy of the version files, every one at 0.0.0, with `edit` applied on top. */
function repo(edit = {}) {
  const files = {
    "package.json": `${JSON.stringify({ name: "reverb", private: true, version: "0.0.0" }, null, 2)}\n`,
    "package-lock.json": `${JSON.stringify(
      { name: "reverb", version: "0.0.0", lockfileVersion: 3, packages: { "": { name: "reverb", version: "0.0.0" } } },
      null,
      2,
    )}\n`,
    "Cargo.toml": CARGO_TOML,
    "Cargo.lock": CARGO_LOCK,
    ...edit,
  };
  return {
    files,
    read: (path) => {
      if (!(path in files)) throw new Error(`no such file: ${path}`);
      return files[path];
    },
    write: (path, contents) => {
      files[path] = contents;
    },
  };
}

describe("setVersion", () => {
  it.each(["1.4.2", "1.5.0-rc.1"])("sets every version check-versions reads to %s", (version) => {
    const { read, write } = repo();

    setVersion(read, write, version);

    const result = checkVersions(read);
    expect(result.ok).toBe(true);
    expect(new Set(result.versions.map(([, found]) => found))).toEqual(new Set([version]));
  });

  it("changes only the version lines", () => {
    const { files, read, write } = repo();

    setVersion(read, write, "1.4.2");

    expect(files["Cargo.toml"]).toBe(CARGO_TOML.replace('version = "0.0.0"', 'version = "1.4.2"'));
    expect(files["Cargo.lock"]).toBe(CARGO_LOCK.replaceAll('version = "0.0.0"', 'version = "1.4.2"'));
    expect(files["Cargo.lock"]).toContain('name = "reqwest"\nversion = "0.13.1"');
    expect(files["package.json"]).toBe(
      `${JSON.stringify({ name: "reverb", private: true, version: "1.4.2" }, null, 2)}\n`,
    );
  });

  it.each(["v1.4.2", "1.4", "", "1.4.2-"])("refuses %j", (version) => {
    const { read, write } = repo();

    expect(() => setVersion(read, write, version)).toThrow(/isn't a version/);
  });

  it("writes nothing when a file is missing its version", () => {
    const { files, read, write } = repo({ "Cargo.lock": CARGO_LOCK.replace('name = "reverb-core"', 'name = "other"') });
    const before = { ...files };

    expect(() => setVersion(read, write, "1.4.2")).toThrow(/reverb-core/);
    expect(files).toEqual(before);
  });
});
