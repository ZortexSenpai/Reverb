import { describe, expect, it } from "vitest";
import { bumpFor, latestStable, parseCommit, planRelease, releaseNotes } from "./next-version.mjs";

let counter = 0;
/** A parsed commit from a message, with a made-up 40-character hash. */
function commit(message) {
  counter += 1;
  return parseCommit(String(counter).padStart(40, "a"), message);
}

describe("parseCommit", () => {
  it("reads the type, scope and subject", () => {
    expect(parseCommit("abc", "fix(queue): stop the queue lagging\n\nLonger body.")).toEqual({
      hash: "abc",
      type: "fix",
      scope: "queue",
      subject: "stop the queue lagging",
      breaking: null,
    });
  });

  it("reads `!` as breaking", () => {
    expect(parseCommit("abc", "feat!: drop Debian 12").breaking).toBe("drop Debian 12");
  });

  it.each(["BREAKING CHANGE", "BREAKING-CHANGE"])("reads a %s footer as breaking", (footer) => {
    const parsed = parseCommit("abc", `feat: newer runner\n\nBody.\n\n${footer}: glibc 2.39 or newer`);
    expect(parsed.breaking).toBe("glibc 2.39 or newer");
  });

  it.each(["Merge pull request #33 from someone/branch", "fixed the thing", "fix:no space", ""])(
    "returns null for %j",
    (message) => {
      expect(parseCommit("abc", message)).toBeNull();
    },
  );
});

describe("bumpFor", () => {
  it.each([
    [["fix: a"], "patch"],
    [["perf: a"], "patch"],
    [["fix: a", "feat: b"], "minor"],
    [["fix: a", "feat: b", "refactor!: c"], "major"],
    [["chore: a\n\nBREAKING CHANGE: settings reset"], "major"],
    [["docs: a", "chore: b", "ci: c", "build: d", "refactor: e", "test: f"], null],
    [[], null],
  ])("%j → %s", (messages, bump) => {
    expect(bumpFor(messages.map(commit))).toBe(bump);
  });
});

describe("latestStable", () => {
  it("picks the highest stable tag by version, not by name", () => {
    expect(latestStable(["v1.9.0", "v1.10.0", "v1.2.3", "v2.0.0-rc.1"])).toBe("v1.10.0");
  });

  it("ignores pre-releases and anything that isn't a version tag", () => {
    expect(latestStable(["v1.0.0-rc.2", "vnext", "1.4.0"])).toBeNull();
  });
});

describe("planRelease", () => {
  const tags = ["v1.0.0-rc.1", "v1.0.0-rc.2", "v1.0.0"];

  it.each([
    ["fix: a", "1.0.1"],
    ["feat: a", "1.1.0"],
    ["feat!: a", "2.0.0"],
  ])("bumps v1.0.0 for %j to %s", (message, version) => {
    const plan = planRelease({ base: "v1.0.0", commits: [commit(message)], tags });

    expect(plan).toMatchObject({ version, tag: `v${version}`, previousTag: "v1.0.0", prerelease: false });
  });

  it("releases nothing when no commit calls for it", () => {
    const plan = planRelease({ base: "v1.0.0", commits: [commit("docs: a")], tags });

    expect(plan.version).toBeNull();
    expect(plan.reason).toContain("since v1.0.0");
  });

  it("numbers pre-releases after the ones already tagged", () => {
    const plan = planRelease({
      base: "v1.0.0",
      commits: [commit("feat: a")],
      tags: [...tags, "v1.1.0-rc.1", "v1.1.0-rc.2", "v1.1.0-beta.7"],
      prerelease: "rc",
    });

    expect(plan).toMatchObject({ version: "1.1.0-rc.3", tag: "v1.1.0-rc.3", prerelease: true });
  });

  it("starts a pre-release at .1", () => {
    const plan = planRelease({ base: "v1.0.0", commits: [commit("fix: a")], tags, prerelease: "rc" });

    expect(plan.version).toBe("1.0.1-rc.1");
  });

  it("doesn't tag the same commit as a second pre-release", () => {
    const plan = planRelease({
      base: "v1.0.0",
      commits: [commit("fix: a")],
      tags: [...tags, "v1.0.1-rc.1"],
      headTags: ["v1.0.1-rc.1"],
      prerelease: "rc",
    });

    expect(plan.version).toBeNull();
    expect(plan.reason).toContain("already v1.0.1-rc.1");
  });

  it("refuses a pre-release identifier that would break the version", () => {
    expect(() => planRelease({ base: "v1.0.0", commits: [commit("fix: a")], tags, prerelease: "rc.1" })).toThrow(
      /pre-release identifier/,
    );
  });

  it("refuses a tag that already exists elsewhere", () => {
    expect(() =>
      planRelease({ base: "v1.0.0", commits: [commit("fix: a")], tags: [...tags, "v1.0.1"] }),
    ).toThrow(/v1\.0\.1 already exists/);
  });

  it("starts from 0.0.0 without a stable tag", () => {
    expect(planRelease({ base: null, commits: [commit("feat: a")], tags: [] }).version).toBe("0.1.0");
  });
});

describe("releaseNotes", () => {
  const repoUrl = "https://github.com/owner/repo";

  it("lists breaking changes, then features, fixes and performance, with links", () => {
    const commits = [
      commit("fix(queue): stop the queue lagging"),
      commit("feat: recently played tab (#19)"),
      commit("docs: glossary"),
      commit("perf: faster scrolling"),
      commit("feat!: newer Linux runner\n\nBREAKING CHANGE: needs glibc 2.39"),
    ];
    const notes = releaseNotes({
      version: "2.0.0",
      tag: "v2.0.0",
      previousTag: "v1.0.0",
      commits,
      date: "2026-11-10",
      repoUrl,
    });
    const link = (c) => `[${c.hash.slice(0, 7)}](${repoUrl}/commit/${c.hash})`;

    expect(notes).toBe(
      [
        `## [2.0.0](${repoUrl}/compare/v1.0.0...v2.0.0) (2026-11-10)`,
        "",
        "### ⚠ BREAKING CHANGES",
        "",
        `* needs glibc 2.39 (${link(commits[4])})`,
        "",
        "### Features",
        "",
        `* recently played tab ([#19](${repoUrl}/issues/19)) (${link(commits[1])})`,
        `* newer Linux runner (${link(commits[4])})`,
        "",
        "### Bug Fixes",
        "",
        `* **queue:** stop the queue lagging (${link(commits[0])})`,
        "",
        "### Performance Improvements",
        "",
        `* faster scrolling (${link(commits[3])})`,
        "",
      ].join("\n"),
    );
  });

  it("opens with a heading the app's release notes panel strips", () => {
    const notes = releaseNotes({
      version: "1.0.1",
      tag: "v1.0.1",
      previousTag: null,
      commits: [commit("fix: a")],
      date: "2026-11-10",
      repoUrl: null,
    });

    expect(notes).toMatch(/^## 1\.0\.1 \(2026-11-10\)\n\n### Bug Fixes\n\n\* a \(\w{7}\)\n$/);
    // The same pattern as withoutVersionHeading in src/components/update/ReleaseNotes.tsx.
    expect(notes.replace(/^\s*#{1,6}\s*\[?v?\d+\.\d+\.\d+[^\n]*\n*/, "")).toMatch(/^### Bug Fixes/);
  });
});
