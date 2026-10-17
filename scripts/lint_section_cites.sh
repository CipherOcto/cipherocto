#!/usr/bin/env bash
# scripts/lint_section_cites.sh
# Guard 1: §section_name hygiene linter (per long-horizon plan v1.3 Phase 2 §Guard 1; R37 P3)

set -euo pipefail

RFC_ROOT="${RFC_ROOT:-rfcs}"
EXIT_CODE=0
ACCEPTED=0
REJECTED=0
NON_CANONICAL=0
SCANNED=0
EXAMINED=0

echo "§section_name hygiene linter — Guard 1 (R37 P3)"
echo "RFC root: $RFC_ROOT"
echo ""

# Every subsection number that is actually a heading in this corpus, e.g. the
# `5a` in `### 5a. PlatformLoss Envelope`.
#
# Built once, before the scan, because Reject 3 needs to answer a question
# about the corpus rather than about the shape of a token.
SUBSECTIONS=""
if [ -d "$RFC_ROOT" ]; then
    SUBSECTIONS=$(grep -rhoE '^#+ [0-9]+[a-zA-Z][ .]' "$RFC_ROOT" 2>/dev/null \
        | sed -E 's/^#+ ([0-9]+[a-zA-Z]).*/\1/' | sort -u || true)
fi
SUBSECTION_COUNT=$(grep -c . <<< "$SUBSECTIONS" || true)
echo "subsection headings indexed: $SUBSECTION_COUNT"
echo ""

