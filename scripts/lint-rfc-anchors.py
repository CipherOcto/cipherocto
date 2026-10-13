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

**The baseline is keyed on (name, site), not on name.** A name-only baseline
cannot see a *new place* that cites an already-known-dangling anchor: a section
written today citing `§145` produced exit 0 and no mention of the file, because
the name was in the baseline and the baseline was never asked about lines.
Verified by planting that citation against the old baseline and watching it
pass, then watching it fail here. 576 of 729 entries in the type-reference
linter's baseline had the same shape and were fixed the same way.
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
# BLUEPRINT section and not an RFC section. CLAUDE.md is in this list for the
# same reason: it is the document that *defines* the `§section_name`
# convention, so a great many references legitimately target its sections and
# none of them could resolve while it went unindexed.
INDEX_SOURCES = [RFC_ROOT, ROOT / "docs", ROOT / "missions", ROOT / "CLAUDE.md"]

# Files checked for dangling anchors, as opposed to indexed as sources of valid
# anchor names. These were the same set, which meant CLAUDE.md was indexed as a
# source of headings while the references *inside* it were never verified.
# Proved by planting the identical invented anchor in both trees: reported in
# rfcs, silently ignored in docs.
#
# CLAUDE.md is checked separately rather than added to SCAN_SOURCES below because
# it is a single file at the repo root, not a directory to walk, and because it is
# the first file any agent reads. It carried three dangling references until this
# was fixed: `§The RFC Process` (cited a BLUEPRINT section that does not exist,
# when the heading is local), `§Multi-Mission Decomposition` (exists in no
# document), and `§RFC Reference Conventions Reaffirmed` (the heading carries a
# parenthesised suffix).
SCAN_FILES = [ROOT / "CLAUDE.md"]
SCAN_SOURCES = [RFC_ROOT]

# `§Section Name` — the name runs to a backtick, pipe, newline, comma, an em or
# en dash, or the start of the next sentence. The comma terminator matters:
# `§A, §B` is two anchors, not one called "a, b". The dash terminators matter
# because this repo writes `§Data Structures — a note`, and without them the
# dash is swallowed into the name and every such reference reads as dangling.
ANCHOR = re.compile(
    r"§([A-Z][A-Za-z0-9][^`|,\n—–]*?)(?=`|\||,|—|–|\n|$|\s+[a-z]|\s*\.)"
)

# `§6`, `§3.5.7` — a purely numeric anchor is a section number, and CLAUDE.md
# explicitly permits numbered references "if resolvable". ANCHOR cannot express
# it, by design: its first character class is `[A-Z]`, because a lone capital is
# far more often an adversarial-review finding id (`§A7`) than a section. A
# digit is ambiguous in the same way — `§593` is as likely to be a line number
# in prose as a section — so numeric anchors are matched on their own and gated
# separately in `is_section_anchor` rather than being trusted by ANCHOR.
#
# The negative lookahead forces the longest match: for `§6.1 Public` the shorter
# `6` is rejected because a dot follows, so `6.1` wins.
# The trailing lookahead excludes a word character, not a period. Excluding the
# period as well meant `§9.9.9.` — a numeric anchor at the end of a sentence,
# which is how prose usually writes one — matched nothing at all: the regex
# backtracked from `9.9.9` to `9.9` to `9` and every candidate was rejected by
# the `.` that followed. Verified by planting `§9.9.9.` and `§9.9.9` in the same
# run: the first produced exit 0 and no finding, the second exit 1 naming the
# file. The word-character exclusion still rejects `§3.5.7a`.
NUMBERED_ANCHOR = re.compile(r"§(\d+(?:\.\d+)*)(?![\w])")

# A citation of a four-digit RFC id, the shape this project uses. External
# specifications are cited the same way in prose (`RFC 8032 §5.1.6`), so the id
# alone does not prove ownership — it is checked against the corpus below.
PROJECT_RFC_CITATION = re.compile(r"RFC-(\d{4})\b")

