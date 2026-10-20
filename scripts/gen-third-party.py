#!/usr/bin/env python3
"""Regenerate THIRD-PARTY-LICENSES.md from the dependencies that ship in the app.

Reverb is GPL-3.0, but almost everything it bundles is permissive (MIT /
Apache-2.0 / BSD) or MPL-2.0. Those licences let us use the code freely but
require that we pass their copyright notices and licence texts on to whoever
receives a build. This collects them.

Scope is deliberately "what ends up in the binary":
  * Rust  — `cargo tree -e normal` across all targets, so the Windows/macOS
            crates are covered too. Build-only and dev-only crates are out.
  * npm   — package.json "dependencies". The devDependencies (vite, tailwind,
            typescript, lightningcss…) are build tooling and ship no code.

Licence texts come from the crates/packages as vendored on this machine, so
they are verbatim rather than templated. Run after changing dependencies:

    python3 scripts/gen-third-party.py
"""

from __future__ import annotations

import glob
import hashlib
import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "THIRD-PARTY-LICENSES.md"
LICENSE_GLOBS = ("LICENSE*", "LICENCE*", "COPYING*", "NOTICE*", "license*", "UNLICENSE*")


def run(cmd: list[str]) -> str:
    return subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, check=True).stdout


def rust_packages() -> list[tuple[str, str]]:
    """(name, version) for every crate linked into the binary, any target."""
    out = run(["cargo", "tree", "-e", "normal", "--workspace", "--prefix", "none",
               "--target", "all"])
    seen: set[tuple[str, str]] = set()
    for line in out.splitlines():
        line = re.sub(r" \(proc-macro\)$", "", line.strip())
        line = re.sub(r" \(/.*\)$", "", line)  # workspace members carry a path
        parts = line.split()
        if len(parts) >= 2 and re.match(r"^v[0-9]", parts[1]):
            seen.add((parts[0], parts[1][1:]))
    # The workspace's own crates are the GPL'd work, not third-party.
    return sorted(p for p in seen if p[0] not in {"reverb", "reverb-core"})


def crate_dir(name: str, version: str) -> pathlib.Path | None:
    for root in glob.glob(str(pathlib.Path.home() / ".cargo/registry/src/*")):
        d = pathlib.Path(root) / f"{name}-{version}"
        if d.is_dir():
            return d
    return None


def crate_license_id(name: str, version: str, meta: dict) -> str:
    return meta.get(f"{name} {version}", "(not declared)")


def npm_packages() -> list[tuple[str, str]]:
    deps = json.loads((ROOT / "package.json").read_text()).get("dependencies", {})
    out = []
    for name in sorted(deps):
        pj = ROOT / "node_modules" / name / "package.json"
        if pj.is_file():
            out.append((name, json.loads(pj.read_text()).get("version", "?")))
        else:
            out.append((name, "?"))
    return out


def npm_dir(name: str) -> pathlib.Path | None:
    d = ROOT / "node_modules" / name
    return d if d.is_dir() else None


def read_licenses(d: pathlib.Path | None) -> list[tuple[str, str]]:
    """[(filename, text)] for the licence-ish files a package ships."""
    if d is None:
        return []
    files: list[pathlib.Path] = []
    for pat in LICENSE_GLOBS:
        files.extend(p for p in d.glob(pat) if p.is_file())
    picked: list[tuple[str, str]] = []
    for f in sorted(set(files)):
        try:
            text = f.read_text(encoding="utf-8", errors="replace").strip()
        except OSError:
            continue
        if text:
            picked.append((f.name, text))
    return picked


def normalise(text: str) -> str:
    """Group key that ignores the copyright line, so N near-identical MIT
    texts collapse into one block with N copyright notices listed."""
    body = [ln for ln in text.splitlines() if not re.match(r"\s*(Copyright|\(c\))", ln, re.I)]
    return re.sub(r"\s+", " ", " ".join(body)).strip().lower()


def copyrights(text: str) -> list[str]:
    return [ln.strip() for ln in text.splitlines()
            if re.match(r"\s*(Copyright|\(c\))", ln, re.I) and len(ln.strip()) > 12]


