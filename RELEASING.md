# Releasing Reverb

Releases are cut by merging a pull request. Nobody edits a version number by
hand, builds on their own machine, or uploads files. This page covers how
that works, what it depends on, and what to do when it goes wrong.

## How a release happens

1. **Commit to `main` with conventional commit messages.** `feat: …` and
   `fix: …` (optionally scoped, `fix(queue): …`) go into the changelog.
   Other types (`chore:`, `ci:`, `build:`, `docs:`, `refactor:`, `test:`) are
   left out of it. So are messages that don't follow the convention at all;
   they never block anything.
2. **release-please keeps a release PR open** ("chore(main): release X.Y.Z").
   It holds the next version bump and its `CHANGELOG.md` section, and every
   push to `main` updates it. The bump touches `package.json`,
   `package-lock.json`, the Cargo workspace version in `Cargo.toml`, and the
   `reverb` and `reverb-core` entries in `Cargo.lock`. `tauri.conf.json` has
   no version of its own; it inherits the Cargo one.
3. **CI runs on the release PR like on any other.** That only happens
   because release-please opens it with `RELEASE_PLEASE_TOKEN` (see
   [Secrets](#secrets)). The `main` ruleset won't let it merge until these
   checks pass:
   - Frontend tests and type check
   - Version consistency
   - Rust tests (linux)
   - Rust tests (macos)
   - Rust tests (windows)

   The version consistency check (`npm run check:versions`) fails if the
   bump missed any of the files above.
4. **Merging the release PR** makes release-please create a **draft**
   release for `vX.Y.Z`, tag included. The release body is that version's
   `CHANGELOG.md` section.
5. **In the same workflow run** (`.github/workflows/release.yml`):
   1. *Check signing key* signs a probe file with the signing secret and
      checks it against the public key in `tauri.conf.json`. A mismatch
      stops the release before any build starts.
   2. *Build* runs three jobs, each uploading its bundles, `.sig` files and
      its entries in `latest.json` to the draft:

      | Job | Runner | Uploads |
      |---|---|---|
      | macos | `macos-latest` | Universal `.dmg`, plus `.app.tar.gz` for the updater. Ad-hoc signed. |
      | windows | `windows-latest` | NSIS `-setup.exe`, a per-user install. Unsigned. |
      | linux | `ubuntu-22.04` | `.AppImage`, `.deb` and `.rpm`. Unsigned. |

   3. *Publish release* runs only if all three builds succeeded. It checks
      that `latest.json` has darwin-aarch64, darwin-x86_64, windows-x86_64
      and linux-x86_64 entries, then publishes the draft.
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

## Versioning rule

Versions are SemVer and tags are `vX.Y.Z`.

- `fix:` → **patch** (1.4.2 → 1.4.3)
- `feat:` → **minor** (1.4.2 → 1.5.0)
- **Major** (1.4.2 → 2.0.0) only for changes that break something *for
  listeners*:
  - dropping an OS or distro version (see [The Linux runner](#the-linux-runner))
  - raising the minimum Navidrome version
  - resetting user settings

  Mark these with `!` (`feat!: …`) or a `BREAKING CHANGE:` footer.
  Breaking changes to internal code don't count.

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
| `RELEASE_PLEASE_TOKEN` | A fine-grained personal access token for this repository only, with read/write access to Contents, Pull requests and Issues | Release PR |

`GITHUB_TOKEN` is provided by Actions and needs no setup.

**Why `RELEASE_PLEASE_TOKEN`:** GitHub runs no workflows for a pull request
pushed with `GITHUB_TOKEN`. Without the PAT, CI never reports on the release
PR, and the ruleset's required checks would block it forever.

- If the secret is missing, release-please falls back to `GITHUB_TOKEN`.
  Releases still work, but merging the release PR then needs the admin
  bypass, with no checks run.
- The fallback also needs **Settings → Actions → General → Allow GitHub
  Actions to create and approve pull requests** turned on.
- Fine-grained tokens expire. When this one does, the *Release PR* job fails
  with "Bad credentials". Generate a new token and replace the secret.

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
   *Publish release* runs again once all three builds are green.
2. If *Publish release* says `latest.json` is missing a platform, re-run
   that platform's build job. Each build merges its entries into
   `latest.json`, so the re-run puts the missing ones back.
3. If the failure needs a code change, a re-run can't help: it builds the
   release commit again. Skip the version instead:
   1. Leave the draft unpublished.
   2. Land the fix on `main`, and merge the next release PR (vX.Y.Z+1).
   3. Once that release is out, delete the stale draft with
      `gh release delete vX.Y.Z`. Keep its tag, because release-please's
      history counts on it.

## Fixing a bad release

Fix forward. There is no rollback or downgrade in the app.

1. Land the fix (`fix: …`) and merge the release PR. Installs on the bad
   version update to the fix like any other update.
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

To cut one through the same pipeline, set these in
`release-please-config.json` for one release PR, then remove them again:

```json
"release-as": "1.3.0-rc.1",
"prerelease": true
```

(`release-as` is deprecated upstream in favour of a `Release-As: 1.3.0-rc.1`
commit footer. Either works.)

## First release

The pipeline is proved with a release candidate before 1.0.0 ships.
`release-please-config.json` starts out set up for it.

1. Generate the signing key and store the secrets (#4). Paste the public key
   into `plugins.updater.pubkey` in `tauri.conf.json`, replacing the
   placeholder. Until then, *Check signing key* fails on purpose. Also create
   `RELEASE_PLEASE_TOKEN`, so CI runs on the release PR.
2. Merge the release PR for **v1.0.0-rc.1** (`release-as: 1.0.0-rc.1`,
   `prerelease: true`). Check that the pre-release has every bundle with its
   `.sig` and a complete `latest.json`. Then install it by hand on each OS
   (#5).
   - Watch the **rpm** here. The bundler puts the version into the rpm's
     Version field as is, and rpm doesn't normally allow `-` there. If
     `1.0.0-rc.1` won't install, that's a pre-release-only problem; 1.0.0
     doesn't have it.
3. Delete the old local tag that was never pushed (`git tag -d v1.0.0`).
   Set `release-as` to `1.0.0` and remove `prerelease`. Merge the release
   PR, and check that **v1.0.0** is GitHub's latest release (#10).
4. Remove `release-as` from the config. From here on, versions follow the
   commits.

1.0.0 is the first build with the updater. Anything installed before it,
including locally built 0.1.0 packages, has to be replaced by hand once.

## The Linux runner

Linux builds run on `ubuntu-22.04` because the binary needs at least the
glibc it was built against, and 22.04 sets that floor at **2.35**: Ubuntu
22.04, Debian 12, Fedora 36 and anything newer.

GitHub will retire that runner. Moving to `ubuntu-24.04` raises the floor to
**glibc 2.39**, which drops Debian 12 (and Ubuntu 22.04). Under the
versioning rule that is a **major** release. Ship the runner change as
`feat!:` and say in the changelog which distros it leaves behind.

## Local builds

`npm run tauri build` signs updater artifacts, so it fails without the
private key. For a local build, skip signing:

```sh
npm run tauri build -- --no-sign
```

`scripts/install-local.sh` already does. A local build is only for the
machine that built it: it carries that machine's glibc floor and can't
update itself.

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
