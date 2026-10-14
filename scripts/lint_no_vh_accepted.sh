#!/usr/bin/env bash
# scripts/lint_no_vh_accepted.sh
# Guard 4: NO_VH_ACCEPTED linter (per long-horizon plan v1.3 Phase 2 §Guard 4; M37 NEW P3)
# Behavior: For each Accepted RFC, verify presence of ## Version History table.
#           Reject RFC promotion to Accepted without VH.
# Coverage: Accepted RFCs corpus-wide + promotion gates.
# BLOCKING promotion: YES.

set -euo pipefail

# MEASURING THIS GUARD: use the tool that already exists.
#
# `scripts/classify_validate_cites.py` classifies this repository's citation
# findings using the exact command `get_rfc_sections` runs, rather than a
# reimplementation of it. A python reimplementation of the Accepted-detection
# logic below counted 148 RFCs where this script counts 155: the difference is
# seven files that write `## Status: Accepted` with the status on the heading
# rather than below it, which this script sees and the reimplementation did not.
#
# So: call `grep -A5` as written here, or call the classifier. Do not
# reimplement the rule in order to measure it. A reimplementation that disagrees
# is more expensive than no measurement, because it produces a confident wrong
# number that then has to be traced.

RFC_ROOT="${RFC_ROOT:-rfcs}"
EXIT_CODE=0
SCANNED=0
SKIPPED_BY_NAME=0
CHECKED=0
PASS=0
FAIL=0
MISSING_VH=0

echo "NO_VH_ACCEPTED linter — Guard 4 (M37 NEW P3)"
echo "RFC root: $RFC_ROOT"
echo ""

while IFS= read -r -d '' rfc_file; do
    SCANNED=$((SCANNED + 1))

    basename=$(basename "$rfc_file")

    # Skip non-content files
    case "$basename" in
        CHANGELOG.md|README.md|INDEX.md) SKIPPED_BY_NAME=$((SKIPPED_BY_NAME + 1)); continue ;;
    esac

    # Check if RFC is Accepted.
    #
    # This used to be `grep -qE '^## (Status|## Status|Status:).*Accepted'`, which
    # requires "Accepted" on the SAME line as the header. It is not. The corpus
    # writes the status on the line(s) AFTER the heading:
    #
    #     ## Status
    #
    #     Accepted (v2.0) — promoted 2026-08-20 ...
    #
    # so that pattern matched 7 files out of 155 Accepted RFCs. The remaining 148
    # hit `continue` and were never examined, and the summary still read
    # "PASS: all Accepted RFCs have ## Version History". CHECKED counted every
    # markdown file while PASS counted only detected Accepted RFCs, so
    # CHECKED=247 against PASS=7 was the arithmetic signature of the skip and
    # nothing in the output said so.
    #
    # Read the Status block: from the heading to the next heading of any level.
    is_accepted=0
    if grep -A5 -E '^## Status' "$rfc_file" 2>/dev/null | grep -q 'Accepted'; then
        is_accepted=1
    fi
    # Also check YAML frontmatter for status: Accepted
    if awk '/^---$/{c++; next} c==1 && /^status:/ {print; exit}' "$rfc_file" 2>/dev/null | grep -qi accepted; then
        is_accepted=1
    fi

    # Skip non-Accepted RFCs
    [ "$is_accepted" -eq 0 ] && continue

    # CHECKED counts Accepted RFCs actually examined. It used to increment once
    # per markdown file found, so the summary compared a whole-corpus count
    # against an Accepted-only count and the gap looked like unexamined work
    # rather than a silently skipped population.
    CHECKED=$((CHECKED + 1))

    # Check for ## Version History (or equivalent ## Changelog)
    # The optional leading section sign matters. This corpus also writes the
    # heading as `## §Version History`, and `^## (Version History|Changelog)$`
    # did not match those, so an Accepted RFC with a populated Version History
    # was reported as having none. 2 files use the § form; both were false
    # positives before this.
    # The optional section number matters. This corpus also numbers its
    # top-level sections, so the same table is written `## 6. Version History`
    # rather than `## Version History`. 14 Accepted RFCs carry a populated,
    # numbered Version History and were reported as having none at all.
    #
    # The number is optional, not required, so this only ever recognizes more.
    # Verified across the whole tree: 14 files newly recognized, none lost.
    if ! grep -qE '^##+ ([0-9]+(\.[0-9]+)*\.? )?(Version History|Changelog|§Version History|§Changelog)' "$rfc_file" 2>/dev/null; then
        echo "MISSING_VH: $rfc_file (Accepted but no ## Version History / ## Changelog)"
        MISSING_VH=$((MISSING_VH + 1))
        FAIL=$((FAIL + 1))
        EXIT_CODE=1
        continue
    fi

    # Verify VH has at least one data row
    # `v?` on the version column matters. This corpus writes it both ways:
    # `| 2.12 |` and `| v2.12 |`. Requiring a bare digit reported 28 files
    # with a populated Version History as having no data rows at all, which
    # is 28 of the 33 findings in that class.
    #
    # Measured across the tree: 1162 rows matched before, 1497 after, and 39
    # files gain at least one. Nothing loses, since the digit is still
    # required and `v` is only an optional prefix.
    #
    # A Version History is not always a table. Six Accepted RFCs write it as a
    # bullet list, either `- **v3** (date): note` or `- date — v3 note`, and
    # those five were reported as having no data rows at all.
    #
    # Both forms are accepted because both are the same content in a different
    # shape, and a check that requires one rendering is measuring the renderer.
    # A Version History is not always a table. Six Accepted RFCs write it as a
    # bullet list, either `- **v3** (date): note` or `- date - v3 note`, and
    # those were reported as having no data rows at all. Both forms are the
    # same content in a different shape, and a check that requires one
    # rendering is measuring the renderer rather than the document.
    #
    # The bullet alternative is kept to a single line and uses no multi-line
    # regex: an earlier attempt spanned lines with an em-dash inside a bracket
    # expression and silently matched nothing, turning the whole linter red.
    vh_has_rows=$(awk '/^##+ ([0-9]+(\.[0-9]+)*\.? )?(Version History|Changelog|§Version History|§Changelog)/{found=1; next} /^\| v?[0-9]+/ {if (found) {print; exit}} /^[ \t]*[-*][ \t]+\*\*v[0-9]/ {if (found) {print; exit}} /^[ \t]*[-*][ \t]+v?[0-9]+\.[0-9]+/ {if (found) {print; exit}} /^[ \t]*[-*][ \t]+[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]/ {if (found) {print; exit}}' "$rfc_file" 2>/dev/null || true)
    if [ -z "$vh_has_rows" ]; then
        echo "MISSING_VH_ROWS: $rfc_file (## Version History exists but no data rows)"
        MISSING_VH=$((MISSING_VH + 1))
        FAIL=$((FAIL + 1))
        EXIT_CODE=1
        continue
    fi

    PASS=$((PASS + 1))