def main() -> int:
    meta = json.loads(run(["cargo", "metadata", "--format-version", "1", "--all-features"]))
    declared = {f"{p['name']} {p['version']}": (p.get("license") or "(not declared)")
                for p in meta["packages"]}

    rust = rust_packages()
    npm = npm_packages()

    # group key -> {"text": verbatim sample, "packages": [...], "copyrights": {...}}
    groups: dict[str, dict] = {}
    no_text: list[tuple[str, str, str]] = []

    def collect(label: str, name: str, version: str, d: pathlib.Path | None, lic: str) -> None:
        found = read_licenses(d)
        if not found:
            no_text.append((label, f"{name} {version}", lic))
            return
        for _fname, text in found:
            key = normalise(text)
            if not key:
                continue
            g = groups.setdefault(key, {"text": text, "packages": set(), "copyrights": set()})
            g["packages"].add(f"{name} {version} ({lic})")
            g["copyrights"].update(copyrights(text))

    for name, version in rust:
        collect("rust", name, version, crate_dir(name, version),
                declared.get(f"{name} {version}", "(not declared)"))
    for name, version in npm:
        d = npm_dir(name)
        lic = "(not declared)"
        if d and (d / "package.json").is_file():
            raw = json.loads((d / "package.json").read_text()).get("license")
            lic = raw if isinstance(raw, str) else lic
        collect("npm", name, version, d, lic)

    ordered = sorted(groups.values(), key=lambda g: (-len(g["packages"]), sorted(g["packages"])[0]))

    L: list[str] = []
    L.append("# Third-party licences\n")
    L.append("Reverb itself is licensed under the GNU General Public License v3.0 "
             "(see `LICENSE`). It bundles the third-party components listed here, "
             "whose licences require that their copyright notices and terms travel "
             "with any distributed build.\n")
    L.append(f"This file is generated — run `python3 scripts/gen-third-party.py` after "
             f"changing dependencies. It covers {len(rust)} Rust crates "
             f"(`cargo tree -e normal`, all targets) and {len(npm)} npm packages "
             f"(runtime `dependencies`). Build-only tooling is excluded because it "
             f"contributes no code to the shipped binary.\n")

    # MPL-2.0 §3.2 additionally requires telling recipients how to get the
    # source of the covered files, which the permissive licences do not.
    mpl = sorted({f"{n} {v}" for n, v in rust
                  if "MPL" in declared.get(f"{n} {v}", "")}
                 | {f"{n} {v}" for n, v in npm
                    if (npm_dir(n) and (npm_dir(n) / "package.json").is_file()
                        and "MPL" in str(json.loads((npm_dir(n) / "package.json").read_text())
                                        .get("license", "")))})
    if mpl:
        L.append("## Source availability for MPL-2.0 components\n")
        L.append("The Mozilla Public License 2.0 requires that recipients of a binary be "
                 "told how to obtain the source of the covered files. The components below "
                 "are used **unmodified**; their source for the exact versions bundled here "
                 "is published on crates.io / npm and at each project's upstream "
                 "repository, linked from its package page.\n")
        for pkg in mpl:
            L.append(f"- {pkg}")
        L.append("")

    L.append("## Bundled Rust crates\n")
    for name, version in rust:
        L.append(f"- {name} {version} — {declared.get(f'{name} {version}', '(not declared)')}")
    L.append("")

    L.append("## Bundled npm packages\n")
    for name, version in npm:
        d = npm_dir(name)
        lic = "(not declared)"
        if d and (d / "package.json").is_file():
            raw = json.loads((d / "package.json").read_text()).get("license")
            lic = raw if isinstance(raw, str) else lic
        L.append(f"- {name} {version} — {lic}")
    L.append("")

    if no_text:
        L.append("## Packages that ship no licence file\n")
        L.append("These declare a licence in their manifest but include no licence text "
                 "of their own; the terms of the named licence apply, and its full text "
                 "appears below.\n")
        for _label, pkg, lic in sorted(no_text):
            L.append(f"- {pkg} — {lic}")
        L.append("")

    L.append("## Licence texts\n")
    L.append("Texts are reproduced verbatim as shipped by the packages. Where several "
             "packages ship the same licence differing only in the copyright line, the "
             "text appears once and every copyright notice is listed with it.\n")
    for i, g in enumerate(ordered, 1):
        pkgs = sorted(g["packages"])
        L.append(f"### {i}. Applies to {len(pkgs)} package(s)\n")
        for p in pkgs:
            L.append(f"- {p}")
        L.append("")
        extra = sorted(g["copyrights"])
        if extra:
            L.append("Copyright notices:\n")
            for c in extra:
                L.append(f"- {c}")
            L.append("")
        L.append("```")
        L.append(g["text"])
        L.append("```\n")

    OUT.write_text("\n".join(L) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(ROOT)}: {len(rust)} crates, {len(npm)} npm packages, "
          f"{len(ordered)} distinct licence texts, {len(no_text)} without text "
          f"({OUT.stat().st_size / 1024:.0f} KB)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
