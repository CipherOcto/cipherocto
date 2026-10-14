#!/usr/bin/env python3
"""Measure what the anchor linter does, by calling it, not by reimplementing it.

Four proposals in this repository's history were measured by copying the
linter's resolution loop into a scratch script and running that. All four
numbers were wrong, for the same two reasons:

  1. The copy omitted the numeric-anchor gate. A numeric reference is checked
     only when its line also cites a project RFC, so external specifications
     such as `RFC 8032 §5.1.6` are never reported. The copy reported them.
  2. The copy replaced `resolves` with a plain `form in known` membership test,
     discarding the bidirectional word-boundary prefix fallback the linter
     already implements.

The result was 1,237 findings reported by the copy where the linter reports 599
on the same tree, and a proposal for a resolver that had been added in the same
session and was already in the file being copied.

This tool imports the linter as a module and calls its own functions. There is
no reimplementation to drift. Where a tree is not one CI already checks, it
reproduces the linter's loop faithfully and says so in its output, so a reader
can tell a measured number from a simulated one.

Usage:
    python3 scripts/measure_anchor_lint.py              # gated tree, as CI runs it
    python3 scripts/measure_anchor_lint.py --tree docs  # an ungated tree
    python3 scripts/measure_anchor_lint.py --all        # every tree worth knowing
    python3 scripts/measure_anchor_lint.py --terminator # also the ) and : variant

Exit 0 always. This reports; it does not gate.
"""

from __future__ import annotations

import argparse
import collections
import importlib.util
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LINTER = ROOT / "scripts" / "lint-rfc-anchors.py"

# The variant whose cost was measured repeatedly this session: a section name
# ends at a closing paren or a colon rather than running into the next word.
TERMINATOR = re.compile(
    r"§([A-Z][A-Za-z0-9][^`|,\n—–)\]:]*?)(?=`|\||,|—|–|\)|\||:|\n|$|\s+[a-z]|\s*\.)"
)


