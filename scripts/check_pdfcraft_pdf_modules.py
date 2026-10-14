#!/usr/bin/env python3
"""Check pdfcraft-pdf production modules against scripts/pdfcraft-pdf-module-edges.txt.

The edge file is the allowed directed graph. This script reads crates/pdfcraft-pdf/src,
ignores cfg(test) modules and cfg(test) items, and fails when the uses in the
tree differ from that file or when the graph has a cycle.

    python3 scripts/check_pdfcraft_pdf_modules.py
    python3 scripts/check_pdfcraft_pdf_modules.py --print
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "crates" / "pdfcraft-pdf" / "src"
EDGES = Path(__file__).resolve().parent / "pdfcraft-pdf-module-edges.txt"

COLLAPSE = {"text", "spec", "backend"}
ROOT_MODULES = {
    "error",
    "geom",
    "limits",
    "canvas",
    "fonts",
    "text",
    "spec",
    "layout",
    "backend",
}
IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


def blank_non_code(src: str) -> str:
    out: list[str] = []
    i = 0
    n = len(src)
    while i < n:
        if src.startswith("//", i):
            i += 2
            while i < n and src[i] != "\n":
                i += 1
            continue
        if src.startswith("/*", i):
            i += 2
            depth = 1
            while i < n and depth:
                if src.startswith("/*", i):
                    depth += 1
                    i += 2
                elif src.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    if src[i] == "\n":
                        out.append("\n")
                    i += 1
            continue
        if src[i] in {'"', "'"} or src.startswith("b'", i) or src.startswith('b"', i):
            i = blank_string_or_char(src, i, out)
            continue
        if src[i] == "r" or src.startswith("br", i):
            nxt = i + (2 if src.startswith("br", i) else 1)
            if nxt < n and (src[nxt] == '"' or src[nxt] == "#"):
                i = blank_raw(src, i, out)
                continue
        out.append(src[i])
        i += 1
    return "".join(out)


def blank_string_or_char(src: str, i: int, out: list[str]) -> int:
    n = len(src)
    if src.startswith("b'", i) or src.startswith('b"', i):
        i += 1
    quote = src[i]
    if quote == "'" and not is_char_literal(src, i):
        out.append("'")
        return i + 1
    out.append(" ")
    i += 1
    while i < n:
        if src[i] == "\\":
            i += 2
            continue
        if src[i] == quote:
            return i + 1
        if src[i] == "\n":
            out.append("\n")
        i += 1
    return i


def is_char_literal(src: str, i: int) -> bool:
    if i + 1 >= len(src) or src[i] != "'":
        return False
    if src[i + 1] == "\\":
        return True
    return i + 2 < len(src) and src[i + 2] == "'"


def blank_raw(src: str, i: int, out: list[str]) -> int:
    n = len(src)
    if src.startswith("br", i):
        i += 2
    else:
        i += 1
    hashes = 0
    while i < n and src[i] == "#":
        hashes += 1
        i += 1
    if i >= n or src[i] != '"':
        out.append(" ")
        return i
    i += 1
    closer = '"' + ("#" * hashes)
    while i < n:
        if src.startswith(closer, i):
            return i + len(closer)
        if src[i] == "\n":
            out.append("\n")
        i += 1
    return i


def end_attribute(src: str, i: int) -> int:
    if not src.startswith("#[", i):
        return i
    j = i + 1
    while j < len(src) and src[j].isspace():
        j += 1
    if j >= len(src) or src[j] != "[":
        return i
    depth = 0
    while j < len(src):
        if src[j] == "[":
            depth += 1
        elif src[j] == "]":
            depth -= 1
            if depth == 0:
                return j + 1
        j += 1
    return len(src)


def is_cfg_test(attr: str) -> bool:
    return re.sub(r"\s+", "", attr) == "#[cfg(test)]"


def skip_ws(src: str, i: int) -> int:
    while i < len(src) and src[i] in " \t\r\n":
        i += 1
    return i


def skip_balanced(src: str, i: int, open_ch: str, close_ch: str) -> int:
    if i >= len(src) or src[i] != open_ch:
        return i
    depth = 0
    while i < len(src):
        if src[i] == open_ch:
            depth += 1
        elif src[i] == close_ch:
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return i


def skip_visibility(src: str, i: int) -> int:
    i = skip_ws(src, i)
    if not src.startswith("pub", i):
        return i
    i += 3
    i = skip_ws(src, i)
    if i < len(src) and src[i] == "(":
        return skip_balanced(src, i, "(", ")")
    return i


def skip_item(src: str, i: int) -> int:
    i = skip_ws(src, i)
    i = skip_visibility(src, i)
    while True:
        i = skip_ws(src, i)
        matched = False
        for word in ("async", "unsafe", "const", "extern"):
            if src.startswith(word, i) and ident_boundary(src, i + len(word)):
                i += len(word)
                if word == "extern":
                    i = skip_ws(src, i)
                    if i < len(src) and src[i] == '"':
                        i = blank_index_of_string(src, i)
                matched = True
                break
        if not matched:
            break
    i = skip_ws(src, i)
    keyword = None
    for word in ("use", "mod", "fn", "struct", "enum", "impl", "static", "type", "trait", "let"):
        if src.startswith(word, i) and ident_boundary(src, i + len(word)):
            keyword = word
            i += len(word)
            break
    if keyword == "use":
        return skip_use(src, i)
    paren = 0
    bracket = 0
    while i < len(src):
        ch = src[i]
        if ch == "(":
            paren += 1
        elif ch == ")":
            paren = max(0, paren - 1)
        elif ch == "[":
            bracket += 1
        elif ch == "]":
            bracket = max(0, bracket - 1)
        elif ch == "{" and paren == 0 and bracket == 0:
            return skip_balanced(src, i, "{", "}")
        elif ch == ";" and paren == 0 and bracket == 0:
            return i + 1
        i += 1
    return i


def ident_boundary(src: str, i: int) -> bool:
    return i >= len(src) or not (src[i].isalnum() or src[i] == "_")


def blank_index_of_string(src: str, i: int) -> int:
    quote = src[i]
    i += 1
    while i < len(src):
        if src[i] == "\\":
            i += 2
            continue
        if src[i] == quote:
            return i + 1
        i += 1
    return i


def skip_use(src: str, i: int) -> int:
    brace = 0
    while i < len(src):
        ch = src[i]
        if ch == "{":
            brace += 1
        elif ch == "}":
            brace = max(0, brace - 1)
        elif ch == ";" and brace == 0:
            return i + 1
        i += 1
    return i


def strip_cfg_test(src: str) -> str:
    out: list[str] = []
    i = 0
    while i < len(src):
        if src.startswith("#[", i):
            attrs_start = i
            attrs: list[str] = []
            while True:
                j = skip_ws(src, i)
                if not src.startswith("#[", j):
                    break
                end = end_attribute(src, j)
                if end == j:
                    break
                attrs.append(src[j:end])
                i = end
            if attrs and any(is_cfg_test(attr) for attr in attrs):
                i = skip_item(src, i)
                continue
            out.append(src[attrs_start:i])
            continue
        out.append(src[i])
        i += 1
    return "".join(out)


def layout_children(src: Path) -> set[str]:
    names = set()
    layout = src / "layout"
    for path in layout.iterdir():
        if path.name == "mod.rs":
            continue
        if path.is_dir():
            names.add(path.name)
        elif path.suffix == ".rs":
            names.add(path.stem)
    return names


def cfg_test_modules(text: str) -> set[str]:
    names = set()
    i = 0
    while True:
        j = text.find("#[", i)
        if j < 0:
            break
        end = end_attribute(text, j)
        if is_cfg_test(text[j:end]):
            k = skip_ws(text, end)
            while text.startswith("#[", k):
                k = end_attribute(text, k)
                k = skip_ws(text, k)
            k = skip_visibility(text, k)
            k = skip_ws(text, k)
            if text.startswith("mod ", k) or text.startswith("mod\t", k):
                k = skip_ws(text, k + 3)
                match = IDENT.match(text, k)
                if match:
                    names.add(match.group(0))
        i = max(end, j + 2)
    return names


def node_of(rel: str) -> str:
    parts = rel.split("/")
    if parts == ["lib.rs"]:
        return "lib"
    if len(parts) == 1 and parts[0].endswith(".rs"):
        return parts[0][:-3]
    top = parts[0]
    if top in COLLAPSE:
        return top
    if top == "layout":
        if parts[1] == "mod.rs":
            return "layout"
        name = parts[1][:-3] if parts[1].endswith(".rs") else parts[1]
        return f"layout/{name}"
    raise ValueError(f"unmapped file {rel}")


def resolve_target(segments: list[str], origin: str, children: set[str]) -> str | None:
    if not segments:
        return None
    head = segments[0]
    if head == "crate":
        return resolve_target(segments[1:], origin, children)
    if head == "super":
        parent = parent_node(origin)
        if parent is None:
            return None
        return walk(parent, segments[1:], children)
    if head == "self":
        return walk(origin, segments[1:], children)
    if head not in ROOT_MODULES and head != "lib":
        return None
    return walk(head, segments[1:], children)


def parent_node(origin: str) -> str | None:
    if origin in COLLAPSE:
        return origin
    if origin.startswith("layout/") and origin.count("/") == 1:
        return "layout"
    if origin.startswith("layout/"):
        return origin.rsplit("/", 1)[0]
    return None


def walk(node: str, rest: list[str], children: set[str]) -> str | None:
    if node in COLLAPSE:
        return node
    if node == "layout":
        if rest and rest[0] in children:
            return f"layout/{rest[0]}"
        return "layout"
    if node.startswith("layout/"):
        return node
    if node in ROOT_MODULES or node == "lib":
        return node
    return None


def parse_tree(src: str, i: int, prefix: list[str]) -> tuple[list[list[str]], int]:
    i = skip_ws(src, i)
    if i < len(src) and src[i] == "{":
        i += 1
        leaves: list[list[str]] = []
        while i < len(src):
            i = skip_ws(src, i)
            if i < len(src) and src[i] == "}":
                return leaves, i + 1
            more, i = parse_tree(src, i, prefix)
            leaves.extend(more)
            i = skip_ws(src, i)
            if i < len(src) and src[i] == ",":
                i += 1
        return leaves, i
    match = IDENT.match(src, i)
    if not match:
        return [], i
    i = match.end()
    segs = prefix + [match.group(0)]
    i = skip_ws(src, i)
    if src.startswith("as", i) and ident_boundary(src, i + 2):
        i = skip_ws(src, i + 2)
        renamed = IDENT.match(src, i)
        if renamed:
            i = renamed.end()
        return [segs], i
    if src.startswith("::", i):
        return parse_tree(src, i + 2, segs)
    return [segs], i


def iter_paths(text: str, needles: tuple[str, ...]) -> list[list[str]]:
    leaves: list[list[str]] = []
    for needle in needles:
        start = 0
        while True:
            j = text.find(needle, start)
            if j < 0:
                break
            if j > 0 and (text[j - 1].isalnum() or text[j - 1] == "_"):
                start = j + len(needle)
                continue
            prefix = needle[:-2]
            parsed, _ = parse_tree(text, skip_ws(text, j + len(needle)), [prefix])
            leaves.extend(parsed)
            start = j + len(needle)
    return leaves


def edges_in_text(text: str, origin: str, children: set[str], bare_roots: bool) -> set[tuple[str, str]]:
    found: set[tuple[str, str]] = set()
    for segments in iter_paths(text, ("crate::", "super::", "self::")):
        target = resolve_target(segments, origin, children)
        if target and target != origin:
            found.add((origin, target))
    if bare_roots:
        for name in sorted(ROOT_MODULES):
            token = name + "::"
            start = 0
            while True:
                j = text.find(token, start)
                if j < 0:
                    break
                if j >= 2 and text[j - 2 : j] == "::":
                    start = j + len(token)
                    continue
                if j > 0 and (text[j - 1].isalnum() or text[j - 1] == "_"):
                    start = j + len(token)
                    continue
                parsed, _ = parse_tree(text, j, [])
                for segments in parsed:
                    target = resolve_target(segments, origin, children)
                    if target and target != origin:
                        found.add((origin, target))
                start = j + len(token)
    return found


def production_text(path: Path) -> str:
    return strip_cfg_test(blank_non_code(path.read_text()))


def skipped_rels(src: Path) -> set[str]:
    skip: set[str] = set()
    for mod_rs in src.rglob("mod.rs"):
        rel_dir = mod_rs.parent.relative_to(src).as_posix()
        if rel_dir == ".":
            rel_dir = ""
        names = cfg_test_modules(blank_non_code(mod_rs.read_text()))
        for name in names:
            base = f"{rel_dir}/{name}" if rel_dir else name
            file = src / f"{base}.rs"
            directory = src / base
            if file.is_file():
                skip.add(file.relative_to(src).as_posix())
            if directory.is_dir():
                for child in directory.rglob("*.rs"):
                    skip.add(child.relative_to(src).as_posix())
    return skip


def observe(src: Path) -> set[tuple[str, str]]:
    children = layout_children(src)
    skip = skipped_rels(src)
    edges: set[tuple[str, str]] = set()
    for path in sorted(src.rglob("*.rs")):
        rel = path.relative_to(src).as_posix()
        if rel in skip:
            continue
        origin = node_of(rel)
        text = production_text(path)
        edges |= edges_in_text(text, origin, children, bare_roots=(rel == "lib.rs"))
    return edges


def load_edges(path: Path) -> set[tuple[str, str]]:
    edges: set[tuple[str, str]] = set()
    for line_no, raw in enumerate(path.read_text().splitlines(), 1):
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        parts = line.split()
        if len(parts) != 2:
            raise SystemExit(f"{path}:{line_no}: expected `from to`")
        edges.add((parts[0], parts[1]))
    return edges


def find_cycle(edges: set[tuple[str, str]]) -> list[str] | None:
    adj: dict[str, list[str]] = defaultdict(list)
    nodes: set[str] = set()
    for src, dst in edges:
        adj[src].append(dst)
        nodes.add(src)
        nodes.add(dst)
    color = {node: 0 for node in nodes}
    stack: list[str] = []

    def walk_node(node: str) -> list[str] | None:
        color[node] = 1
        stack.append(node)
        for nxt in adj[node]:
            if color[nxt] == 1:
                start = stack.index(nxt)
                return stack[start:] + [nxt]
            if color[nxt] == 0:
                found = walk_node(nxt)
                if found:
                    return found
        stack.pop()
        color[node] = 2
        return None

    for node in sorted(nodes):
        if color[node] == 0:
            found = walk_node(node)
            if found:
                return found
    return None


def format_edges(edges: set[tuple[str, str]]) -> str:
    return "\n".join(f"{src} {dst}" for src, dst in sorted(edges))


def self_check() -> None:
    sample = """
