# Releasing Reverb

Releases are cut by merging to `main`. Nobody picks a version number, edits
one by hand, builds on their own machine, or uploads files. This page covers
how that works, what it depends on, and what to do when it goes wrong.

## How a release happens

1. **Commit with conventional commit messages.** `feat: …`, `fix: …` and
   `perf: …` (optionally scoped, `fix(queue): …`) go into the release notes.
   Other types (`chore:`, `ci:`, `build:`, `docs:`, `refactor:`, `test:`) are
   left out of them. So are messages that don't follow the convention at all;
   they never block anything. Merge commits are skipped, and the commits they
   bring in are the ones counted. A squash merge's message is the PR title,
   so give such a PR a conventional title.
2. **Merge to `main`.** The `main` ruleset only lets a commit in once these
   checks pass:
   - Frontend tests and type check
   - Version consistency
   - Rust tests (linux)
   - Rust tests (macos)
   - Rust tests (windows)
3. **Every push to `main` runs the release workflow**
   (`.github/workflows/release.yml`). Its first job, *Next version*, runs
   `scripts/next-version.mjs`. It reads the commits since the newest stable
   `vX.Y.Z` tag that `main` descends from, and picks the version by the
   [versioning rule](#versioning-rule). If none of them calls for a release
   (a push of only `docs:` or `chore:` commits, say), the run stops there.
4. **Otherwise it tags the pushed commit `vX.Y.Z`** and opens a **draft**
   release for it. The release notes list the breaking changes, features,
   bug fixes and performance improvements since the last release, each
   linked to its commit.
5. **In the same workflow run:**
   1. *Check signing key* signs a probe file with the signing secret and
      checks it against the public key in `tauri.conf.json`. A mismatch
      stops the release before any build starts.
   2. *Build* runs three jobs. Each one first writes the version into the
      files that hold it (`scripts/set-version.mjs`, then
      `npm run check:versions` to prove none was missed), then uploads its
      bundles, `.sig` files and its entries in `latest.json` to the draft:

      | Job | Runner | Uploads |
      |---|---|---|
      | macos | `macos-latest` | Universal `.dmg`, plus `.app.tar.gz` for the updater. Ad-hoc signed. |
      | windows | `windows-latest` | NSIS `-setup.exe`, a per-user install. Unsigned. |
      | linux | `ubuntu-22.04` | `.AppImage`, `.deb` and `.rpm`. Unsigned. |

   3. *Publish release* runs only if all three builds succeeded. It checks
      that `latest.json` has darwin-aarch64, darwin-x86_64, windows-x86_64
      and linux-x86_64 entries, plus the per-installer ones an install
      looks up first (`-app`, `-nsis`, `-appimage`, `-deb`, `-rpm`), then
      publishes the draft.
6. **Installs pick it up.** Publishing makes it GitHub's "latest" release,
   and every install with automatic checks on finds it within 6 hours (or
   at its next launch).

How the app finds updates: it reads
`https://github.com/ZortexSenpai/Reverb/releases/latest/download/latest.json`.
GitHub's "latest" never points at a draft or a pre-release, and that is
the only thing keeping both away from listeners; the app doesn't filter
them itself. `latest.json` points each platform at its file through the
GitHub API (`api.github.com/repos/…/releases/assets/<id>`), and the app
checks every download's signature before installing it.

### Where the version lives

Only in the release's tag. In the repository, `package.json`,
`package-lock.json`, the Cargo workspace version in `Cargo.toml` and the
`reverb` and `reverb-core` entries in `Cargo.lock` all hold the placeholder
**0.0.0**, and nothing commits a version back to `main`. Leave them at
0.0.0. The *Version consistency* check fails if they disagree, because
`set-version.mjs` relies on all of them being there. `tauri.conf.json` has
no version of its own; it inherits the Cargo one. The reasons are in
`docs/adr/0002-release-version-lives-in-the-tag.md`.

### Release notes

The notes live on the GitHub release, and the app shows them before it
installs an update. The app reads them from `latest.json`, which copied them
at build time, so editing a published release's notes on GitHub doesn't
change what installs show. `CHANGELOG.md` stops at 1.0.0.

## Versioning rule

Versions are SemVer and tags are `vX.Y.Z`. The release takes the biggest
bump that any commit since the last release calls for.

- `fix:` or `perf:` → **patch** (1.4.2 → 1.4.3)
- `feat:` → **minor** (1.4.2 → 1.5.0)
- **Major** (1.4.2 → 2.0.0) only for changes that break something *for
  listeners*:
  - dropping an OS or distro version (see [The Linux runner](#the-linux-runner))
  - raising the minimum Navidrome version
  - resetting user settings

  Mark these with `!` (`feat!: …`) or a `BREAKING CHANGE:` footer. Either
  one makes a major release whatever the type, so don't use them for
  breaking changes to internal code.
- Anything else → **no release**. The commits wait and go out with the next
  release that has a `feat:`, `fix:` or `perf:`.

Settings must stay readable across updates: new versions only add fields,
each with a default. Changing an existing field's type needs a deliberate
migration (see `src-tauri/src/persistence/settings.rs`). If an update can't
parse the settings file, it moves it aside as `settings.json.bak-<timestamp>`
and starts from defaults, so nothing is lost, but the listener's setup is
gone until they restore it.

## Secrets

| Secret | Holds | Used by |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | The updater signing key (from `tauri signer generate`) | Check signing key, Build |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Its password | Check signing key, Build |

`GITHUB_TOKEN` is provided by Actions and needs no setup. It creates the
tag and the draft. (`RELEASE_PLEASE_TOKEN`, from when release-please cut
releases, is no longer used and can be deleted.)

The **public** key is not a secret. It lives in `src-tauri/tauri.conf.json`
under `plugins.updater.pubkey`, and every install checks downloads against
the copy it shipped with.

**Backup:** the private key and its password are also kept in the
maintainer's password manager. They are never committed and never stored
anywhere else. GitHub won't show a secret again once it's saved, so the
password manager is the only copy you can read.

## Losing the signing key

**Losing the private key strands every install.** Installs only accept
updates signed with the key whose public half they shipped with, so a new
key pair can't be used to reach them. No existing install could ever update
again. Every listener would have to download and reinstall Reverb by hand,
and the only way to tell them is outside the app.

So:

- keep the password-manager backup
- never rotate the key casually
- if the GitHub secret is lost but the backup isn't, restore the secret
  from the backup

The *Check signing key* job exists because the Tauri CLI only *warns* when
the signing key doesn't match the shipped public key. A release signed with
the wrong key would build fine and then be refused by every install.

## When a build fails

A failed build job leaves the release a **draft**. Nothing is published, and
installs keep seeing the previous release. To recover:

1. Open the failed run in **Actions → Release** and use **Re-run failed
   jobs**. tauri-action replaces any files the failed attempt uploaded, and
   *Publish release* runs again once all three builds are green. A job that
   failed with "The job was not acquired by Runner" never ran at all
   (GitHub had no runner free), so a re-run is all it needs. Don't use
   *Re-run all jobs*: *Next version* would find the commit already tagged
   and release nothing.
2. If *Publish release* says `latest.json` is missing a platform, re-run
   that platform's build job. Each build merges its entries into
   `latest.json`, so the re-run puts the missing ones back.
3. If the failure needs a code change, a re-run can't help: it builds the
   release commit again. Skip the version instead:
   1. Leave the draft unpublished.
   2. Land the fix on `main` as a `fix:`. Its push releases vX.Y.Z+1 on its
      own: the failed version's tag already exists, so the next release
      counts from it.
   3. Once that release is out, delete the stale draft with
      `gh release delete vX.Y.Z`. Keep its tag, because the next version is
      worked out from it.
4. If *Next version* failed after pushing the tag but before opening the
   draft, there's nothing to skip. Delete the tag
   (`git push origin :refs/tags/vX.Y.Z`) and re-run the job.

## Fixing a bad release

Fix forward. There is no rollback or downgrade in the app.

1. Land the fix (`fix: …`) on `main`. It releases on its own, and installs
   on the bad version update to the fix like any other update.
2. Optionally, mark the bad release as a **pre-release**:

   ```sh
   gh release edit vX.Y.Z --prerelease
   ```

   GitHub's "latest" then falls back to the previous release, so installs
   that haven't taken the bad one yet stop being offered it. Installs
   already on it aren't moved back (the updater never downgrades); they wait
   for the fix.

## Pre-releases

Pre-releases are for installing by hand, to try a build before it reaches
anyone. The updater never offers them, because GitHub's "latest" skips
pre-releases.

To cut one, open **Actions → Release → Run workflow**, pick the branch, and
enter a pre-release identifier such as `rc`. The version is the one the
branch's commits call for, plus `-rc.N`, with N one past the `rc` tags that
version already has. A branch with a `feat:` since v1.2.0 gives
`v1.3.0-rc.1`, and running it again after more commits gives `v1.3.0-rc.2`.
When the branch merges, the push to `main` releases `v1.3.0` as usual, with
notes covering everything since v1.2.0.

Run it on the branch before merging. `main` releases every releasable push
straight away, so there's no unreleased work left on `main` to pre-release.
Left empty on `main`, the identifier retries a normal release (for a run
that was cancelled, say). Left empty on any other branch, the run refuses.

The Linux build of a pre-release takes about 5 minutes longer. Tauri can't
bundle an `.rpm` for a version with a `-` in it yet
([tauri-apps/tauri#16336](https://github.com/tauri-apps/tauri/pull/16336)),
so the job builds the Tauri CLI from source with that fix applied
(`.github/patches/tauri-rpm-prerelease-version.patch`). The rpm's Version
tag says `1.3.0~rc.1`, which rpm sorts before `1.3.0`, so the `rpm -U` the
updater runs takes 1.3.0 over it. The file name keeps the `-`. Once a Tauri
release has the fix, update `@tauri-apps/cli` and delete the patch, the
workflow step that applies it, and `tauriScript`. Until then, that step
fails the build if the patch stops applying.

The `.deb` gets it the other way round: dpkg reads `1.3.0-rc.1` as
`1.3.0` packaging revision `rc.1`, which sorts *after* `1.3.0`. The updater
still installs 1.3.0 over it (it runs `dpkg -i`, which only warns), but apt
treats installing 1.3.0 by hand as a downgrade.

## Before 1.0.1

Up to 1.0.0, release-please cut releases from a release PR, and kept
`CHANGELOG.md`. The v1.0.0-rc.1 tag has no release: its Linux build failed,
and the version was skipped.

1.0.0 is the first build with the updater. Anything installed before it,
including locally built 0.1.0 packages, has to be replaced by hand once.

## The Linux runner

Linux builds run on `ubuntu-22.04` because the binary needs at least the
glibc it was built against, and 22.04 sets that floor at **2.35**: Ubuntu
22.04, Debian 12, Fedora 36 and anything newer.

GitHub will retire that runner. Moving to `ubuntu-24.04` raises the floor to
**glibc 2.39**, which drops Debian 12 (and Ubuntu 22.04). Under the
versioning rule that is a **major** release. Ship the runner change as
`feat!:`, with a `BREAKING CHANGE:` footer saying which distros it
leaves behind, so the release notes say it too.

## Local builds

`npm run tauri build` signs updater artifacts, so it fails without the
private key. For a local build, skip signing:

```sh
npm run tauri build -- --no-sign
```

`scripts/install-local.sh` already does. A local build is only for the
machine that built it: it carries that machine's glibc floor and can't
update itself. It also says it is version 0.0.0, the repository's
placeholder ([Where the version lives](#where-the-version-lives)).

## Platform findings

Answers from updating a real install 1.0.0 → 1.0.1 on each OS (#10).

- **macOS:** does Reverb ask for Keychain access again after an
  ad-hoc-signed update?
  - *Not yet tested.*
- **Windows:** does SmartScreen stay quiet for an installer Reverb
  downloaded itself?
  - *Not yet tested.*
- **Linux (.deb / .rpm):** is the `pkexec` password prompt acceptable?
  - *Not yet tested.*
