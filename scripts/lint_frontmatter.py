#!/usr/bin/env python3
"""Check that mission and RFC frontmatter is parseable YAML.

Three linters read document frontmatter:

  * `scripts/lint_status_vh_sync.sh` reads `version:`
  * `scripts/lint_no_vh_accepted.sh` reads `status:`
  * `scripts/lint_plan_yaml_vh_sync.sh` reads `version:`

All three read it with `awk` and pattern-match a line. None of them parses it,
which means malformed YAML is invisible to every check in the repository. A
document whose `description` is an unterminated quoted scalar, or whose value
contains `: ` unquoted, or whose author list contains `@`, reads perfectly
well to a line-oriented reader while being unparseable to anything actually
loading the metadata.

This parses with a real YAML loader and reports which documents fail and why.
It is not wired into CI. Wiring it is a maintainer decision, because adopting
it as a gate means repairing the corpus it names.

Counters are reported separately on purpose: `SCANNED`, `WITH_FRONTMATTER` and
`PARSED` are different populations, and a summary that collapses them makes a
skipped document look like a passing one.

Usage:
    python3 scripts/lint_frontmatter.py            # summary, exit 1 if any fail
    python3 scripts/lint_frontmatter.py --report   # name every failing document
    python3 scripts/lint_frontmatter.py --tree missions
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

import yaml

ROOT = pathlib.Path(__file__).resolve().parent.parent


def frontmatter_of(text: str) -> str | None:
    """The YAML block between the leading `---` fences, or None."""
    if not text.startswith("---\n"):
        return None
    end = text.find("\n---", 3)
    if end == -1:
        return ""
    return text[4:end]


def fence_misplaced(block: str) -> bool:
    """True if the block runs past the metadata into body prose.

    Detected structurally rather than by reading the loader's message. Three
    mission files open a frontmatter block and do not close it until after a
    paragraph of body text, so the loader reports a YAML *alias* error about a
    `**bold**` marker that has no business being in metadata at all. The error
    names the symptom; the defect is the fence. A first version of this
    function trusted the message and mislabelled all three. A second trusted a
    too-loose proxy -- a blank line followed by a top-level line -- and
    over-matched, because that is ordinary YAML.
    """
    top_level_key = re.compile(r"^[A-Za-z0-9_][A-Za-z0-9_. -]*:")
    for line in block.splitlines():
        if not line.strip() or line[:1] in (" ", "\t", "-", "#"):
            continue
        if top_level_key.match(line):
            continue
        return True
    return False


def classify(error: Exception, block: str) -> str:
    """Name the defect, because the four causes need four different fixes.

    A single "invalid YAML" bucket would leave the reader to rediscover which
    of these they are looking at, for each of the failing documents.
    """
    message = str(error)
    if fence_misplaced(block):
        return "closing fence placed after body text - move the fence up"
    if "mapping values are not allowed" in message:
        return "unquoted value containing ': ' - quote the value"
    if "found character '@'" in message or "cannot start any token" in message:
        return "reserved indicator '@' - quote the value"
    if "found character '*'" in message or "alias" in message:
        return "leading '*' read as an alias - quote the value"
    if "quoted scalar" in message or "block mapping" in message:
        return "unterminated quote or structural error - check the fences"
    return "unclassified YAML error"


def documents(tree: str) -> list[pathlib.Path]:
    root = ROOT / tree
    if root.is_dir():
        return sorted(root.rglob("*.md"))
    if root.is_file():
        return [root]
    return []


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tree", help="a subdirectory of the repository (default: all three)")
    parser.add_argument("--report", action="store_true", help="name every failing document")
    args = parser.parse_args()

    trees = [args.tree] if args.tree else ["missions", "rfcs", "docs"]

    scanned = 0
    with_frontmatter = 0
    parsed = 0
    failures: list[tuple[str, str]] = []

    for tree in trees:
        paths = documents(tree)
        if not paths:
            print(f"FAIL: no markdown documents under {tree}", file=sys.stderr)
            return 2
        scanned += len(paths)
        for path in paths:
            try:
                text = path.read_text(encoding="utf-8")
            except OSError as exc:
                print(f"FAIL: cannot read {path}: {exc}", file=sys.stderr)
                return 2
            block = frontmatter_of(text)
            if block is None:
                continue
            with_frontmatter += 1
            try:
                yaml.safe_load(block)
            except Exception as exc:  # noqa: BLE001 - report any loader error
                failures.append((str(path.relative_to(ROOT)), classify(exc, block)))
                continue
            parsed += 1

    print(f"SCANNED           {scanned}")
    print(f"WITH_FRONTMATTER  {with_frontmatter}")
    print(f"PARSED            {parsed}")
    print(f"FAILED            {len(failures)}")

    # A run that examined nothing must not report success. Four scripts in this
    # repository once did, and the counter that hid it was the one being
    # reported.
    if scanned == 0 or with_frontmatter == 0:
        print("FAIL: examined no frontmatter; a green result here proves nothing")
        return 2

    if args.report:
        by_cause: dict[str, list[str]] = {}
        for path, cause in failures:
            by_cause.setdefault(cause, []).append(path)
        for cause in sorted(by_cause, key=lambda c: (-len(by_cause[c]), c)):
            print(f"\n{len(by_cause[cause])}  {cause}")
            for path in sorted(by_cause[cause]):
                print(f"     {path}")

    if failures:
        print(f"\n{len(failures)} document(s) carry frontmatter that no YAML loader accepts.")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())