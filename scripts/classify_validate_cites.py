#!/usr/bin/env python3
"""Classify validate_cites.sh INVALID findings using the script's own logic.

The question this answers is whether `validate_cites.sh` is worth keeping or
should be deleted. It reports 1335 INVALID findings; if almost all of them are
one known ambiguity class then the fix is a rule, and if they are spread across
many causes then the script is reporting rot rather than a defect.

Nothing here reimplements the linter. The findings are read from the script's
own output, and the heading list for each cited file is produced by the exact
command `get_rfc_sections` uses:

    grep -oE '^##+ [^#].*$' FILE | sed 's/^##* //'

The rule under test is the one measured in
`missions/open/the-qualifier-rule-is-not-the-prerequisite-...`: a trailing
token that is an identifier rather than an English word is a qualifier, with
three exclusions. This measures how many of the script's findings that rule
would absorb, and — as importantly — how many of the remaining ones it would
absorb wrongly.
"""

from __future__ import annotations

import collections
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

# Names arrive already normalized by the script's own normalize_section.
IDENTIFIER_TAIL = re.compile(r"^(?:[a-z]{1,4}\d+[a-z]?|[a-z]|\d+[a-z]?)$")
LINE_NUMBER_TAIL = re.compile(r"^l\d+$")
GENERIC_HEADS = {"the", "top", "a", "an", "of", "in", "to", "is", "and", "or"}

# INVALID [section missing]: FILE:LINE: CITE (looking for 'SEC' in RFCPATH)
FINDING = re.compile(
    r"^INVALID \[section missing\]: (?P<file>[^:]+):(?P<line>\d+): "
    r"(?P<cite>.+?) \(looking for '(?P<sec>[^']*)' in (?P<rfc>.+)\)$"
)

_sections_cache: dict[str, str] = {}
_paths_cache: dict[str, list[str]] = {}


def rfc_siblings(rfc_path: str) -> list[str]:
    """Every tracked file that carries the same RFC number.

    RFC numbers are not unique in this repository. `RFC-0959` is carried by
    four separate files. `find_rfc_path` resolves to one of them, so a cite
    whose section exists in a sibling is reported as broken because the wrong
    file was opened. Recording the siblings is what separates that from a
    genuinely dangling citation.
    """
    if rfc_path in _paths_cache:
        return _paths_cache[rfc_path]
    name = pathlib.Path(rfc_path).name
    # The number is the leading digits of the filename, e.g. 0959, 0011.
    match = re.match(r"(\d{4})", name)
    if not match:
        _paths_cache[rfc_path] = [rfc_path]
        return _paths_cache[rfc_path]
    num = match.group(1)
    try:
        out = subprocess.run(
            ["bash", "-c", f"find {ROOT / 'rfcs'} -iname '{num}*.md' | sort"],
            capture_output=True, text=True, check=False,
        ).stdout.split()
    except OSError:
        out = []
    rel = [str(pathlib.Path(p).relative_to(ROOT)) for p in out]
    _paths_cache[rfc_path] = rel or [rfc_path]
    return _paths_cache[rfc_path]


def sections_of(rfc_path: str) -> str:
    """The heading blob, built by the same pipeline get_rfc_sections uses."""
    if rfc_path in _sections_cache:
        return _sections_cache[rfc_path]
    path = ROOT / rfc_path
    if not path.is_file():
        _sections_cache[rfc_path] = ""
        return ""
    out = subprocess.run(
        ["bash", "-c",
         f"grep -oE '^##+ [^#].*$' {path!s} 2>/dev/null | sed 's/^##* //' | tr -d '\\r'"],
        capture_output=True, text=True, check=False,
    ).stdout
    _sections_cache[rfc_path] = out
    return out


def resolves(name: str, blob: str) -> bool:
    """The script's own test: is the normalized name a substring of the blob."""
    return bool(name) and name in blob