while IFS= read -r -d '' rfc_file; do
    SCANNED=$((SCANNED + 1))
    # The cite NAME is alphanumeric, with `.`, `_` and `/` as INTERNAL
    # separators. It never contains a bracket, a comma, a semicolon, a colon,
    # or a trailing sentence mark.
    #
    # The extraction used `§[A-Za-z0-9._/(),;:]+`, which absorbs sentence
    # punctuation into the token. So `(per RFC-0957 §caveats)` extracted as
    # `§caveats)` — the cite plus the sentence's own closing paren. Every
    # downstream rule then examined a token that was not a citation.
    #
    # `[A-Za-z0-9]+(?:[._/][A-Za-z0-9]+)*` is the same shape stated positively:
    # a name is alphanumeric, and every separator must have alphanumeric
    # characters on BOTH sides. That keeps `§2.1`, `§7a`, `§S5.1` and
    # `§DOT_PRODUCT` intact while dropping the sentence punctuation in
    # `§6.6.`, `§2.3/` and `§B.)`.
    #
    # Requiring characters on both sides is what distinguishes the two roles a
    # separator plays. A trailing `.` or `/` is sentence punctuation; an
    # internal one is part of the name. A single character class cannot tell
    # them apart, because both are the same character.
    #
    # Measured over the corpus: 10276 cite tokens before and after, so nothing
    # was lost — only re-tokenised. 1431 of them differed from the greedy form
    # by trailing punctuation alone, and all 1431 were prose: the closing
    # paren in `(per RFC-0104 §3.2)`, the colon in `§9c):`, the period in
    # `§Algorithms.`
    #
    # One rendering was invisible to every one of those patterns: the QUOTED
    # name, `§"Roles and Authorities"`. Measured over the corpus, 264 cited
    # sections are written that way and **none of them were extracted**. A
    # fixture containing only two quoted citations reported `EXAMINED=0` and
    # printed "none contained a §cite token" — a clean result produced by
    # examining nothing, which is the failure this whole suite exists to catch.
    #
    # The alternative arm above reads a quoted name to its closing quote. It is
    # matched second so a bare name still wins when both could start at the same
    # position, and a quoted name is kept whole rather than truncated at its
    # first space.
    # A name may also START with `/`, because this corpus cites API paths as
    # sections — `§/v1/realtime` in RFC-0846 is a real endpoint, not a
    # punctuation slip. Requiring an alphanumeric first character dropped it,
    # and the only evidence was a single-token drop in the examined count
    # (10276 to 10275), which is exactly the kind of quiet undercount a total
    # is supposed to expose.
    matches=$(grep -nEo '§/?[A-Za-z0-9]+([._/][A-Za-z0-9]+)*|§"[^"]+"' "$rfc_file" 2>/dev/null || true)
    [ -z "$matches" ] && continue

    while IFS= read -r match; do
        [ -z "$match" ] && continue
        EXAMINED=$((EXAMINED + 1))
        line_num="${match%%:*}"
        cite="${match#*:}"

        # Strip prefix from line_num
        line_num="${line_num##*:}"

        # Reject 1: trailing-punct — now an invariant check, not a rejection rule.
        #
        # This is the second dead-rule episode for the same check, and the two
        # had different causes.
        #
        # FIRST, the pattern was `[.,/();:\]]`, and `grep -E` reads `\]]` as an
        # escaped closing bracket, so the character class never closes and the
        # pattern matches NOTHING. Verified by bisection:
        #
        #     [.]                matches .
        #     [.,]               matches .
        #     [.;]               matches .
        #     [(;)]              matches nothing
        #     [.,/();:]          matches .
        #     [.,/();:\]]        matches nothing
        #
        # So the rule could not fire. The corpus reported zero trailing-punct
        # findings, which is what a clean corpus looks like — the two are
        # indistinguishable from the output alone.
        #
        # SECOND, repairing the class to `[]().,;:/]` made it fire, and it
        # reported 1431 violations. Every one was prose punctuation absorbed by
        # a greedy extraction, not a malformed citation: the closing paren in
        # `(per RFC-0104 §3.2)`, the period in `§Algorithms.`, the colon in
        # `§9c):`. The extraction, not the rule, was the defect. Fixing the
        # extraction to `§/?[A-Za-z0-9]+([._/][A-Za-z0-9]+)*` removed all 1431.
        #
        # That pattern cannot produce a token ending in punctuation: every
        # separator must have alphanumerics on both sides, and the first
        # character is alphanumeric or `/`. So the check is now kept as an
        # ASSERTION that the extractor still holds its shape — if the pattern
        # is ever loosened back to a greedy class, this fires immediately and
        # says so, instead of the extractor quietly absorbing prose again.
        #
        # A rejection rule that cannot reject is worse than no rule, because it
        # reads as coverage. An assertion that fails loudly when its invariant
        # breaks is the honest form of the same check.
        if grep -qE '[]().,;:/]$' <<< "$cite" 2>/dev/null; then
            echo "REJECT [trailing-punct]: $rfc_file:$line_num: $cite"
            echo "  (the extractor should have stripped this punctuation; the"
            echo "   cite pattern was loosened and is absorbing prose again)"
            REJECTED=$((REJECTED + 1))
            EXIT_CODE=1
            continue
        fi

        # Reject 2: over-deep numeric anchors (>5 levels)
        if [[ "$cite" =~ ^§[0-9]+(\.[0-9]+){6,} ]]; then
            echo "REJECT [over-deep]: $rfc_file:$line_num: $cite"
            REJECTED=$((REJECTED + 1))
            EXIT_CODE=1
            continue
        fi

        # Reject 3: numeric-then-alpha token with no matching subsection
        # heading (e.g., §1a, §1.2x).
        #
        # This used to reject the *shape* outright, on the assumption that a
        # digit followed by a letter is a review-finding id. In this corpus
        # that is backwards. Sections are written `5a`, `9b`, `9c`, and 7 of
        # the 9 distinct tokens used carry a matching heading; review findings
        # are written letter-first (`A7`, `F4`, `R15`), and all 344 of those
        # passed untouched. The rule was therefore rejecting exactly the class
        # it was written to catch and passing the class it was written to
        # reject.
        #
        # It now asks the question it meant to ask: does a heading by this name
        # exist. A typo like `§1z` still fails, which is what the shape test was
        # for.
        if [[ "$cite" =~ ^§([0-9]+[a-zA-Z]) ]]; then
            subsection="${BASH_REMATCH[1]}"
            if ! grep -qxF "$subsection" <<< "$SUBSECTIONS" 2>/dev/null; then
                echo "REJECT [phantom-token]: $rfc_file:$line_num: $cite"
                REJECTED=$((REJECTED + 1))
                EXIT_CODE=1
                continue
            fi
        fi

        # Non-canonical: alphabetic §section_name
        if [[ "$cite" =~ ^§[^0-9] ]]; then
            NON_CANONICAL=$((NON_CANONICAL + 1))
            continue
        fi

        # Accept: pure numeric-anchor
        ACCEPTED=$((ACCEPTED + 1))
    done <<< "$matches"
done < <(find "$RFC_ROOT" -type f -name '*.md' -print0 2>/dev/null || true)

echo ""
# How much of the corpus exists, against how much was examined. Reported
# because "EXAMINED=10281 cite tokens" is indistinguishable from "that is all
# of them" unless the total is stated, and it is not all of them: this linter
# reads RFC_ROOT, which defaults to rfcs/, while roughly half the tracked
# section references in the repository live in docs/ and missions/.
#
# The count is over git-tracked markdown and takes about 30ms, so it is not
# worth omitting. Widening RFC_ROOT is a maintainer decision because it changes
# what this linter rejects; making the blind spot visible is not.
# The denominator is the REPOSITORY's total, so a run against a fixture corpus
# reports COVERAGE=0% no matter how many citations the fixture holds: the
# numerator is drawn from RFC_ROOT and the denominator from `git ls-files`.
#
# That is harmless on the real tree, where the two are the same corpus, and it
# is misleading on every other run. A fixture whose entire content was examined
# reports 0% coverage, which reads as "nothing was examined" — the exact
# confusion this figure exists to prevent.
#
# So the figure is suppressed when the corpus root is not the repository, and
# the summary says why. On the real tree it is printed exactly as before.
#
# The test is a comparison of resolved physical paths, not a list of accepted
# spellings. A spelling list is open-ended, and a spelling that is missing from
# it silently produces the wrong answer rather than an error — the first
# version of this gate matched `rfcs`, `./rfcs` and `$REPO_ROOT/rfcs`, where
# `REPO_ROOT` is never defined in this script, so on any corpus that did not
# match the first pattern `set -u` aborted the run before the summary printed.
# It passed on the real tree because the real tree matched the first pattern.
# Only a run against a fixture ever reached the second pattern, and the harness
# is what supplies those runs — so the gate stayed unexercised until the harness
# was pointed at a fixture, at which point three section_cites cases failed
# because the guard aborted before printing a summary at all.
#
# Both paths below are CWD-relative, matching the `git ls-files` denominator,
# so the comparison and the figure cannot disagree about which tree is meant.
# An unresolvable RFC_ROOT fails closed to "not the repository".
_corpus_phys=""
if [ -d "$RFC_ROOT" ]; then
    _corpus_phys=$(cd "$RFC_ROOT" && pwd -P) || _corpus_phys=""
