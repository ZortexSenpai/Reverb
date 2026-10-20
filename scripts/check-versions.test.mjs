import { describe, expect, it } from "vitest";
import { checkVersions } from "./check-versions.mjs";

/** A trimmed-down repo where every version says `v`, with `edit` applied on top. */
function repo(v = "1.2.3", edit = {}) {
  const files = {
    "package.json": JSON.stringify({ name: "reverb", version: v }),
    "package-lock.json": JSON.stringify({
      name: "reverb",
      version: v,
      lockfileVersion: 3,
      packages: { "": { name: "reverb", version: v } },
    }),
    "Cargo.toml": [
      "[workspace]",
      'members = ["src-tauri", "crates/reverb-core"]',
      "",
      "[workspace.package]",
      `version = "${v}"`,
      'edition = "2021"',
      "",
      "[workspace.dependencies]",
      'serde = { version = "1", features = ["derive"] }',
    ].join("\n"),
    "Cargo.lock": [
      "[[package]]",
      'name = "reqwest"',
      'version = "0.13.1"',
      "",
      "[[package]]",
      'name = "reverb"',
      `version = "${v}"`,
      "dependencies = [",
      ' "reverb-core",',
      "]",
      "",
      "[[package]]",
      'name = "reverb-core"',
      `version = "${v}"`,
    ].join("\n"),
    ...edit,
  };
  return (path) => {
    if (!(path in files)) throw new Error(`no such file: ${path}`);
    return files[path];
  };
}

describe("checkVersions", () => {
  it("passes when every version agrees", () => {
    const result = checkVersions(repo("1.2.3"));

    expect(result.ok).toBe(true);
    expect(new Set(result.versions.map(([, version]) => version))).toEqual(new Set(["1.2.3"]));
  });

  it.each([
    ["package.json", "package.json", JSON.stringify({ name: "reverb", version: "1.2.4" })],
    [
      "the Cargo workspace version",
      "Cargo.toml",
      '[workspace.package]\nversion = "1.2.4"\n\n[workspace.dependencies]\nserde = { version = "1.2.3" }',
    ],
    [
      "the app crate in Cargo.lock",
      "Cargo.lock",
      '[[package]]\nname = "reverb"\nversion = "1.2.4"\n\n[[package]]\nname = "reverb-core"\nversion = "1.2.3"',
    ],
    [
      "the engine crate in Cargo.lock",
      "Cargo.lock",
      '[[package]]\nname = "reverb"\nversion = "1.2.3"\n\n[[package]]\nname = "reverb-core"\nversion = "1.2.4"',
    ],
  ])("fails when %s disagrees", (_what, path, contents) => {
    const result = checkVersions(repo("1.2.3", { [path]: contents }));

    expect(result.ok).toBe(false);
    expect(result.versions).toContainEqual([expect.stringContaining(path), "1.2.4"]);
  });

  it("fails when a version is missing altogether", () => {
    const result = checkVersions(
      repo("1.2.3", { "Cargo.lock": '[[package]]\nname = "reverb"\nversion = "1.2.3"' }),
    );

    expect(result.ok).toBe(false);
    expect(result.versions).toContainEqual(["Cargo.lock: reverb-core", null]);
  });
});