use crate::error::Error;
#[cfg(test)]
mod tests {
    use crate::layout::paginate::PaginateConfig;
}
#[cfg(test)]
fn helper() {
    let _x = crate::fonts::FontId::from_raw(0);
}
"""
    children = {"paginate", "decorate"}
    text = strip_cfg_test(blank_non_code(sample))
    edges = edges_in_text(text, "layout/decorate", children, bare_roots=False)
    if edges != {("layout/decorate", "error")}:
        raise SystemExit(f"self-check: cfg(test) strip failed, saw {sorted(edges)}")
    loop = {("a", "b"), ("b", "a")}
    cycle = find_cycle(loop)
    if cycle is None:
        raise SystemExit("self-check: cycle detector missed a two-node loop")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--print",
        action="store_true",
        help="print observed production edges and do not compare the edge file",
    )
    args = parser.parse_args()
    self_check()
    observed = observe(SRC)
    if args.print:
        print(format_edges(observed))
        return 0
    if not EDGES.is_file():
        print(f"missing {EDGES}", file=sys.stderr)
        return 1
    allowed = load_edges(EDGES)
    extra = observed - allowed
    missing = allowed - observed
    problems: list[str] = []
    if extra:
        problems.append("edges in the tree that are not in the edge file:\n" + format_edges(extra))
    if missing:
        problems.append("edges in the edge file that the tree does not use:\n" + format_edges(missing))
    cycle = find_cycle(allowed)
    if cycle:
        problems.append("cycle: " + " -> ".join(cycle))
    forbidden = ("layout/decorate", "layout/paginate")
    if forbidden in allowed or forbidden in observed:
        problems.append(
            "self-check: layout/decorate -> layout/paginate is present. "
            "Production decorate must not use paginate."
        )
    else:
        extended = set(allowed)
        extended.add(forbidden)
        made = find_cycle(extended)
        if made is None:
            problems.append(
                "self-check: adding layout/decorate -> layout/paginate did not cycle. "
                "layout/paginate -> layout/decorate is missing from the graph."
            )
    if problems:
        print("pdfcraft-pdf module dag failed", file=sys.stderr)
        print("\n\n".join(problems), file=sys.stderr)
        return 1
    print(f"pdfcraft-pdf module dag: {len(observed)} edges, acyclic")
    print("self-check: forbids layout/decorate -> layout/paginate")
    print("self-check: that edge cycles through layout/paginate -> layout/decorate")
    return 0


if __name__ == "__main__":
    sys.exit(main())