fi
_repo_rfcs=""
if [ -d "rfcs" ]; then
    _repo_rfcs=$(cd "rfcs" && pwd -P) || _repo_rfcs=""
fi
CORPUS_IS_REPO=0
if [ -n "$_corpus_phys" ] && [ "$_corpus_phys" = "$_repo_rfcs" ]; then
    CORPUS_IS_REPO=1
fi

# Not computed at all when the figure cannot be printed: an unused denominator
# over the whole tree is ~30ms spent on a number that would be discarded.
CORPUS_TOTAL=0
if [ "$CORPUS_IS_REPO" -eq 1 ]; then
    CORPUS_TOTAL=$(git ls-files '*.md' 2>/dev/null | grep -v '^\.claude/worktrees/' | xargs grep -o '§' 2>/dev/null | wc -l | tr -d ' ')
    if [ -z "$CORPUS_TOTAL" ] || [ "$CORPUS_TOTAL" -eq 0 ] 2>/dev/null; then
        CORPUS_TOTAL=0
    fi
fi

echo "Summary: SCANNED=$SCANNED files, EXAMINED=$EXAMINED cite tokens"
echo "         ACCEPTED=$ACCEPTED NON_CANONICAL=$NON_CANONICAL REJECTED=$REJECTED"
if [ "$CORPUS_IS_REPO" -eq 0 ]; then
    echo "         COVERAGE not reported: RFC_ROOT=$RFC_ROOT is not this repository's rfcs/"
elif [ "$CORPUS_TOTAL" -gt 0 ]; then
    CORPUS_PCT=$((EXAMINED * 100 / CORPUS_TOTAL))
    if [ "$EXAMINED" -lt "$CORPUS_TOTAL" ]; then
        echo "         COVERAGE=$CORPUS_PCT% of $CORPUS_TOTAL tracked section references"
        echo "         unexamined: $((CORPUS_TOTAL - EXAMINED)) outside RFC_ROOT=$RFC_ROOT"
    else
        echo "         COVERAGE=100% of $CORPUS_TOTAL tracked section references"
    fi
fi

# A lint that examined no files has verified nothing, and must not report that
# its corpus is clean. Each of these scripts takes its corpus root from the
# environment, so pointing it at a path that does not exist — which is exactly
# what happens on a CI runner when the default names a directory outside the
# repository — used to print PASS and exit 0.
#
# This guard tested ACCEPTED, which was wrong in both directions:
#
#   - a corpus whose cites are all alphabetic (§Identity) is fully examined and
#     still reports "no files were examined", because non-canonical cites never
#     reach the ACCEPTED counter. Confirmed: two cites examined, ACCEPTED=0,
#     and the lint claimed it had looked at nothing.
#   - ACCEPTED is the *result* of the classification, not evidence of work.
#
# EXAMINED counts cite tokens the classifier actually looked at, which is the
# population this guard means.
if [ "$EXAMINED" -eq 0 ] && [ "$SCANNED" -eq 0 ]; then
    echo ""
    echo "FAIL: no files were examined (RFC_ROOT=$RFC_ROOT)."
    echo "This lint verified nothing. Point RFC_ROOT at the intended corpus"
    echo "before treating a pass as meaningful."
    exit 1
fi

if [ "$SCANNED" -eq 0 ]; then
    echo ""
    echo "FAIL: RFC_ROOT=$RFC_ROOT contains no files."
    exit 1
fi

if [ "$EXAMINED" -eq 0 ]; then
    echo ""
    echo "NOTE: $SCANNED file(s) scanned, none contained a §cite token."
    echo "This is a real result, not an empty run."
fi

if [ "$EXIT_CODE" -ne 0 ]; then
    echo ""
    echo "FAIL: $REJECTED cite hygiene violation(s) found."
    exit 1
fi

echo "PASS: 0 cite hygiene violations."
exit 0
