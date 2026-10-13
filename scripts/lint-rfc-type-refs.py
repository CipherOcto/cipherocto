#!/usr/bin/env python3
"""Type-reference lint for RFC code blocks — the defect class adversarial review misses.

Per R41 (2026-10-09), the dominant failure in this repository's RFCs is not a
wrong argument. It is **an authoritative-looking type declaration that no crate
contains.** Three times in one session an RFC described a registry with a
`populate()` gate the registry does not have, an `OperationFact` missing a
variant the code matches on, and two error enums that exist nowhere in the repo
— each written with more confidence than the surrounding prose, and each
checkable in three seconds with a grep.

This lint makes that check mechanical. It extracts type-shaped identifiers from
RFC fenced code blocks and asks whether each one resolves:

  1. to a symbol defined in any crate under `crates/` (the landed truth), or
  2. to a declaration inside an RFC itself (a type this document is specifying),
     or
  3. to a known std/primitive, which is ignored.

What is left is a type referenced as though it existed but defined nowhere.
Those are reported. Because the pre-existing corpus contains many, findings are
baselined and the default run fails only on new ones.

**The baseline is keyed on (name, site), not on name.** That is the whole
difference between a suppression list and a record of debt. A name-only
baseline made every *new* occurrence of an already-known-unresolved name
invisible: an RFC written today referencing `ActionPayload` — undefined
anywhere — produced exit 0 and no mention of the file, because the name was in
the baseline and the baseline was never asked about places. Verified by
planting exactly that and watching it pass, then watching it fail after the
change.

Deliberate limit: this checks *existence*, not *shape*. A type that exists with
the wrong variants, or a function with the wrong signature, passes. Catching
that needs the declaration compared field-by-field against the crate, which is a
semantic check and not a lint. Existence is the cheap half, and it is the half
that was failing.

Run: python3 scripts/lint-rfc-type-refs.py [--strict] [--write-baseline]
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RFC_ROOT = ROOT / "rfcs"
CRATES_ROOT = ROOT / "crates"
BASELINE_PATH = Path(__file__).resolve().parent / "lint-rfc-type-refs-baseline.txt"

# Fenced code blocks tagged rust (and cairo, which has its own declaration form).
CODE_BLOCK = re.compile(r"```(?:rust|cairo)\n(.*?)```", re.DOTALL)

# Declarations an RFC may legitimately make itself.
RFC_DECL = re.compile(
    r"^\s*(?:pub\s+|pub\(crate\)\s+)?"
    r"(?:struct|enum|trait|type|union)\s+([A-Z]\w*)",
    re.MULTILINE,
)

# Declarations that exist in a crate.
CRATE_DECL = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?"
    r"(?:struct|enum|trait|type|union|fn|macro_rules!|const|static)\s+"
    r"([A-Za-z_]\w*)",
    re.MULTILINE,
)
# Cairo declarations.
CAIRO_DECL = re.compile(r"^\s*(?:pub\s+)?(?:struct|enum|trait)\s+([A-Za-z_]\w*)", re.MULTILINE)

# A type-shaped token. Deliberately narrow:
#   * starts with an uppercase letter, and
#   * contains at least a second uppercase letter, and
#   * is NOT entirely uppercase.
#
# The last two clauses matter more than they look. Most of the noise in this
# corpus is SCREAMING_CASE pseudo-code (`AIR`, `AGGREGATOR_SHARE`, `API`) and
# single-capital method names (`Abort`), neither of which is the failure mode
# being hunted. The defect R41 found repeatedly is a CamelCase *type* asserted
# with confidence and defined nowhere.
IDENT = re.compile(r"\b([A-Z][A-Za-z0-9_]*)\b")
# Scoped paths are left intact. An earlier version stripped `A::B::` before
# tokenising, intending to keep only the final segment — but that removes the
# type itself in the common `Type::method()` and `Enum::Variant` forms, which
# are precisely the references this lint exists to catch. Path segments that are
# modules are lowercase and never reach `is_type_shaped`.
SCOPED = re.compile(r"[A-Za-z_]\w*(?:::<[^>]*>)?::")


def is_type_shaped(name: str) -> bool:
    if len(name) < 2:
        return False
    if name.isupper():  # AIR, API, AGGREGATOR_SHARE
        return False
    return sum(1 for ch in name if ch.isupper()) >= 2


# An enum body declares its variants implicitly — `AdmissionDenied,` inside
# `enum AdmissionError {` is a declaration, not a reference. Without this the
# lint drowns in error variants, which is where most of the corpus's
# CamelCase tokens live.
ENUM_OPEN = re.compile(r"\benum\s+\w+\s*\{")


def enum_variant_names(block: str) -> set[str]:
    """CamelCase names declared inside an `enum { .. }` body in this block."""
    declared: set[str] = set()
    for match in ENUM_OPEN.finditer(block):
        depth = 0
        start = match.end() - 1
        for index in range(start, len(block)):
            char = block[index]
            if char == "{":
                depth += 1
            elif char == "}":
                depth -= 1
                if depth == 0:
                    body = block[start + 1 : index]
                    break
        else:
            continue
        # Variant lines look like `    Name,` or `    Name { .. }` or `    Name(..),`
        for line in body.splitlines():
            stripped = line.strip()
            if not stripped or stripped.startswith(("//", "#", "///")):
                continue
            head = stripped.split("{", 1)[0].split("(", 1)[0].split(",", 1)[0].strip()
            if is_type_shaped(head):
                declared.add(head)
    return declared

# Names that are language vocabulary, not project types. A type-shaped name in
# this set is never interesting.
IGNORED = {
    # std / core vocabulary
    "Result", "Option", "Some", "None", "Ok", "Err", "String", "Vec", "Box", "Arc",
    "Rc", "HashMap", "BTreeMap", "HashSet", "BTreeSet", "Hash", "Eq", "PartialEq",
    "Ord", "PartialOrd", "Clone", "Copy", "Debug", "Default", "Drop", "Iterator",
    "Into", "From", "TryFrom", "TryInto", "AsRef", "AsMut", "Deref", "Send", "Sync",
    "Sized", "Fn", "FnMut", "FnOnce", "ToString", "ToOwned", "Display", "Error",
    "Self", "Self_", "Ordering", "Duration", "Instant", "SystemTime", "UNIX_EPOCH",
    # syntax / pseudo-code markers that survive tokenisation
    "RFC", "TODO", "NOTE", "JSON", "MUST", "SHOULD", "MAY", "KEEP", "REMOVE",
    # doc-comment idioms that appear inside the blocks we parse
    "MUST_NOT", "SHOULD_NOT", "MAYBE",
}


def normalize_ignored() -> set[str]:
    return {name.lower() for name in IGNORED}


def strip_code_comments(text: str) -> str:
    """Drop Rust line/block comments so a type named only in prose is not counted."""
    text = re.sub(r"//[^\n]*", "", text)
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.DOTALL)
    return text


def strip_string_literals(text: str) -> str:
    """Blank out string literal contents, keeping the surrounding code.

    A CamelCase token inside a string is data, not a type. RFC code blocks are
    full of illustrative values, and one of them is a Telegram bot token:

        bot_token = "1234567890:ABCdefGHIjklMNOpqrsTUVwxyz"

    `ABCdefGHIjklMNOpqrsTUVwxyz` is not a type reference, it is a sample
    credential in an example, and reporting it put a baselined entry on the
    record that describes nothing. Measured across the baseline, 42 of 729
    entries were string content rather than code.

    Rust raw strings (`r"..."`, `r#"..."#`) are handled before the ordinary
    forms so their contents are not rescanned as code. Lifetimes such as
    `'static` are consumed by the same rule and that is harmless: a lifetime is
    lowercase and can never match `IDENT`, which requires a leading capital.
    """
    text = re.sub(r'r#*"[^"]*"#*', '""', text)
    text = re.sub(r'"(?:[^"\\\n]|\\.)*"', '""', text)
    text = re.sub(r"'(?:[^'\\\n]|\\.)*'", "''", text)
    return text


def rfc_declared_types() -> set[str]:
    declared: set[str] = set()
    for path in RFC_ROOT.rglob("*.md"):
        text = strip_code_comments(path.read_text(encoding="utf-8"))
        for block in CODE_BLOCK.findall(text):
            declared |= set(RFC_DECL.findall(block))
            declared |= enum_variant_names(block)
    return declared


def crate_symbols() -> set[str]:
    symbols: set[str] = set()
    for path in list(CRATES_ROOT.rglob("*.rs")) + list(CRATES_ROOT.rglob("*.cairo")):
        if "/target/" in str(path):
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        text = strip_code_comments(text)
        symbols |= set(CRATE_DECL.findall(text))
        symbols |= set(CAIRO_DECL.findall(text))
        # Variants are declarations too. They are generic enough that this
        # over-resolves — `Denied` existing in any crate satisfies a reference
        # to `Denied` — which is the correct bias for an existence check.
        symbols |= enum_variant_names(text)
    return symbols


def known() -> set[str]:
    return rfc_declared_types() | crate_symbols() | IGNORED


def references(text: str) -> set[str]:
    """Type-shaped identifiers referenced inside RFC code blocks."""
    found: set[str] = set()
    for block in CODE_BLOCK.findall(strip_string_literals(strip_code_comments(text))):
        for name in IDENT.findall(block):
            if is_type_shaped(name):
                found.add(name)
    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--strict", action="store_true",
                        help="report everything and always fail")
    parser.add_argument("--write-baseline", action="store_true",
                        help="rewrite the baseline from current findings")
    args = parser.parse_args()

    if not RFC_ROOT.is_dir():
        # Not a pass — see the note in lint-rfc-anchors.py. A missing corpus
        # is a broken gate, not a clean one.
        print(f"no rfcs/ directory at {RFC_ROOT}", file=sys.stderr)
        return 2

    resolved = known()
    ignored_lower = normalize_ignored()

    unresolved: dict[str, list[str]] = {}
    for path in sorted(RFC_ROOT.rglob("*.md")):
        text = strip_code_comments(path.read_text(encoding="utf-8"))
        for name in references(text):
            if name in resolved or name.lower() in ignored_lower:
                continue
            unresolved.setdefault(name, []).append(str(path.relative_to(ROOT)))

    # (name, site) pairs, not names. A name-only baseline makes every *new*
    # occurrence of an already-known-unresolved name invisible: an RFC written
    # today that references `ActionPayload` — undefined anywhere — produced exit
    # 0 and no mention of the file, because the name is in the baseline and the
    # baseline was never asked about places.
    #
    # The thing being measured is violations, not vocabulary. 576 known-bad
    # names in a baseline is a suppression list; 576 known-bad *sites* is a
    # record of where the debt is, and a new site is new debt.
    findings = {(name, site) for name, sites in unresolved.items() for site in sites}

    if args.write_baseline:
        BASELINE_PATH.write_text(
            "\n".join(f"{name}\t{site}" for name, site in sorted(findings))
            + ("\n" if findings else ""),
            encoding="utf-8",
        )
        print(f"baseline written: {len(findings)} unresolved reference(s) "
              f"across {len(unresolved)} name(s)")
        return 0

    if not unresolved:
        if not resolved:
            # Zero symbols indexed and zero references found is agreement
            # between two empty sets, not evidence that references resolve.
            print("nothing indexed and nothing referenced — corpus is empty",
                  file=sys.stderr)
            return 2
        print(f"type-reference lint clean — {len(resolved)} symbols indexed")
        return 0

    baseline: set[tuple[str, str]] = set()
    if BASELINE_PATH.exists():
        for line in BASELINE_PATH.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if not line:
                continue
            name, _, site = line.partition("\t")
            if not site:
                # A bare name from a baseline written before this was
                # site-keyed. Kept as a name-only suppression so the migration
                # does not turn the whole corpus red on one run, but it is
                # reported below so the debt is visible rather than silent.
                baseline.add((name, "*"))
            else:
                baseline.add((name, site))

    wildcard = {n for n, site in baseline if site == "*"}
    new_findings = sorted(f for f in findings - baseline if f[0] not in wildcard)
    fixed = len({f for f in baseline if f not in findings and f[1] != "*"})

    print(f"symbols indexed: {len(resolved)}")
    print(f"unresolved in RFC code blocks: {len(findings)} reference(s) over "
          f"{len(unresolved)} name(s)")
    if wildcard:
        print(f"note: {len(wildcard)} name(s) in the baseline are still name-only "
              f"and cannot see a new site — re-run with --write-baseline")
    if fixed:
        print(f"baselined references now resolved: {fixed} — shrink with --write-baseline")

    if new_findings:
        by_name: dict[str, list[str]] = {}
        for name, site in new_findings:
            by_name.setdefault(name, []).append(site)
        print(f"\nNEW type reference(s) with no declaration anywhere:\n")
        for name, sites in sorted(by_name.items()):
            shown = ", ".join(sorted(sites)[:3])
            more = f" (+{len(sites) - 3} more)" if len(sites) > 3 else ""
            print(f"  {name}\n      {shown}{more}")

    if args.strict and unresolved:
        return 1
    if new_findings:
        print(
            "\nA type referenced in an RFC code block and defined nowhere is the\n"
            "failure mode R41 found repeatedly: authoritative prose describing a\n"
            "crate that does not exist. Either declare it in this RFC, cite the\n"
            "crate that has it, or delete it."
        )
        return 1

    print("\nno new unresolved type references.")
    return 0


if __name__ == "__main__":
    sys.exit(main())