def load_linter():
    """Import the linter so its own functions are the ones under measurement."""
    spec = importlib.util.spec_from_file_location("anchors_lint_under_test", LINTER)
    if spec is None or spec.loader is None:
        sys.exit(f"cannot load {LINTER}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def index(m) -> set[str]:
    """Every heading name the linter itself would know about."""
    known: set[str] = set()
    for source in m.INDEX_SOURCES:
        if source.is_file() and source.suffix == ".md":
            known |= m.heading_names(source)
        elif source.is_dir():
            for path in source.rglob("*.md"):
                known |= m.heading_names(path)
    return known


def scan_occurrences(m, paths, known, numbers, rx=None) -> int:
    """Raw occurrence count, which is what the linter prints as 'dangling'.

    The linter reports occurrences; the baseline is keyed on (name, site), so a
    name cited twice on one line counts once against the baseline and twice in
    the headline. Both numbers are useful and they are not equal, which is
    exactly why a measurement that quotes one of them without saying which is
    easy to misread.
    """
    rx = rx or m.ANCHOR
    total = 0
    for path in paths:
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except OSError:
            continue
        for line in lines:
            for raw in rx.findall(line):
                name = m.normalize(raw)
                if m.is_section_anchor(name) and not m.resolves(name, known):
                    total += 1
            cited = m.PROJECT_RFC_CITATION.findall(line)
            if any(num in numbers for num in cited):
                for raw in m.NUMBERED_ANCHOR.findall(line):
                    if raw not in known and not m.resolves(raw, known):
                        total += 1
    return total


def scan(m, paths, known, numbers, rx=None) -> set[tuple[str, str]]:
    """The linter's loop, calling its predicates. Not a reimplementation of them.

    Mirrors the loop in `main` deliberately: numbered anchors are gated on the
    line citing a project RFC, and `resolves` is used for every decision. The
    only parameter this varies is the extraction pattern, because measuring a
    different pattern is the point.
    """
    rx = rx or m.ANCHOR
    found: set[tuple[str, str]] = set()
    # The linter records a site as a repo-relative path plus a line number.
    # Absolute paths here would silently match nothing in the baseline and make
    # every finding look new, which is exactly the kind of quiet disagreement
    # this tool exists to rule out.
    for path in paths:
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except OSError:
            continue
        for lineno, line in enumerate(lines, 1):
            try:
                site = f"{path.relative_to(m.ROOT)}:{lineno}"
            except ValueError:
                site = f"{path}:{lineno}"
            for raw in rx.findall(line):
                name = m.normalize(raw)
                if not m.is_section_anchor(name):
                    continue
                if not m.resolves(name, known):
                    found.add((name, site))
            cited = m.PROJECT_RFC_CITATION.findall(line)
            if any(num in numbers for num in cited):
                for raw in m.NUMBERED_ANCHOR.findall(line):
                    if raw not in known and not m.resolves(raw, known):
                        found.add((raw, site))
    return found


def baseline_entries(m) -> set[tuple[str, str]]:
    entries: set[tuple[str, str]] = set()
    if not m.BASELINE_PATH.exists():
        return entries
    for line in m.BASELINE_PATH.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        name, _, site = line.partition("\t")
        entries.add((name, site if site else "*"))
    return entries


def qualify(m, names: set[str], known: set[str]) -> int:
    """How many unresolvable names a multi-word prefix would resolve.

    Mirrors the shape `resolves` already uses, so the number answers "what is
    left after the fallback that exists" rather than proposing a new one.
    """
    hits = 0
    for name in names:
        words = name.split()
        if any(m.resolves(" ".join(words[:cut]), known) for cut in range(len(words) - 1, 0, -1)):
            hits += 1
    return hits


# The identifier-qualifier rule, measured rather than assumed.
#
# `resolves` never trims an anchor below two words. That floor is what stops
# `§Gas Model` from resolving against a `§Gas` heading, and it is the correct
# default. It has one consequence that is wrong far more often than not:
# `§Adversary A5` does not resolve against a heading `Adversary`.
#
# A trailing token that is an identifier rather than an English word is
# distinguishable from that ambiguity, which is the rule proposed in mission
# `a-capitalized-word-after-a-section-name-is-prose-or-part-of-the-name-and-the-regex-cannot-tell`.
# Names reaching this function have already been through `normalize`, so they
# are lower case and the patterns below are written that way.
#
# Three exclusions, each of which a shape-only rule gets wrong. All three were
# found by listing the items the rule rescues rather than counting them:
#
#   * `L###` tails are CLAUDE.md convention violations, not qualifiers. A
#     resolver that accepts them deletes the only signal that they exist, since
#     `--line-refs` is a separate measurement and is not wired into CI.
#   * A bare common word is not a section name even when a heading carries it.
#     `§Top K` is `Top` plus the letter K; `§The V2` is nothing at all.
#   * A name containing `(` `)` or `:` is over-capture. Trimming it can appear
#     to rescue it while the real defect is the extraction boundary.
IDENTIFIER_TAIL = re.compile(r"^(?:[a-z]{1,4}\d+[a-z]?|[a-z]|\d+[a-z]?)$")
LINE_NUMBER_TAIL = re.compile(r"^l\d+$")
OVER_CAPTURE = re.compile(r"[():]")
GENERIC_HEADS = {"the", "top", "a", "an", "of", "in", "to", "is", "and", "or"}


def qualifier_breakdown(m, names: set[str], known: set[str]):
    """Partition the two-word identifier-shaped names by what the rule does.

    Returns a counter of verdicts and the names behind each, because the
    verdicts are the claim and the names are the check. A rule reported only as
    a count hides the items it rescues wrongly, which is the whole risk here.
    """
    verdicts: collections.Counter = collections.Counter()
    examples: dict[str, list[str]] = collections.defaultdict(list)

    def record(verdict: str, name: str) -> None:
        verdicts[verdict] += 1
        examples[verdict].append(name)

    for name in sorted(names):
        words = name.split()
        if len(words) != 2 or not IDENTIFIER_TAIL.match(words[-1]):
            continue
        head = words[0]
        if LINE_NUMBER_TAIL.match(words[-1]):
            record("excluded: L### line reference", name)
        elif OVER_CAPTURE.search(name):
            record("excluded: over-capture", name)
        elif head in GENERIC_HEADS:
            record("refused: generic head", name)
        elif m.resolves(head, known):
            record("rescued", name)
        else:
            record("refused: head is not a heading", name)
    return verdicts, examples


def report(m, label, paths, known, numbers, base, rx=None, qualifier=False) -> dict:
    findings = scan(m, paths, known, numbers, rx)
    new = {f for f in findings - base if f[0] not in {n for n, s in base if s == "*"}}
    names = {n for n, _ in findings}
    print(f"\n{label}")
    print(f"  files scanned          {len(paths)}")
    print(f"  occurrences            {scan_occurrences(m, paths, known, numbers, rx)}")
    print(f"  findings (name, site)  {len(findings)}")
    print(f"  distinct names         {len(names)}")
    print(f"  baselined (name, site) {len(findings & base)}")
    print(f"  would turn CI red      {len(new)}")
    print(f"  prefix-resolvable      {qualify(m, names, known)} of {len(names)}")
    if qualifier:
        verdicts, examples = qualifier_breakdown(m, names, known)
        print(f"  identifier-qualifier rule, two-word names only:")
        for verdict in sorted(verdicts, key=lambda v: (-verdicts[v], v)):
            shown = ", ".join(repr(n) for n in examples[verdict][:6])
            more = f" (+{len(examples[verdict]) - 6} more)" if len(examples[verdict]) > 6 else ""
            print(f"      {verdicts[verdict]:>4}  {verdict}{more}")
            print(f"            {shown}")
    return {"findings": findings, "new": new, "names": names}


LINE_REF = re.compile(r"§([A-Za-z][A-Za-z0-9._/ ]*?\b[lL]\d{2,}\b)")


def line_refs(paths) -> list[tuple[str, str]]:
    """Section references carrying a line number, which CLAUDE.md forbids.

    A separate measurement from anchor resolution, deliberately. A reference
    such as `§Test Vectors header L205` names a real section and appends a
    forbidden suffix. Folded into the dangling-anchor report it would be
    reported as a missing heading, and the reader sent to add a heading that
    already exists. The diagnosis has to name the actual defect.
    """
    hits: list[tuple[str, str]] = []
    for path in paths:
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except OSError:
            continue
        for lineno, line in enumerate(lines, 1):
            for raw in LINE_REF.findall(line):
                try:
                    site = f"{path.relative_to(ROOT)}:{lineno}"
                except ValueError:
                    site = f"{path}:{lineno}"
                hits.append((raw, site))
    return hits


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--tree", help="a directory to measure (not a CI-enforced tree)")
    ap.add_argument("--all", action="store_true", help="measure rfcs, docs and missions")
    ap.add_argument("--terminator", action="store_true", help="also the ) and : variant")
    ap.add_argument("--line-refs", action="store_true",
                    help="report section references carrying a line number")
    ap.add_argument("--qualifier", action="store_true",
                    help="partition two-word identifier-qualified names by verdict")
    args = ap.parse_args()

    m = load_linter()
    known = index(m)
    numbers = m.project_rfc_numbers()
    base = baseline_entries(m)

    gated = sorted(m.RFC_ROOT.rglob("*.md")) + [ROOT / "CLAUDE.md"]
    gated = [p for p in gated if p.is_file()]
    # The headline is a count of the INDEX, which is not the same set as the
    # files the linter SCANS. The index covers every tree; the scan covers rfcs
    # and one top-level file. Naming which is which is the whole point of this
    # tool, so the labels are read from the linter's own constants rather than
    # written here — an earlier version asserted the index was rfcs-only and
    # was wrong, inferred from a headline that happened to sit near the rfcs
    # count.
    index_desc = ", ".join(
        f"{pathlib.Path(s).name}/" if pathlib.Path(s).is_dir()
        else pathlib.Path(s).name
        for s in getattr(m, "INDEX_SOURCES", [])
    ) or "unknown"
    print(f"headings indexed: {len(known)} (from {index_desc})")
    print(f"project RFC numbers: {len(numbers)}")
    print(f"baseline entries: {len(base)}")

    scan_desc = ", ".join(
        f"{pathlib.Path(s).name}/" if pathlib.Path(s).is_dir()
        else pathlib.Path(s).name
        for s in list(getattr(m, "SCAN_SOURCES", [])) + list(getattr(m, "SCAN_FILES", []))
    ) or "unknown"

    # Pick the targets once, and label each one with what it is. An earlier
    # version reused a single `paths` variable across branches and labelled a
    # `--tree docs` run as "gated tree", which is the sort of quiet
    # mislabelling this tool exists to rule out.
    targets: list[tuple[str, list[pathlib.Path]]] = []
    if args.tree:
        root = ROOT / args.tree
        paths = sorted(root.rglob("*.md")) if root.is_dir() else [root]
        targets.append((f"{args.tree} (measured, not enforced)", paths))
    elif args.all:
        for name in ("rfcs", "docs", "missions"):
            targets.append((name, sorted((ROOT / name).rglob("*.md"))))
    else:
        targets.append(("gated tree (rfcs + CLAUDE.md), as CI runs it", gated))

    for label, paths in targets:
        if not label.startswith("gated tree") and label != "rfcs":
            print("")
            print(f"NOTE before {label}: the index covers {index_desc}.")
            print(f"      What CI scans for dangling references is narrower — {scan_desc}.")
            print("      Headings cited here already resolve, because this tree's own")
            print("      headings are indexed. The counts for it are what WOULD be")
            print("      reported if it were scanned, which is the coverage gap.")
        report(m, label, paths, known, numbers, base, qualifier=args.qualifier)
        if args.terminator:
            print(f"  -- with the ) and : terminator --")
            report(m, f"{label} + terminator", paths, known, numbers, base, TERMINATOR)

    if args.line_refs:
        print("\nline-number references in section cites (CLAUDE.md forbids these)")
        for label, paths in targets:
            hits = line_refs(paths)
            files = {site.rsplit(":", 1)[0] for _, site in hits}
            print(f"  {label}: {len(hits)} occurrence(s) across {len(files)} file(s)")
            for raw, site in hits[:12]:
                print(f"      §{raw}  {site}")
            if len(hits) > 12:
                print(f"      ... and {len(hits) - 12} more")

    print("\nThese numbers come from the linter's own functions.")
    print("A tree other than the gated one is measured, not enforced.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())