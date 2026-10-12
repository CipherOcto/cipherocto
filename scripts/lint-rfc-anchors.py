#!/usr/bin/env python3
"""Anchor lint for rfcs/ — the defect class adversarial review does not catch.

Every `§Some Section` reference in `rfcs/` must resolve to a real heading in
some RFC. An unresolvable anchor is a navigation dead-end, and per R41 that is
how a security control becomes unaudited: RFC-0009's A13 defence rests on
"Settled in RFC-0958 §Revocation and freshness", and that heading did not exist.

Run: python3 scripts/lint-rfc-anchors.py [--strict] [--write-baseline]

The class is endemic, not per-RFC: as of R41 (2026-10-09) the pre-existing
corpus contains ~1.6k references that resolve nowhere. A lint that fails on
1.6k pre-existing findings gets ignored, so findings are baselined and the
default run fails only on NEW breakage. `--strict` reports everything and
always exits 1. `--write-baseline` rewrites the baseline after a deliberate
sweep.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RFC_ROOT = ROOT / "rfcs"
BASELINE_PATH = Path(__file__).resolve().parent / "lint-rfc-anchors-baseline.txt"

# Sources whose headings an RFC `§X` reference may legitimately target.
# BLUEPRINT.md is in this list because the single most common dangling
# reference in the corpus is `§Adversarial Review Process`, which is a
# BLUEPRINT section and not an RFC section.
INDEX_SOURCES = [RFC_ROOT, ROOT / "docs", ROOT / "missions"]

# `§Section Name` — the name runs to a backtick, pipe, newline, comma, an em or
# en dash, or the start of the next sentence. The comma terminator matters:
# `§A, §B` is two anchors, not one called "a, b". The dash terminators matter
# because this repo writes `§Data Structures — a note`, and without them the
# dash is swallowed into the name and every such reference reads as dangling.
ANCHOR = re.compile(
    r"§([A-Z][A-Za-z0-9][^`|,\n—–]*?)(?=`|\||,|—|–|\n|$|\s+[a-z]|\s*\.)"
)

# A heading is `## Something` / `### Something`. Strip markdown emphasis and a
# trailing parenthetical qualifier, so `## Additive Changes (per R28 M3)` is
# addressable as `§Additive Changes`.
HEADING = re.compile(r"^#{1,6}\s+(.*?)\s*$")


def normalize(text: str) -> str:
    """Strip emphasis, a leading section sign, and trailing punctuation.

    The leading `§` is the subtle one. ANCHOR captures a cited name *without*
    its `§`, but this repo also authors headings that way — `## §Format
    Bypass Defense`, `## §S6 — TimestampOpaque newtype`. Leaving the sign on
    the heading side made every such heading unaddressable, so the linter
    fabricated dangling findings for it and then baselined them: 44 baselined
    names resolved the moment it was stripped, and the baseline shrank from
    743 entries to 699 with no entry added.

    Parentheticals are handled at match time, not here, because `§Adversarial
    Review (12-finding table)` and `§Adversarial Review` must both resolve.
    """
    text = re.sub(r"[*_`]", "", text)
    return text.strip().lstrip("§").strip().rstrip(".,;:)]}").strip().lower()


def candidates(name: str) -> list[str]:
    """Progressively shorter forms of an anchor, longest first.

    `§Adversarial Review (12-finding table)` yields the full string, then
    `adversarial review` once the parenthetical is removed.
    """
    out: list[str] = []
    current = name
    while current:
        if current not in out:
            out.append(current)
        # Drop everything from an opening paren onward. The capture routinely
        # ends mid-parenthetical (`§Adversarial Review (12-finding` ), so a
        # balanced-group strip alone does not fire.
        stripped = re.sub(r"\s*\(.*$", "", current).strip().rstrip(".,;:)]}")
        if stripped == current:
            break
        current = stripped.lower()
    return out


def is_section_anchor(name: str) -> bool:
    """Distinguish `§Revocation and freshness` from review findings like `§A7`.

    A section anchor is multi-word. Single tokens in this corpus are almost
    always adversarial-review finding ids (`§A1`, `§F4`, `§C1`) or a stray
    `§)` from prose, neither of which has a heading.
    """
    return len(name.split()) >= 2


def heading_names(path: Path) -> set[str]:
    names: set[str] = set()
    for line in path.read_text(encoding="utf-8").splitlines():
        match = HEADING.match(line)
        if match:
            raw = normalize(match.group(1))
            for form in candidates(raw):
                names.add(form)
    return names


def resolves(name: str, known: set[str]) -> bool:
    """True if any progressive form of the anchor is a real heading.

    Falls back to a word-boundary prefix match so `§Adversarial Review`
    resolves against a heading written as `§Adversarial Review (12-finding
    table covering ...)`, and `§Configuration Validation` resolves against a
    heading carrying a trailing qualifier.
    """
    for form in candidates(name):
        if form in known:
            return True
    longest = candidates(name)[-1]
    if longest and any(k.startswith(longest + " ") for k in known):
        return True
    return False


def main() -> int:
    parser = argparse.ArgumentParser(add_help=True, description=__doc__)
    parser.add_argument(
        "--strict",
        action="store_true",
        help="report every dangling reference and always fail",
    )
    parser.add_argument(
        "--write-baseline",
        action="store_true",
        help="rewrite the baseline from the current findings",
    )
    args = parser.parse_args()

    rfc_files = sorted(RFC_ROOT.rglob("*.md"))
    if not rfc_files:
        # Not a pass. An absent or renamed corpus means this gating step
        # verified nothing, and returning 0 here would let that read as a
        # clean run — the same "reads nothing, reports green" shape this
        # suite exists to catch.
        print(f"no RFCs found under {RFC_ROOT}", file=sys.stderr)
        return 2

    known: set[str] = set()
    for source in INDEX_SOURCES:
        if source.is_file() and source.suffix == ".md":
            known |= heading_names(source)
        elif source.is_dir():
            for path in source.rglob("*.md"):
                known |= heading_names(path)

    unresolved: list[tuple[Path, int, str]] = []
    for path in rfc_files:
        for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            for raw in ANCHOR.findall(line):
                name = normalize(raw)
                if not is_section_anchor(name):
                    continue
                if not resolves(name, known):
                    unresolved.append((path, lineno, name))

    by_name: dict[str, list[str]] = {}
    for path, lineno, name in unresolved:
        by_name.setdefault(name, []).append(f"{path.relative_to(ROOT)}:{lineno}")

    if args.write_baseline:
        BASELINE_PATH.write_text(
            "\n".join(sorted(by_name)) + ("\n" if by_name else ""), encoding="utf-8"
        )
        print(f"baseline written: {len(by_name)} dangling anchor name(s)")
        return 0

    if not unresolved:
        print(f"anchor lint clean — {len(known)} headings indexed across docs")
        return 0

    baseline: set[str] = set()
    if BASELINE_PATH.exists():
        baseline = {
            line.strip()
            for line in BASELINE_PATH.read_text(encoding="utf-8").splitlines()
            if line.strip()
        }

    new_names = sorted(set(by_name) - baseline)
    fixed = len(baseline - set(by_name))

    print(f"anchors indexed: {len(known)} headings")
    print(f"dangling: {len(unresolved)} reference(s) over {len(by_name)} distinct name(s)")
    print(f"baselined: {len(baseline - set(by_name))} of {len(baseline)} previously known are now fixed")
    if fixed:
        print("  -> shrink the baseline with --write-baseline")

    if new_names:
        print(f"\nNEW dangling anchor(s) — not in baseline:\n")
        for name in new_names:
            sites = ", ".join(by_name[name][:4])
            extra = f" (+{len(by_name[name]) - 4} more)" if len(by_name[name]) > 4 else ""
            print(f"  §{name}\n      {sites}{extra}")

    if args.strict and by_name:
        print(f"\n--strict: {len(by_name)} distinct dangling anchor name(s) outstanding.")
        return 1

    if new_names:
        print(
            "\nA dangling anchor in a normative 'settled in RFC-NNNN §X' sentence "
            "is an\nunauditable control: the reader is sent nowhere."
        )
        return 1

    print("\nno new dangling anchors.")
    return 0


if __name__ == "__main__":
    sys.exit(main())