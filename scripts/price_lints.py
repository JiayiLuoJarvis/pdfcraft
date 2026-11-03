#!/usr/bin/env python3
"""Price [workspace.lints] for the four pdfcraft crates.

Copies the checkout's root Cargo.toml and clippy.toml into a detached git
worktree, switches every member to `[lints] workspace = true`, and lowers
each workspace deny to warn. One locked workspace clippy then prints hit
counts. Each crate column is prod/test. CI does not call this script.

    python3 scripts/price_lints.py
    python3 scripts/price_lints.py --extra-lints clippy::indexing_slicing,trivial_casts

The worktree is deleted when the run ends, including after a failure.
Build output goes in $CARGO_TARGET_DIR/price-lints when CARGO_TARGET_DIR
is set, and in the worktree's target/price-lints otherwise.
The worktree directory is $TMPDIR when that path is set and is not on the
root filesystem. Otherwise it is /mnt/data/tmp when that directory exists.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import uuid
from collections import defaultdict
from pathlib import Path
from typing import Literal

ROOT = Path(__file__).resolve().parents[1]

CRATES = (
    ("pdf", "pdfcraft-pdf"),
    ("forms", "pdfcraft-forms"),
    ("orgchart", "pdfcraft-orgchart"),
    ("post", "pdfcraft-post"),
)
CRATE_KEYS = tuple(key for key, _package in CRATES)

# Directory names and file suffixes that are test code even outside cfg(test).
TEST_DIR_PARTS = frozenset({"tests", "examples", "benches", "node_tests"})

Side = Literal["prod", "test"]
Ranges = dict[Path, list[tuple[int, int]]]
Counts = dict[tuple[str, str, Side], int]

CFG_TEST = re.compile(r"#\[\s*cfg\s*\(\s*test\s*\)\s*\]")
CFG_ANY = re.compile(r"#\[\s*cfg\s*\(([^]]*)\)\s*\]")
MOD_RE = re.compile(
    r"^(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*([;{])",
    re.M,
)
PATH_ATTR = re.compile(r"#\[\s*path\s*=\s*\"([^\"]+)\"\s*\]")
LINTS_BLOCK = re.compile(
    r"\[lints(?:\.[^\]]*)?\].*?(?=\n\[(?!lints(?:\.|\]))|\Z)",
    re.S,
)
LINT_NAME = re.compile(r"(?:clippy::)?[A-Za-z_][A-Za-z0-9_]*")
UNKNOWN_LINT = re.compile(r"unknown (?:clippy )?lint: `([^`]+)`")
RUSTC_ERROR = re.compile(r"E\d+")


def mask_rust(src: str) -> str:
    """Blank comments and string or char literals.

    Newlines and length stay put so a later index still names the same line
    clippy reports.
    """
    chars = list(src)
    n = len(chars)
    i = 0

    def blank(start: int, end: int) -> None:
        for k in range(start, end):
            if chars[k] != "\n":
                chars[k] = " "

    while i < n:
        end = string_end(chars, i)
        if end is not None:
            blank(i, end)
            i = end
            continue
        end = char_end(chars, i)
        if end is not None:
            blank(i, end)
            i = end
            continue
        if chars[i] == "/" and i + 1 < n and chars[i + 1] == "/":
            j = i
            while j < n and chars[j] != "\n":
                j += 1
            blank(i, j)
            i = j
            continue
        if chars[i] == "/" and i + 1 < n and chars[i + 1] == "*":
            j = i + 2
            while j + 1 < n and not (chars[j] == "*" and chars[j + 1] == "/"):
                j += 1
            j = min(n, j + 2)
            blank(i, j)
            i = j
            continue
        i += 1
    return "".join(chars)


def string_end(chars: list[str], i: int) -> int | None:
    n = len(chars)
    if chars[i] not in 'brc"':
        return None
    if chars[i] in "brc" and i > 0 and (chars[i - 1].isalnum() or chars[i - 1] == "_"):
        return None
    j = i
    raw = False
    if chars[i] in "bc" and i + 1 < n and chars[i + 1] == "r":
        raw = True
        j = i + 2
    elif chars[i] == "r":
        raw = True
        j = i + 1
    elif chars[i] in "bc":
        j = i + 1
    if j >= n:
        return None
    if raw:
        if chars[j] not in '#"':
            return None
        hashes = 0
        while j < n and chars[j] == "#":
            hashes += 1
            j += 1
        if j >= n or chars[j] != '"':
            return None
        j += 1
        closer = '"' + ("#" * hashes)
        while j < n:
            if "".join(chars[j : j + len(closer)]) == closer:
                return j + len(closer)
            j += 1
        return n
    if chars[j] != '"':
        return None
    j += 1
    while j < n:
        if chars[j] == "\\":
            j += 2
            continue
        if chars[j] == '"':
            return j + 1
        if chars[j] == "\n":
            return None
        j += 1
    return None


def char_end(chars: list[str], i: int) -> int | None:
    if chars[i] != "'":
        return None
    if i > 0 and (chars[i - 1].isalnum() or chars[i - 1] == "_"):
        return None
    n = len(chars)
    if i + 1 >= n:
        return None
    j = i + 1
    if chars[j] == "\\":
        j += 1
        if j < n and chars[j] == "u" and j + 1 < n and chars[j + 1] == "{":
            j += 2
            while j < n and chars[j] != "}":
                j += 1
            j += 1
        else:
            j += 1
        if j < n and chars[j] == "'":
            return j + 1
        return None
    if chars[j].isalpha() or chars[j] == "_":
        if j + 1 < n and chars[j + 1] == "'":
            return j + 2
        return None
    if chars[j] != "'" and j + 1 < n and chars[j + 1] == "'":
        return j + 2
    return None


def line_of(text: str, idx: int) -> int:
    return text.count("\n", 0, idx) + 1


def skip_attribute(masked: str, pos: int) -> int:
    if not masked.startswith("#[", pos):
        return pos
    depth = 0
    i = pos + 1
    n = len(masked)
    while i < n:
        if masked[i] == "[":
            depth += 1
        elif masked[i] == "]":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return n


def end_of_item(masked: str, pos: int) -> int:
    rel = 0
    seen_brace = False
    i = pos
    n = len(masked)
    while i < n:
        c = masked[i]
        if c == "{":
            rel += 1
            seen_brace = True
        elif c == "}":
            rel -= 1
            if seen_brace and rel == 0:
                return i + 1
        elif c == ";" and rel == 0:
            return i + 1
        i += 1
    return n


def item_start(masked: str, pos: int) -> int:
    n = len(masked)
    while pos < n:
        while pos < n and masked[pos] in " \t\r\n":
            pos += 1
        if masked.startswith("#[", pos):
            pos = skip_attribute(masked, pos)
            continue
        if masked.startswith("///", pos) or masked.startswith("//!", pos) or masked.startswith("//", pos):
            nl = masked.find("\n", pos)
            pos = n if nl < 0 else nl + 1
            continue
        break
    return pos


def module_file(parent: Path, name: str, path_attr: str | None) -> Path | None:
    if path_attr is not None:
        return (parent.parent / path_attr).resolve()
    # A crate root keeps children beside itself. A nested module file uses a sibling directory.
    if parent.name in {"lib.rs", "main.rs", "mod.rs"}:
        base = parent.parent
    else:
        base = parent.parent / parent.stem
    file = base / f"{name}.rs"
    mod = base / name / "mod.rs"
    if file.is_file():
        return file
    if mod.is_file():
        return mod
    return file


def expand_test_files(seeds: set[Path]) -> set[Path]:
    out = {path.resolve() for path in seeds if path.is_file()}
    changed = True
    while changed:
        changed = False
        for path in list(out):
            root = path.parent if path.name == "mod.rs" else path.parent / path.stem
            if not root.is_dir():
                continue
            for child in root.rglob("*.rs"):
                child = child.resolve()
                if child not in out:
                    out.add(child)
                    changed = True
    return out


def scan_tree(crate_dir: Path) -> tuple[set[Path], Ranges, list[str]]:
    """Return cfg(test) files, inclusive line ranges, and cfg notes that mention test."""
    files = sorted(crate_dir.rglob("*.rs"))
    ranges: Ranges = defaultdict(list)
    seeds: set[Path] = set()
    odd: list[str] = []
    for path in files:
        text = path.read_text(encoding="utf-8")
        masked = mask_rust(text)
        for match in CFG_ANY.finditer(masked):
            inner = match.group(1)
            if re.search(r"\btest\b", inner) and not re.fullmatch(r"\s*test\s*", inner):
                odd.append(f"{path}:{line_of(text, match.start())}: cfg({inner.strip()})")
        for match in CFG_TEST.finditer(masked):
            start = item_start(masked, match.end())
            end = end_of_item(masked, start)
            ranges[path.resolve()].append(
                (line_of(text, match.start()), line_of(text, max(match.start(), end - 1)))
            )
            item = masked[start:end]
            mod = MOD_RE.match(item)
            if mod and mod.group(2) == ";":
                zone = text[match.start() : start]
                path_m = PATH_ATTR.search(zone)
                dest = module_file(path, mod.group(1), path_m.group(1) if path_m else None)
                if dest is not None:
                    seeds.add(dest)
    return expand_test_files(seeds), ranges, odd


def bucket(path: Path, line: int, test_files: set[Path], ranges: Ranges) -> Side:
    resolved = path.resolve()
    if resolved.name.endswith("_tests.rs") or any(part in TEST_DIR_PARTS for part in resolved.parts):
        return "test"
    if resolved in test_files:
        return "test"
    for lo, hi in ranges.get(resolved, []):
        if lo <= line <= hi:
            return "test"
    return "prod"


def lower_workspace_denies(text: str) -> str:
    section = ""
    lines: list[str] = []
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            section = stripped[1:-1]
        if section == "workspace.lints" or section.startswith("workspace.lints."):
            line = line.replace('"deny"', '"warn"')
        lines.append(line)
    body = "\n".join(lines)
    if text.endswith("\n"):
        body += "\n"
    return body


def use_workspace_lints(text: str) -> str:
    match = LINTS_BLOCK.search(text)
    block = "[lints]\nworkspace = true\n"
    if match is None:
        suffix = "" if text.endswith("\n") or text == "" else "\n"
        return text + suffix + block
    return text[: match.start()] + block + text[match.end() :]


def parse_extra(raw: str | None) -> list[str]:
    if not raw:
        return []
    names: list[str] = []
    for part in raw.split(","):
        name = part.strip().replace("-", "_")
        if not name:
            continue
        if LINT_NAME.fullmatch(name) is None:
            raise SystemExit(f"bad --extra-lints entry: {part.strip()}")
        if name not in names:
            names.append(name)
    return names


def same_device(path: Path, other: Path) -> bool:
    probe = path
    while not probe.exists() and probe != probe.parent:
        probe = probe.parent
    try:
        return probe.stat().st_dev == other.stat().st_dev
    except OSError:
        return False


def scratch_parent() -> Path:
    """Where the detached worktree is created.

    The root filesystem is nearly full. Honor TMPDIR unless that path is on
    the root filesystem and /mnt/data/tmp exists on another one.
    """
    preferred = Path("/mnt/data/tmp")
    raw = os.environ.get("TMPDIR")
    if raw:
        chosen = Path(raw)
        if preferred.is_dir() and same_device(chosen, Path("/")) and not same_device(preferred, Path("/")):
            return preferred
        return chosen
    if preferred.is_dir():
        return preferred
    return Path(tempfile.gettempdir())


def target_dir(worktree: Path) -> Path:
    raw = os.environ.get("CARGO_TARGET_DIR")
    if raw:
        parent = Path(raw)
        if not parent.is_absolute():
            parent = Path.cwd() / parent
        return parent / "price-lints"
    return worktree / "target" / "price-lints"


def inside_repo(path: Path) -> bool:
    try:
        path.resolve().relative_to(ROOT.resolve())
    except ValueError:
        return False
    return True


def add_worktree(path: Path) -> None:
    if inside_repo(path):
        raise SystemExit(f"worktree path is inside the checkout: {path}")
    proc = subprocess.run(
        ["git", "worktree", "add", "--detach", str(path)],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr)
        raise SystemExit(proc.returncode or 1)


def remove_worktree(path: Path) -> None:
    if not path.name.startswith("pdfcraft-price-lints-"):
        print(f"refuse to delete {path}", file=sys.stderr)
        return
    if inside_repo(path):
        print(f"refuse to delete {path}", file=sys.stderr)
        return
    subprocess.run(
        ["git", "worktree", "remove", "--force", str(path)],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if path.exists():
        shutil.rmtree(path)
    subprocess.run(
        ["git", "worktree", "prune"],
        cwd=ROOT,
        capture_output=True,
        check=False,
    )


def sync_worktree(worktree: Path) -> None:
    root_toml = ROOT / "Cargo.toml"
    clippy_toml = ROOT / "clippy.toml"
    if not root_toml.is_file():
        raise SystemExit(f"missing {root_toml}")
    if not clippy_toml.is_file():
        raise SystemExit(f"missing {clippy_toml}")
    lowered = lower_workspace_denies(root_toml.read_text(encoding="utf-8"))
    (worktree / "Cargo.toml").write_text(lowered, encoding="utf-8")
    (worktree / "clippy.toml").write_text(clippy_toml.read_text(encoding="utf-8"), encoding="utf-8")
    for _key, package in CRATES:
        manifest = worktree / "crates" / package / "Cargo.toml"
        if not manifest.is_file():
            raise SystemExit(f"missing {manifest}")
        manifest.write_text(use_workspace_lints(manifest.read_text(encoding="utf-8")), encoding="utf-8")


def run_clippy(worktree: Path, extra: list[str], log: Path) -> int:
    log.parent.mkdir(parents=True, exist_ok=True)
    cmd = [
        "cargo",
        "clippy",
        "--locked",
        "--workspace",
        "--all-targets",
        "--all-features",
        "--message-format=json",
    ]
    if extra:
        cmd.append("--")
        for name in extra:
            cmd.extend(["-W", name])
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(log.parent)
    # Ambient rustc flags would price a different set than [workspace.lints].
    env.pop("RUSTFLAGS", None)
    env.pop("CARGO_ENCODED_RUSTFLAGS", None)
    err_path = log.with_suffix(".err")
    print(f"cargo clippy in {worktree}", file=sys.stderr, flush=True)
    with log.open("w", encoding="utf-8") as out, err_path.open("w", encoding="utf-8") as err:
        proc = subprocess.run(
            cmd,
            cwd=worktree,
            env=env,
            stdout=out,
            stderr=err,
            check=False,
        )
    return proc.returncode


def locate(file_name: str, worktree: Path) -> Path | None:
    raw = Path(file_name)
    path = raw if raw.is_absolute() else worktree / raw
    try:
        resolved = path.resolve()
        resolved.relative_to(worktree.resolve())
    except ValueError:
        return None
    if not resolved.is_file():
        return None
    return resolved


def crate_key(rel: Path) -> str | None:
    parts = rel.parts
    if len(parts) < 2 or parts[0] != "crates":
        return None
    for key, package in CRATES:
        if parts[1] == package:
            return key
    return None


def note_unknown(text: str, code: str, unknown: set[str]) -> bool:
    if code in {"unknown_lints", "clippy::unknown_clippy_lints"} or "unknown lint" in text:
        found = UNKNOWN_LINT.findall(text)
        if found:
            unknown.update(found)
        elif text.strip():
            unknown.add(text.strip())
        return True
    return False


def consume(
    log: Path,
    worktree: Path,
    scans: dict[str, tuple[set[Path], Ranges]],
    counts: Counts,
    examples: dict[str, list[str]],
    unknown: set[str],
) -> None:
    seen: set[tuple[str, str, int, int, str]] = set()
    if not log.is_file():
        return
    for raw in log.read_text(encoding="utf-8", errors="replace").splitlines():
        if not raw.startswith("{"):
            continue
        try:
            item = json.loads(raw)
        except json.JSONDecodeError:
            continue
        if item.get("reason") != "compiler-message":
            continue
        message = item.get("message") or {}
        level = message.get("level")
        if level not in {"warning", "error"}:
            continue
        code = ((message.get("code") or {}).get("code")) or ""
        code = code.replace("-", "_")
        text = message.get("message") or ""
        if note_unknown(text, code, unknown):
            continue
        if not code or RUSTC_ERROR.fullmatch(code):
            continue
        spans = message.get("spans") or []
        primary = next((span for span in spans if span.get("is_primary")), None)
        if primary is None and spans:
            primary = spans[0]
        if primary is None:
            continue
        path = locate(primary.get("file_name") or "", worktree)
        if path is None:
            continue
        try:
            rel = path.relative_to(worktree.resolve())
        except ValueError:
            continue
        key_name = crate_key(rel)
        if key_name is None:
            continue
        line_no = int(primary.get("line_start") or 0)
        column = int(primary.get("column_start") or 0)
        dedupe = (code, rel.as_posix(), line_no, column, text)
        if dedupe in seen:
            continue
        seen.add(dedupe)
        test_files, ranges = scans[key_name]
        side = bucket(path, line_no, test_files, ranges)
        counts[(code, key_name, side)] += 1
        if side != "prod":
            continue
        sample = examples[code]
        loc = f"{rel.as_posix()}:{line_no}"
        if loc not in sample and len(sample) < 2:
            sample.append(loc)


def print_report(
    counts: Counts,
    examples: dict[str, list[str]],
    extra: list[str],
    unknown: set[str],
    rc: int,
) -> None:
    print(f"clippy exit={rc}")
    print("unknown lints:")
    if unknown:
        for name in sorted(unknown):
            print(name)
    else:
        print("(none)")
    print("\t".join(["lint", *CRATE_KEYS, "prod_total", "example"]))
    codes = sorted({code for code, _crate, _side in counts} | set(extra))
    for code in codes:
        cells: list[str] = []
        prod_total = 0
        for key in CRATE_KEYS:
            prod = counts.get((code, key, "prod"), 0)
            test = counts.get((code, key, "test"), 0)
            prod_total += prod
            cells.append(f"{prod}/{test}")
        print("\t".join([code, *cells, str(prod_total), " ".join(examples.get(code, []))]))


def assert_denies_lowered(original: str) -> None:
    lowered = lower_workspace_denies(original)
    src_lines = original.splitlines()
    dst_lines = lowered.splitlines()
    if len(src_lines) != len(dst_lines):
        raise SystemExit("self-check: lowering denies changed the line count")
    section = ""
    for src, dst in zip(src_lines, dst_lines):
        stripped = src.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            section = stripped[1:-1]
        in_lints = section == "workspace.lints" or section.startswith("workspace.lints.")
        if in_lints:
            if '"deny"' in dst:
                raise SystemExit(f"self-check: workspace deny remains: {dst}")
            if '"forbid"' in src and src != dst:
                raise SystemExit(f"self-check: forbid line changed: {dst}")
        elif src != dst:
            raise SystemExit(f"self-check: non-lint line changed: {dst}")


def cfg_test_ranges(text: str) -> list[tuple[int, int]]:
    masked = mask_rust(text)
    found: list[tuple[int, int]] = []
    for match in CFG_TEST.finditer(masked):
        start = item_start(masked, match.end())
        end = end_of_item(masked, start)
        found.append((line_of(text, match.start()), line_of(text, max(match.start(), end - 1))))
    return found


def self_check() -> None:
    src = """