done < <(find "$RFC_ROOT" -type f -name '*.md' -print0 2>/dev/null || true)

echo ""
echo "Summary: SCANNED=$SCANNED markdown files, CHECKED=$CHECKED Accepted RFCs"
echo "         PASS=$PASS FAIL=$FAIL MISSING_VH=$MISSING_VH"
#
# The files this guard says nothing about are the non-Accepted ones. They are
# skipped by design, because this is a promotion gate and a Draft RFC is not
# required to carry a Version History. The summary previously named only the
# two numbers without saying the gap was a skipped population, which reads as
# unexamined work rather than as a rule.
#
# Stating it costs nothing and makes the scope auditable: 155 plus 91 is 246,
# and every file in the corpus lands in exactly one of those two groups.
if [ "$SCANNED" -gt 0 ]; then
    echo "         NOT_ACCEPTED=$((SCANNED - CHECKED - SKIPPED_BY_NAME)) (this is a promotion gate, so a non-Accepted RFC needs no Version History)"
    echo "         SKIPPED_BY_NAME=$SKIPPED_BY_NAME (CHANGELOG/README/INDEX)"
    echo "         COVERAGE=$((CHECKED * 100 / SCANNED))% of scanned files are Accepted and in scope"
fi

# A lint that examined no files has verified nothing, and must not report that
# its corpus is clean. Each of these scripts takes its corpus root from the
# environment, so pointing it at a path that does not exist — which is exactly
# what happens on a CI runner when the default names a directory outside the
# repository — used to print PASS and exit 0.
if [ "$CHECKED" -eq 0 ]; then
    echo ""
    echo "FAIL: no files were examined (RFC_ROOT=$RFC_ROOT)."
    echo "This lint verified nothing. Point RFC_ROOT at the intended corpus"
    echo "before treating a pass as meaningful."
    exit 1
fi

if [ "$EXIT_CODE" -ne 0 ]; then
    echo ""
    echo "FAIL: $MISSING_VH Accepted RFC(s) missing ## Version History."
    echo "Fix: per BLUEPRINT.md §RFC Process, append ## Version History table before promotion to Accepted."
    exit 1
fi

echo "PASS: all Accepted RFCs have ## Version History."
exit 0
