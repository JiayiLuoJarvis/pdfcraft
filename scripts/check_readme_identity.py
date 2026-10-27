#!/usr/bin/env python3
"""README identity check: root vs crate READMEs stay in sync.

Checks, per crate (pdfcraft-pdf, pdfcraft-forms, pdfcraft-orgchart,
pdfcraft-post):
  1. The crate Cargo.toml version appears in the crate README install
     snippet (`<name> = "<version>"`) and in the root README status line.
  2. The crate CHANGELOG.md exists and is linked from the root README and
     the root CHANGELOG.md.

Exit 0 when everything is in sync. Exit 1 naming each drift found.

Wire-up (left for the ci.yml owner, todo 11 touches the matrix job): add a
step to the `basics` job in `.github/workflows/ci.yml`, right after fmt:

    - name: readme identity
      run: python3 scripts/check_readme_identity.py
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES = ["pdfcraft-pdf", "pdfcraft-forms", "pdfcraft-orgchart", "pdfcraft-post"]

failures = []


def crate_version(name):
    text = (ROOT / "crates" / name / "Cargo.toml").read_text()
    m = re.search(r'^version\s*=\s*"([^"]+)"', text, re.M)
    if not m:
        failures.append(f"{name}: no version in crates/{name}/Cargo.toml")
        return None
    return m.group(1)


def check(name):
    version = crate_version(name)
    if version is None:
        return
    crate_readme = (ROOT / "crates" / name / "README.md").read_text()
    root_readme = (ROOT / "README.md").read_text()
    root_changelog = (ROOT / "CHANGELOG.md").read_text()

    if f'{name} = "{version}"' not in crate_readme:
        failures.append(
            f"{name}: crate README install snippet lacks "
            f'{name} = "{version}"'
        )
    if version not in root_readme:
        failures.append(f"{name}: root README has no status line for {version}")

    changelog = ROOT / "crates" / name / "CHANGELOG.md"
    if not changelog.is_file():
        failures.append(f"{name}: missing crates/{name}/CHANGELOG.md")
    else:
        link = f"crates/{name}/CHANGELOG.md"
        if link not in root_readme:
            failures.append(f"{name}: root README does not link {link}")
        if link not in root_changelog and name not in root_changelog:
            failures.append(f"{name}: root CHANGELOG.md does not reference {link}")


for crate in CRATES:
    check(crate)

if failures:
    for f in failures:
        print(f"drift: {f}")
    sys.exit(1)
print("readme identity: all 4 crates in sync")