fn prod() {
    let s = "{";
    let raw = r#" { "#;
    let ch = '{';
    let life = "a";
    let hidden = "#[cfg(test)] fn not_a_test() {}";
}
#[cfg(test)]
fn tests_fn() {
    let x = 1;
}
#[cfg(test)]
mod tests {
    fn inner() {}
}
"""
    masked = mask_rust(src)
    if masked.count("{") != masked.count("}"):
        raise SystemExit("self-check: string masking changed brace balance")
    ranges = cfg_test_ranges(src)
    if len(ranges) != 2:
        raise SystemExit(f"self-check: expected 2 cfg(test) ranges, saw {ranges}")
    prod_line = src.splitlines().index("fn prod() {") + 1
    hidden = src.splitlines().index('    let hidden = "#[cfg(test)] fn not_a_test() {}";') + 1
    fn_line = src.splitlines().index("fn tests_fn() {") + 1
    inner = src.splitlines().index("    fn inner() {}") + 1
    if any(lo <= prod_line <= hi for lo, hi in ranges):
        raise SystemExit("self-check: prod fn fell inside cfg(test)")
    if any(lo <= hidden <= hi for lo, hi in ranges):
        raise SystemExit("self-check: cfg(test) inside a string became a range")
    if not any(lo <= fn_line <= hi for lo, hi in ranges):
        raise SystemExit("self-check: cfg(test) fn was not a test range")
    if not any(lo <= inner <= hi for lo, hi in ranges):
        raise SystemExit("self-check: cfg(test) module body was not a test range")

    virtual = Path("/virtual/crates/pdfcraft-pdf/src/lib.rs")
    if bucket(virtual, prod_line, set(), {virtual.resolve(): ranges}) != "prod":
        raise SystemExit("self-check: bucket marked a prod line as test")
    node = Path("/virtual/crates/pdfcraft-pdf/src/layout/node_tests/case.rs")
    tests_dir = Path("/virtual/crates/pdfcraft-pdf/tests/smoke.rs")
    suffix = Path("/virtual/crates/pdfcraft-pdf/src/layout/decorate_tests.rs")
    if bucket(node, 1, set(), {}) != "test":
        raise SystemExit("self-check: node_tests/ was prod")
    if bucket(tests_dir, 1, set(), {}) != "test":
        raise SystemExit("self-check: tests/ was prod")
    if bucket(suffix, 1, set(), {}) != "test":
        raise SystemExit("self-check: *_tests.rs was prod")

    sample = """
