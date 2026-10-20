#!/usr/bin/env bash
# Build the rpm and install it, choosing install vs reinstall automatically.
#
# The trap this exists to avoid: the app version (the Cargo workspace's) and
# the rpm `release` (default "1") together decide the NVR, so every local
# build of an unchanged version produces the *same* NVR. `dnf install` treats an
# already-installed NVR as a no-op — it prints "Nothing to do", exits 0, and
# your new build is simply not installed. That reads as a broken build when it
# is really a packaging no-op. So dispatch on what is actually installed:
#
#   not installed          -> dnf install
#   installed, same NVR    -> dnf reinstall   (install would no-op)
#   installed, other NVR   -> dnf install, falling back to downgrade
#
# Usage:
#   scripts/install-local.sh                # build, then install/reinstall
#   scripts/install-local.sh --skip-build   # install whatever is already built
#
# Note this rpm is only usable on the machine that built it (or newer) — the
# binary picks up the local glibc floor, and it is unsigned, so it can't
# update itself. Releases come from the release workflow (RELEASING.md).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLE_DIR="$REPO_ROOT/target/release/bundle/rpm"
PKG_NAME="reverb"

SKIP_BUILD=0
for arg in "$@"; do
  case "$arg" in
    --skip-build) SKIP_BUILD=1 ;;
    # Print the header comment, stopping at the first line that is not one.
    -h|--help) awk 'NR>1 && /^#/ {sub(/^# ?/, ""); print; next} NR>1 {exit}' \
                 "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "unknown option: $arg (try --help)" >&2; exit 2 ;;
  esac
done

info() { printf '\n==> %s\n' "$*"; }
die()  { printf 'error: %s\n' "$*" >&2; exit 1; }

for tool in rpm dnf; do
  command -v "$tool" >/dev/null || die "$tool not found — this script is for rpm-based distros"
done

if [[ $SKIP_BUILD -eq 0 ]]; then
  info "Building rpm bundle"
  # Runs the vite build via beforeBuildCommand, then a release cargo build.
  # Slow even for config-only changes: lto=true forces a full relink.
  # --no-sign: the updater signing key lives in CI only, and without it the
  # build would fail at the signing step.
  (cd "$REPO_ROOT" && npm run tauri build -- --bundles rpm --no-sign)
fi

# Glob rather than construct the filename, so a version bump needs no edit here.
# Newest by mtime, in case older versions are still lying around.
RPM_FILE="$(find "$BUNDLE_DIR" -maxdepth 1 -name '*.rpm' -printf '%T@ %p\n' 2>/dev/null \
  | sort -rn | head -1 | cut -d' ' -f2-)"
[[ -n "$RPM_FILE" ]] || die "no rpm found in $BUNDLE_DIR — run without --skip-build"

NVR_FMT='%{NAME}-%{VERSION}-%{RELEASE}.%{ARCH}'
NEW_NVR="$(rpm -qp --queryformat "$NVR_FMT" "$RPM_FILE" 2>/dev/null)"

# `rpm -q` on a missing package prints "package foo is not installed" on
# *stdout* and exits 1, so the exit status is the only safe signal — capturing
# output alone would leave that sentence in the variable.
installed_nvr() {
  local nvr
  if nvr="$(rpm -q --queryformat "$NVR_FMT" "$PKG_NAME" 2>/dev/null)"; then
    printf '%s' "$nvr"
  fi
}
INSTALLED_NVR="$(installed_nvr)"

info "Built:     $NEW_NVR ($(du -h "$RPM_FILE" | cut -f1))"
printf '    Installed: %s\n' "${INSTALLED_NVR:-(not installed)}"

# A running instance keeps executing the old binary via its open inode, so the
# reinstall looks like it did nothing. Warn rather than kill — this is someone's
# music player, and stopping it is their call.
if pgrep -x "$PKG_NAME" >/dev/null 2>&1; then
  printf '\n    note: %s is running (pid %s) — quit and relaunch it after this,\n' \
    "$PKG_NAME" "$(pgrep -x "$PKG_NAME" | tr '\n' ' ' | sed 's/ $//')"
  printf '          or it will keep running the old build.\n'
fi

if [[ -z "$INSTALLED_NVR" ]]; then
  info "Not installed — installing"
  sudo dnf install -y "$RPM_FILE"
elif [[ "$INSTALLED_NVR" == "$NEW_NVR" ]]; then
  info "Same NVR already installed — reinstalling"
  sudo dnf reinstall -y "$RPM_FILE"
else
  info "Version changed — upgrading"
  # dnf refuses to move backwards, which is what a bumped-down version looks
  # like; that case needs `downgrade` explicitly.
  sudo dnf install -y "$RPM_FILE" || sudo dnf downgrade -y "$RPM_FILE"
fi

NOW_NVR="$(installed_nvr)"
[[ -n "$NOW_NVR" ]] || die "install reported success but $PKG_NAME is not in the rpm db"
info "Done — $NOW_NVR installed"
printf '    binary: /usr/bin/%s\n' "$PKG_NAME"
# rpm only owns /usr, so settings, stats and the keychain entry are untouched
# by any of the paths above.
printf '    app data left as-is: ~/.config, ~/.local/share, ~/.cache/dev.zortexsenpai.reverb\n\n'
