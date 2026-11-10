#!/usr/bin/env node
// Work out Reverb's next release from the conventional commits since the last.
//
// A release's version lives only in its git tag (docs/adr/0002). On every
// push to main, the release workflow runs this script. It reads the commits
// since the newest stable `vX.Y.Z` tag the commit descends from and decides
// whether they call for a release and what kind:
//
//   breaking (`feat!:`, or a `BREAKING CHANGE:` footer)   major
//   feat                                                   minor
//   fix, perf                                              patch
//   anything else (chore, ci, build, docs, refactor, ...)  no release
//
// Merge commits are skipped. The commits they bring in are the ones counted.
//
//     node scripts/next-version.mjs [--prerelease rc] [--notes-file notes.md]
//
// Prints the version and its release notes. Under GitHub Actions it also sets
// the step outputs `version`, `tag`, `prerelease` and `notes`. `version` is
// empty when there is nothing to release.

import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { appendFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const HEADER = /^(?<type>[a-z]+)(?:\((?<scope>[^()\r\n]*)\))?(?<bang>!)?: (?<subject>.+)$/i;
const BREAKING_FOOTER = /^BREAKING[ -]CHANGE: *(.+)$/m;
const STABLE_TAG = /^v(\d+)\.(\d+)\.(\d+)$/;

/** The changelog sections, in the order the notes list them. */
const SECTIONS = [
  { type: "feat", title: "Features" },
  { type: "fix", title: "Bug Fixes" },
  { type: "perf", title: "Performance Improvements" },
];

/**
 * A commit's conventional-commit fields, or `null` when its message doesn't
 * follow the convention. `breaking` holds what the commit says it breaks, or
 * `null` when it doesn't break anything.
 */
export function parseCommit(hash, message) {
  const [header = "", ...rest] = message.trim().split("\n");
  const match = HEADER.exec(header.trim());
  if (!match) return null;
  const { type, scope, bang, subject } = match.groups;
  const footer = BREAKING_FOOTER.exec(rest.join("\n"));
  return {
    hash,
    type: type.toLowerCase(),
    scope: scope?.trim() || null,
    subject: subject.trim(),
    breaking: footer ? footer[1].trim() : bang ? subject.trim() : null,
  };
}

/** `major`, `minor` or `patch` for these commits, or `null` if they need no release. */
export function bumpFor(commits) {
  if (commits.some((commit) => commit.breaking)) return "major";
  if (commits.some((commit) => commit.type === "feat")) return "minor";
  if (commits.some((commit) => commit.type === "fix" || commit.type === "perf")) return "patch";
  return null;
}

function parseStable(tag) {
  const match = STABLE_TAG.exec(tag);
  return match ? match.slice(1).map(Number) : null;
}

/** Negative, zero or positive as `[major, minor, patch]` `a` is older than, equal to or newer than `b`. */
function compareVersions(a, b) {
  const i = a.findIndex((part, index) => part !== b[index]);
  return i === -1 ? 0 : a[i] - b[i];
}

/** The highest stable `vX.Y.Z` tag among `tags`, or `null` if there is none. Pre-releases don't count. */
export function latestStable(tags) {
  let best = null;
  for (const tag of tags) {
    const version = parseStable(tag);
    if (version && (!best || compareVersions(version, best.version) > 0)) best = { tag, version };
  }
  return best?.tag ?? null;
}

function increment([major, minor, patch], bump) {
  if (bump === "major") return `${major + 1}.0.0`;
  if (bump === "minor") return `${major}.${minor + 1}.0`;
  return `${major}.${minor}.${patch + 1}`;
}

/**
 * What to release, given the newest stable tag the commit descends from
 * (`base`), the conventional commits since it, every tag in the repository,
 * and the tags already on this commit. With `prerelease` (say `rc`), the
 * result is the next `-rc.N` of the version those commits call for.
 *
 * `version` is `null` when nothing should be released, and `reason` says why.
 */
export function planRelease({ base, commits, tags, headTags = [], prerelease = "" }) {
  const bump = bumpFor(commits);
  if (!bump) {
    const since = base ? ` since ${base}` : "";
    return { version: null, reason: `Nothing to release: no feat, fix, perf or breaking commits${since}.` };
  }

  const next = increment(base ? parseStable(base) : [0, 0, 0], bump);
  let version = next;
  if (prerelease) {
    if (!/^[0-9A-Za-z-]+$/.test(prerelease)) {
      throw new Error(`"${prerelease}" can't be a pre-release identifier. Use letters, digits and "-", like "rc".`);
    }
    const prefix = `v${next}-${prerelease}.`;
    const already = headTags.find((tag) => tag.startsWith(prefix));
    if (already) return { version: null, reason: `Nothing to release: this commit is already ${already}.` };
    const numbers = tags
      .filter((tag) => tag.startsWith(prefix) && /^\d+$/.test(tag.slice(prefix.length)))
      .map((tag) => Number(tag.slice(prefix.length)));
    version = `${next}-${prerelease}.${Math.max(0, ...numbers) + 1}`;
  }

  const tag = `v${version}`;
  if (tags.includes(tag)) {
    throw new Error(`${tag} already exists, but this commit doesn't descend from it. See RELEASING.md.`);
  }
  const since = base ? ` since ${base}` : "";
  return {
    version,
    tag,
    previousTag: base,
    prerelease: Boolean(prerelease),
    reason: `Releasing ${tag}: a ${bump} bump for ${commits.length} commit(s)${since}.`,
  };
}

/** `#12` in a commit subject, as a link to that issue or PR. */
function linkIssues(text, repoUrl) {
  if (!repoUrl) return text;
  return text.replace(/(^|[\s(])#(\d+)\b/g, (_, before, number) => `${before}[#${number}](${repoUrl}/issues/${number})`);
}

function entry(commit, text, repoUrl) {
  const scope = commit.scope ? `**${commit.scope}:** ` : "";
  const short = commit.hash.slice(0, 7);
  const link = repoUrl ? `[${short}](${repoUrl}/commit/${commit.hash})` : short;
  return `* ${scope}${linkIssues(text, repoUrl)} (${link})`;
}

/**
 * The release notes: GitHub shows them on the release page, and the app
 * shows them before it installs an update. The app drops the opening version
 * heading (`ReleaseNotes.tsx`) because its panel already titles the version.
 */
export function releaseNotes({ version, tag, previousTag, commits, date, repoUrl }) {
  const heading =
    repoUrl && previousTag
      ? `## [${version}](${repoUrl}/compare/${previousTag}...${tag}) (${date})`
      : `## ${version} (${date})`;
  const parts = [heading];

  const breaking = commits.filter((commit) => commit.breaking);
  if (breaking.length) {
    const lines = breaking.map((commit) => entry(commit, commit.breaking, repoUrl));
    parts.push(`### ⚠ BREAKING CHANGES\n\n${lines.join("\n")}`);
  }
  for (const { type, title } of SECTIONS) {
    const lines = commits.filter((commit) => commit.type === type).map((commit) => entry(commit, commit.subject, repoUrl));
    if (lines.length) parts.push(`### ${title}\n\n${lines.join("\n")}`);
  }
  return `${parts.join("\n\n")}\n`;
}

function git(...args) {
  return execFileSync("git", args, { encoding: "utf8" });
}

function lines(text) {
  return text.split("\n").map((line) => line.trim()).filter(Boolean);
}

/** The conventional commits after `base` up to HEAD, oldest first. */
function commitsSince(base) {
  const range = base ? `${base}..HEAD` : "HEAD";
  return git("log", "--no-merges", "--reverse", "--format=%H%x1f%B%x1e", range)
    .split("\x1e")
    .map((record) => record.trim())
    .filter(Boolean)
    .map((record) => {
      const [hash, message = ""] = record.split("\x1f");
      return parseCommit(hash, message);
    })
    .filter(Boolean);
}

function setOutputs(outputs) {
  const file = process.env.GITHUB_OUTPUT;
  if (!file) return;
  for (const [name, value] of Object.entries(outputs)) {
    if (value.includes("\n")) {
      const delimiter = `EOF_${randomUUID()}`;
      appendFileSync(file, `${name}<<${delimiter}\n${value}\n${delimiter}\n`);
    } else {
      appendFileSync(file, `${name}=${value}\n`);
    }
  }
}

function main() {
  const { values } = parseArgs({
    options: { prerelease: { type: "string", default: "" }, "notes-file": { type: "string" } },
  });
  const base = latestStable(lines(git("tag", "--merged", "HEAD", "--list", "v*")));
  const commits = commitsSince(base);
  const plan = planRelease({
    base,
    commits,
    tags: lines(git("tag", "--list", "v*")),
    headTags: lines(git("tag", "--points-at", "HEAD", "--list", "v*")),
    prerelease: values.prerelease.trim(),
  });

  if (!plan.version) {
    console.log(plan.reason);
    setOutputs({ version: "", tag: "", prerelease: "false", notes: "" });
    return;
  }

  const { GITHUB_SERVER_URL: server, GITHUB_REPOSITORY: repository } = process.env;
  const notes = releaseNotes({
    ...plan,
    commits,
    date: new Date().toISOString().slice(0, 10),
    repoUrl: server && repository ? `${server}/${repository}` : null,
  });
  console.log(`${plan.reason}\n\n${notes}`);
  if (values["notes-file"]) writeFileSync(values["notes-file"], notes);
  setOutputs({ version: plan.version, tag: plan.tag, prerelease: String(plan.prerelease), notes });
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    console.error(process.env.GITHUB_ACTIONS ? `::error::${error.message}` : error.message);
    process.exit(1);
  }
}