[workspace.lints.rust]
missing_docs = "deny"
unsafe_code = "forbid"

[workspace.lints.clippy]
panic = "deny"

[workspace.dependencies]
left = "deny"
"""
    lowered = lower_workspace_denies(sample)
    if 'missing_docs = "warn"' not in lowered or 'panic = "warn"' not in lowered:
        raise SystemExit("self-check: workspace deny was not lowered")
    if 'unsafe_code = "forbid"' not in lowered:
        raise SystemExit("self-check: forbid was rewritten")
    if '[workspace.dependencies]\nleft = "deny"' not in lowered:
        raise SystemExit("self-check: deny outside workspace.lints was rewritten")

    member = '[package]\nname = "x"\n\n[lints.rust]\nmissing_docs = "deny"\n\n[lints.clippy]\npanic = "deny"\n\n[[example]]\nname = "hello"\n'
    rewritten = use_workspace_lints(member)
    expect = '[package]\nname = "x"\n\n[lints]\nworkspace = true\n\n[[example]]\nname = "hello"\n'
    if rewritten != expect:
        raise SystemExit(f"self-check: member lints rewrite\n{rewritten}")

    for _key, package in CRATES:
        manifest = ROOT / "crates" / package / "Cargo.toml"
        original = manifest.read_text(encoding="utf-8")
        updated = use_workspace_lints(original)
        if "[lints.rust]" in updated or "[lints.clippy]" in updated:
            raise SystemExit(f"self-check: {package} still has a crate lint table")
        if "[lints]\nworkspace = true\n" not in updated:
            raise SystemExit(f"self-check: {package} did not switch to workspace lints")
        start = original.find("[lints")
        if start < 0 or not updated.startswith(original[:start]):
            raise SystemExit(f"self-check: {package} prefix changed")
    pdf = (ROOT / "crates" / "pdfcraft-pdf" / "Cargo.toml").read_text(encoding="utf-8")
    if "[[example]]" in pdf and "[[example]]" not in use_workspace_lints(pdf):
        raise SystemExit("self-check: pdf examples were dropped")
    assert_denies_lowered((ROOT / "Cargo.toml").read_text(encoding="utf-8"))

    if parse_extra("clippy::indexing-slicing, trivial_casts,trivial_casts") != [
        "clippy::indexing_slicing",
        "trivial_casts",
    ]:
        raise SystemExit("self-check: extra lint parse")


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--extra-lints",
        metavar="LIST",
        help=(
            "comma-separated lints passed as -W on the same clippy run. "
            "Prefix clippy lints with clippy::. Leave rustc lints bare. "
            "A name with no hits is still a row of zeros"
        ),
    )
    args = parser.parse_args()
    extra = parse_extra(args.extra_lints)
    self_check()
    if not (ROOT / "Cargo.toml").is_file():
        raise SystemExit(f"missing {ROOT / 'Cargo.toml'}")

    parent = scratch_parent()
    parent.mkdir(parents=True, exist_ok=True)
    worktree = parent / f"pdfcraft-price-lints-{os.getpid()}-{uuid.uuid4().hex[:8]}"
    try:
        add_worktree(worktree)
        sync_worktree(worktree)
        built = target_dir(worktree)
        log = built / "clippy.json"
        rc = run_clippy(worktree, extra, log)
        scans: dict[str, tuple[set[Path], Ranges]] = {}
        for key, package in CRATES:
            test_files, ranges, odd = scan_tree(worktree / "crates" / package)
            scans[key] = (test_files, ranges)
            for note in odd:
                print(f"odd cfg: {note}", file=sys.stderr)
        counts: Counts = defaultdict(int)
        examples: dict[str, list[str]] = defaultdict(list)
        unknown: set[str] = set()
        err_path = log.with_suffix(".err")
        if err_path.is_file():
            err_text = err_path.read_text(encoding="utf-8", errors="replace")
            unknown.update(UNKNOWN_LINT.findall(err_text))
        else:
            err_text = ""
        consume(log, worktree, scans, counts, examples, unknown)
        for line in err_text.splitlines():
            if "unknown field" in line or ("clippy.toml" in line and "warning" in line.lower()):
                print(f"config: {line}")
        print_report(counts, examples, extra, unknown, rc)
        if rc != 0 and err_text.strip():
            tail = "\n".join(err_text.splitlines()[-40:])
            print("clippy stderr (tail):", file=sys.stderr)
            print(tail, file=sys.stderr)
        return rc
    finally:
        remove_worktree(worktree)


if __name__ == "__main__":
    sys.exit(main())