def heading_exact(name: str, blob: str) -> bool:
    """Stricter test: does some heading *be* this section, not merely contain it.

    The script's substring semantics are generous by design, and that
    generosity matters for attribution. `§Forward` "resolving" because some
    heading in a sibling happens to contain the word Forward is not evidence
    that a citation is sound; it is evidence that a loose match fired. Counting
    those as recovered citations would overstate the size of the fix, so the
    two are reported separately.

    A heading qualifies only when it *is* the cited section, once a trailing
    parenthetical qualifier is dropped. Prefix matching is deliberately not
    accepted: an earlier version allowed `Phase 1: Data Structures` to satisfy
    a citation of `§Phase`, which is not a recovery but a different section
    being silently substituted for the one that was written. Every heading
    this rule now accepts is a heading a reader would agree is the one meant.
    """
    target = name.strip().strip("§").strip()
    for line in blob.splitlines():
        h = line.strip().lstrip("#").strip().lstrip("§").strip()
        h = re.sub(r"^\d+(\.\d+)*\.?\s+", "", h)
        h = re.sub(r"[*`_]", "", h).strip()
        if not h:
            continue
        h = re.sub(r"\s*\(.*$", "", h).strip()
        if h == target:
            return True
    return False


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: classify_validate_cites.py <validate_cites output>")
        return 2
    text = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")

    findings = []
    for line in text.splitlines():
        m = FINDING.match(line)
        if m:
            findings.append(m.groupdict())

    verdicts: collections.Counter = collections.Counter()
    examples: dict[str, list[str]] = collections.defaultdict(list)
    by_file: collections.Counter = collections.Counter()

    for f in findings:
        # Deliberately NOT lowercased. The script prints `$sec_normalized`
        # and tests it with a case-sensitive bash substring match, so `Gas
        # Model` is compared against heading text as written. Lowercasing here
        # would make this classifier answer a different question from the one
        # it claims to measure.
        sec = f["sec"].strip()
        words = sec.split()
        blob = sections_of(f["rfc"])
        siblings = rfc_siblings(f["rfc"])
        verdict = None

        # First and most decisive: is the section present in a sibling file
        # carrying the same RFC number? If so the citation is fine and only
        # the file selection was wrong.
        if len(siblings) > 1 and any(
            heading_exact(sec, sections_of(sib)) for sib in siblings if sib != f["rfc"]
        ):
            verdict = "recovered: sibling has this exact heading (path ambiguity)"
        elif len(siblings) > 1 and any(
            resolves(sec, sections_of(sib)) for sib in siblings if sib != f["rfc"]
        ):
            verdict = "weak: name occurs inside a sibling heading (substring only)"
        elif len(words) >= 2 and IDENTIFIER_TAIL.match(words[-1].lower()):
            head = " ".join(words[:-1])
            if LINE_NUMBER_TAIL.match(words[-1].lower()):
                verdict = "qualifier excluded: L### line reference"
            elif head.lower() in GENERIC_HEADS:
                verdict = "qualifier refused: generic head"
            elif resolves(head, blob):
                verdict = "qualifier rule WOULD RESOLVE"
            else:
                verdict = "qualifier rule does not help"

        if verdict is None:
            # Not identifier-shaped. Is it a plain substring near-miss?
            head = " ".join(words[:2])
            if len(words) >= 3 and resolves(head, blob):
                verdict = "near-miss: 2-word prefix is a heading"
            elif resolves(" ".join(words[:-1]), blob):
                verdict = "near-miss: dropping the last word resolves it"
            else:
                verdict = "genuinely missing"

        verdicts[verdict] += 1
        by_file[f["file"]] += 1
        if len(examples[verdict]) < 4:
            examples[verdict].append(
                f"{sec!r} looked in {f['rfc'].rsplit('/', 1)[-1]} "
                f"({len(siblings)} file(s) share this number)"
            )

    total = len(findings)
    print(f"INVALID findings read from the linter's own output : {total}")
    print(f"distinct citing files                               : {len(by_file)}")
    print()
    for verdict, n in verdicts.most_common():
        print(f"  {n:>5}  {verdict}")
        for ex in examples[verdict]:
            print(f"           {ex}")
    rescued = verdicts.get("qualifier rule WOULD RESOLVE", 0)
    print()
    if total:
        print(f"the qualifier rule would absorb {rescued} of {total} "
              f"({100 * rescued / total:.1f}%)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())