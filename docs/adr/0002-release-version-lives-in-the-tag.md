---
status: accepted
---

# A release's version lives in its tag, not in the repository

Every push to `main` that carries a `feat:`, `fix:`, `perf:` or breaking commit releases straight away. The version is worked out from the conventional commits since the last stable `vX.Y.Z` tag, the pushed commit gets the tag, and the release builds stamp that version into `package.json`, `package-lock.json`, `Cargo.toml` and `Cargo.lock` before building. Nothing commits the version back to `main`, so those files hold the placeholder 0.0.0. This replaces release-please, whose release PR had to be merged by hand before anything shipped. Committing a bump straight to `main` from CI isn't possible either: the `main` ruleset requires CI checks on every commit and lets nobody bypass them. Weakening that rule, or keeping a bot PR that merges itself, would have been the price of an accurate version in the repository.

## Consequences

Local and dev builds report version 0.0.0. `CHANGELOG.md` stops at 1.0.0, and the notes on each GitHub release (which the app shows before updating) take its place. A release whose build fails keeps its tag, so the next release skips past that version.