# The leading number of a heading, so `## 6.1 Public surface additions` is
# addressable as `§6.1`. Deliberately matches the number only, never the title,
# so adding numeric support cannot make a *title* reference resolve that it did
# not before.
SECTION_NUMBER = re.compile(r"^(\d+(?:\.\d+)*)(?![\d.])")

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

    A purely numeric token is the exception: `§6.1` is a section number, and
    CLAUDE.md sanctions numbered references precisely when they resolve. The
    ambiguity that motivates the multi-word rule — a digit might be a line
    number in prose — is why numeric anchors are matched by their own regex and
    verified against a heading rather than waved through.
    """
    return len(name.split()) >= 2 or bool(NUMBERED_ANCHOR.match("§" + name))


def heading_names(path: Path) -> set[str]:
    names: set[str] = set()
    for line in path.read_text(encoding="utf-8").splitlines():
        match = HEADING.match(line)
        if match:
            raw = normalize(match.group(1))
            for form in candidates(raw):
                names.add(form)
            number = SECTION_NUMBER.match(raw)
            if number:
                names.add(number.group(1))
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
    # The other direction: the anchor may carry a qualifier the heading does
    # not. This corpus writes `§Implementation Phases Phase 1` against a
    # `## Implementation Phases` heading, and `§Adversarial Review Process Dry
    # Run` against `## Adversarial Review Process`, far more often than it
    # writes the heading verbatim. Without this, two thirds of everything the
    # lint reported was an anchor that named a real section with extra prose
    # glued on.
    #
    # Only word-prefixes of two or more words are tried, so this cannot resolve
    # an anchor down to a single common word.
    words = longest.split()
    for cut in range(len(words) - 1, 1, -1):
        if " ".join(words[:cut]) in known:
            return True
    return False


def project_rfc_numbers() -> set[str]:
    """The four-digit ids of the RFCs this repository owns.

    Used to tell `RFC-0957 §3.5.7` (this project, checkable) from
    `RFC 8032 §5.1.6` (an external specification, not ours to resolve). Derived
    from the corpus rather than hardcoded, so a new RFC is covered by landing
    the file and a renamed or deleted one stops being claimed.
    """
    found: set[str] = set()
    for path in RFC_ROOT.rglob("*.md"):
        match = re.match(r"(\d{4})-", path.name)
        if match:
            found.add(match.group(1))
    return found


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
    rfc_files += [p for p in SCAN_FILES if p.is_file()]
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
    project_numbers = project_rfc_numbers()
    for path in rfc_files:
        for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            for raw in ANCHOR.findall(line):
                name = normalize(raw)
                if not is_section_anchor(name):
                    continue
                if not resolves(name, known):
                    unresolved.append((path, lineno, name))
            # Numbered anchors are collected separately so a numeric reference is
            # checked on its own terms instead of being lost in ANCHOR's
            # leading-letter assumption. Deduped against the pass above so a
            # reference both regexes can see is reported once, not twice.
            #
            # A numeric anchor carries no RFC number of its own, so it cannot say
            # whether it points into this project or into an external spec. That
            # is not hypothetical: `RFC 8032 §5.1.6` cites the IETF Ed25519
            # document, whose headings are not indexed here. Requiring the line
            # to cite a project RFC is what keeps an external reference from
            # being reported as a dangling project section.
            cited = PROJECT_RFC_CITATION.findall(line)
            if any(num in project_numbers for num in cited):
                for raw in NUMBERED_ANCHOR.findall(line):
                    if raw not in known and not resolves(raw, known):
                        unresolved.append((path, lineno, raw))

    by_name: dict[str, list[str]] = {}
    for path, lineno, name in unresolved:
        by_name.setdefault(name, []).append(f"{path.relative_to(ROOT)}:{lineno}")

    # (name, site) pairs, not names. A name-only baseline cannot see a *new*
    # place that cites an already-known-dangling anchor: a document written
    # today citing `§ac-8 2-cycle` produced exit 0 and no mention of the file,
    # because the name was in the baseline and the baseline was never asked
    # about lines. The thing being measured is violations, not vocabulary.
    findings = {(name, site) for name, sites in by_name.items() for site in sites}

    if args.write_baseline:
        BASELINE_PATH.write_text(
            "\n".join(f"{name}\t{site}" for name, site in sorted(findings))
            + ("\n" if findings else ""),
            encoding="utf-8",
        )
        print(f"baseline written: {len(findings)} dangling reference(s) across "
              f"{len(by_name)} distinct name(s)")
        return 0

    if not unresolved:
        print(f"anchor lint clean — {len(known)} headings indexed across docs")
        return 0

    baseline: set[tuple[str, str]] = set()
    if BASELINE_PATH.exists():
        for line in BASELINE_PATH.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if not line:
                continue
            name, _, site = line.partition("\t")
            if not site:
                # A bare name from a pre-site-keyed baseline. Kept as a
                # name-only suppression so the migration does not turn the whole
                # corpus red on one run, and reported so the debt stays visible.
                baseline.add((name, "*"))
            else:
                baseline.add((name, site))

    wildcard = {n for n, site in baseline if site == "*"}
    new_findings = sorted(f for f in findings - baseline if f[0] not in wildcard)

    # Two different populations, and this line conflated them for a long time.
    # `baselined_count` is how much of the current debt the baseline already
    # covers. `stale` is how much of the baseline no longer matches any current
    # finding, which is debt that has been fixed or has moved.
    #
    # The printed value was `stale` under the label "baselined", so a fully
    # covered run — 588 findings, 588 baselined, 0 stale — printed
    # `baselined: 0 of 588`, which reads as though the baseline were doing
    # nothing at all. A reader cannot tell from that line whether the gate is
    # holding or the baseline has been abandoned.
    baselined_count = len(findings & baseline)
    stale = len({f for f in baseline if f not in findings and f[1] != "*"})

    print(f"anchors indexed: {len(known)} headings")
    if wildcard:
        print(f"note: {len(wildcard)} name(s) in the baseline are still name-only "
              f"and cannot see a new site — re-run with --write-baseline")
    print(f"dangling: {len(unresolved)} reference(s) over {len(by_name)} distinct name(s)")
    print(f"baselined: {baselined_count} of {len(findings)} current reference(s) "
          f"are recorded as known")
    if stale:
        print(f"  -> {stale} baseline entry(ies) no longer match a finding; "
              f"shrink with --write-baseline")

    if new_findings:
        by_new: dict[str, list[str]] = {}
        for name, site in new_findings:
            by_new.setdefault(name, []).append(site)
        print(f"\nNEW dangling anchor(s) — not in baseline:\n")
        for name, sites in sorted(by_new.items()):
            shown = ", ".join(sorted(sites)[:4])
            extra = f" (+{len(sites) - 4} more)" if len(sites) > 4 else ""
            print(f"  §{name}\n      {shown}{extra}")

    if args.strict and by_name:
        print(f"\n--strict: {len(by_name)} distinct dangling anchor name(s) outstanding.")
        return 1

    if new_findings:
        print(
            "\nA dangling anchor in a normative 'settled in RFC-NNNN §X' sentence "
            "is an\nunauditable control: the reader is sent nowhere."
        )
        return 1

    print("\nno new dangling anchors.")
    return 0


if __name__ == "__main__":
    sys.exit(main())