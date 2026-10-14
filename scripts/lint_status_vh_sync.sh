#!/usr/bin/env bash
# scripts/lint_status_vh_sync.sh
# Guard 3: Status header / VH row sync linter (per long-horizon plan v1.3 Phase 2 §Guard 3; M37 NEW P3)
# Behavior: Parse ## Status + ## Version History per RFC.
#           Detect OUT_OF_SYNC_STATUS_OLDER / OUT_OF_SYNC_STATUS_NEWER / NO_STATUS_VERSION / NO_VH.
#           Mirror M35 corpus audit logic.
# Coverage: 193 RFCs (96 accepted + 79 draft + 10 planned + 5 archived + 3 final).

set -euo pipefail

RFC_ROOT="${RFC_ROOT:-rfcs}"
EXIT_CODE=0
CHECKED=0
SCANNED=0
IN_SYNC=0
OUT_OF_SYNC_OLDER=0
OUT_OF_SYNC_NEWER=0
# Attributes of compared files whose Status version was recovered by guessing —
# a Status line naming several versions, or the frontmatter `version:` fallback.
# Not part of the partition below; bounded by CHECKED.
STATUS_MULTI_TOKEN=0
STATUS_FROM_FRONTMATTER=0
NO_STATUS_VERSION=0
NO_VH=0
NO_VH_VERSION=0
NOT_RFC=0
CLASSIFIED=0
UNCLASSIFIED=0

# The Version History heading is written in four shapes across this corpus:
# bare, numbered (`## 6. Version History`), and each of those with a leading
# section sign. `lint_no_vh_accepted.sh` was taught all four; this script was
# not, and the gap was worse here than in the row patterns.
#
# `vh_line_num` matched only the bare form, so a numbered or signed section
# left it empty and the per-file loop hit its `continue` before reaching any
# comparison. RFC-0206 §8 Version History was never examined by this guard at
# all — it reported nothing for the file because it did not read it, which is
# the same defect as a lint that reports success on an empty corpus, and this
# script already carries a guard for that at the end of the file.
# The section sign may come before the number as well as after it. The previous
# alternation listed `§Version History` beside `6. Version History` and so
# recognized three of the four shapes its own comment above names: it had no arm
# for `§6 Version History`, where the sign leads. Two files in this corpus write
# that form, and neither had its Version History read by this guard at all —
# they fell past the NO_VH branch, which counts only Accepted files, and reached
# a `continue` having incremented nothing. The population reconciliation added
# below is what surfaced them, as six files this guard read and could not place.
#
# Both signs are now optional and independent, so every shape the comment names
# is matched, and no shape that matched before stops matching.
VH_HEADING='^##+ (§)?([0-9]+(\.[0-9]+)*\.? )?(§)?(Version History|Changelog)'

# Normalise a version to three numeric components so that `2.14` and `2.14.0`
# compare equal. Missing components are zero, which is what they mean in every
# version scheme this corpus uses.
pad3() {
    local v="${1#v}" i comp
    local IFS=.
    local -a parts=()
    read -ra parts <<< "$v"
    for ((i = 0; i < 3; i++)); do
        # A trailing `-rN`, `-rc1`, `-draft` is a review-round or draft marker
        # on the same version, not a different one: this corpus writes
        # `v3.7-r3a` in a history table against a `v3.7` Status. Comparing the
        # marker reported six Accepted RFCs as out of sync with themselves.
        #
        # Read through `${parts[$i]:-}` rather than `${parts[$i]%%...}`. Under
        # `set -u` the suffix-strip form aborts on an unset element, and this
        # script runs `set -euo pipefail`. It aborted on exactly the two-
        # component versions the function exists to handle — `2.14` — so
        # RFC-0110 stayed out of sync after the fix that was supposed to clear
        # it, and the abort printed to stderr where the report did not show it.
        comp="${parts[$i]:-}"
        comp="${comp%%[-+]*}"
        parts[$i]="${comp:-0}"
    done
    printf '%s' "${parts[0]}.${parts[1]}.${parts[2]}"
}
DUAL_VH=0

echo "Status/VH sync linter — Guard 3 (M37 NEW P3)"
echo "RFC root: $RFC_ROOT"
echo ""

# Every file this loop reads must land in a category, and the categories must
# add back up to the population.
#
# They did not. Three paths placed a scanned file in no counter at all, so the
# summary described a corpus it had only partly accounted for. On this tree
# SCANNED=247 while CHECKED=74, NO_STATUS_VERSION=160 and NO_VH_VERSION=12
# — 246, and not even that, because six files are counted in two categories at
# once. The residual was seven files the guard read and reported nothing about,
# which is the same shape as the defects this script already carries comments
# about: a number beside a population it does not describe.
#
# The silent paths were a basename skip that incremented SCANNED and continued,
# and a file holding a Status version but no Version History heading that is not
# Accepted — which falls past the NO_VH branch, because that branch only counts
# Accepted files, and reaches its `continue` having incremented nothing.
#
# `tally` closes both. A file that set `touched` reached at least one category
# and is counted once; one that did not is counted as UNCLASSIFIED, which is a
# finding about this guard rather than about the corpus.
tally() {
    if [ "$touched" -eq 1 ]; then
        CLASSIFIED=$((CLASSIFIED + 1))
    else
        # Named, like every other category. A count of files this guard read
        # and could not place, with nothing to identify them, is the same
        # defect one level up. The reason is stated too, where the path that
        # reached here knew it — "read, but no category applies" describes
        # this guard's state of mind, not the document's.
        echo "UNCLASSIFIED: $rfc_file (${unclassified_why:-read, but no category applies})"
        UNCLASSIFIED=$((UNCLASSIFIED + 1))
    fi
}

while IFS= read -r -d '' rfc_file; do
    SCANNED=$((SCANNED + 1))
    touched=0
    unclassified_why=""

    # Skip non-content files (CHANGELOG-only, README, etc.)
    basename=$(basename "$rfc_file")
    case "$basename" in
        CHANGELOG.md|README.md|INDEX.md)
            NOT_RFC=$((NOT_RFC + 1))
            continue
            ;;
    esac

    # Find ## Status block
    #
    # The VERSION lives in the lines after the heading, never on it. This used
    # to read only the heading line:
    #
    #     status_line=$(grep -m1 -E '^## (Status|## Status|Status:)' ...)
    #     status_version=$(grep -oE 'v[0-9]+(\.[0-9]+)*' <<< "$status_line")
    #
    # Measured over the whole corpus: the matched heading line carries a version
    # 0 times in 246 files. This corpus writes the status on the line(s) after
    # the heading — `## Status` / blank / `Accepted (v1.1.0) — promoted ...` — so
    # the extraction ran against a string that never contained a version.
    #
    # The only reason the guard reported anything at all is the frontmatter
    # fallback below, which reaches the 25 Accepted RFCs carrying a YAML
    # `version:`. Everything else fell through to NO_STATUS_VERSION: 210 of 247
    # files were reported as having no Status version, including files whose
    # Status block plainly reads `Final (v35 - Stoolap compatibility)`.
    #
    # So this guard compared versions on roughly a seventh of the corpus and
    # called the rest "no status version", which is the same defect as a lint
    # that reports success on an empty corpus, wearing a different hat. Its
    # sibling `lint_no_vh_accepted.sh` was already taught to read the Status
    # *block*; this one was not, so two guards disagreed about the same
    # documents.
    #
    # Read the whole Status block, from the heading to the next heading of any
    # level, and take the version from there.
    #
    # The block is filtered to the STATUS line itself before the version is
    # read. `grep -A5` returns prose too, and prose in this corpus is full of
    # version-shaped tokens that are not the document's version: RFC-0959 writes
    #
    #     ## Status
    #
    #     Accepted
    #
    #     > **Note (v2.0, 2026-08-19):** Wire-format amendment adds ...
    #
    # so an unfiltered read reports Status=v2.0 for a document whose Status line
    # says only "Accepted", and then manufactures an out-of-sync finding
    # against the Version History. The `v2.0` there is the wire-format
    # amendment's version, not the RFC's.
    #
    # The Status line is the first line of the block that is not blank, not a
    # heading (the matched `## Status` itself, and the next section heading that
    # -A5 can reach), not a blockquote (`>` Note / Amendment-chain asides), and
    # not a table row. Drop all of those, then take what remains.
    #
    # The heading line is the trap here. It matches `-A5` as the first output
    # line, and a filter that drops blanks, `>` asides and `|` rows leaves
    # `## Status` at the head of the list. `head -1` then returns the heading —
    # a line that by construction never carries a version — and the extraction
    # reports "no Status version" for every file, reproducing the original
    # defect behind a filter that looks like it works. Any filter on this block
    # must drop headings too.
    status_line=$(grep -m1 -A5 -E '^## (Status|## Status|Status:)' "$rfc_file" 2>/dev/null \
        | grep -vE '^[[:space:]]*$' \
        | grep -vE '^[[:space:]]*[>|]' \
        | grep -vE '^#+ ' \
        | head -1 || true)

    # Find ## Version History block (or equivalent)
    vh_line_num=$(grep -m1 -nE "$VH_HEADING" "$rfc_file" 2>/dev/null | head -1 | cut -d: -f1 || true)

    # Extract Status version (e.g., "Draft v3.1", "Accepted (2026-08-20)", etc.)
    #
    # A `v`-token that belongs to an RFC NUMBER is not this document's version.
    # The extraction took the first `v[0-9]` on the line, and these Status lines
    # open by summarising which other RFCs they amend:
    #
    #     **Accepted (2026-09-12)** — codifies the ... of RFC-0012-v2 + RFC-0014-v3
    #
    # so it read `v2` out of the cross-reference `RFC-0012-v2` and reported
    # RFC-0012 and RFC-0014 as having Status=v2 against a Version History whose
    # latest row is 3.0.0. Both files were reported out of sync because of a
    # version belonging to a different document. Two of the eleven findings on
    # the tree were this.
    #
    # Excluding the token when it is immediately preceded by an RFC number keeps
    # the real cases: `Accepted (v2.0 — ...)` and `spec elaboration closed
    # (v1.3)` are both bare tokens and both still match.
    status_version=""
    status_tokens=""
    status_token_count=0
    status_source="status-line"
    if [ -n "$status_line" ]; then
        # Drop tokens that are the version half of an `RFC-NNNN-vX` reference.
        #
        # Every remaining token is emitted, not just the first, and the first is
        # still the one compared. Which token is "this document's version" is
        # not decidable from the line alone: these Status blocks are prose, and
        # 18 of them name more than one version. RFC-0862 narrates its own
        # history in the Status line — "Updated v1.1.0 (2026-06-21): ... Updated
        # v1.2.0 (2026-06-25): ..." — so the first token is the older of the two
        # and the guard compared v1.1.0 against a history whose latest row is
        # 1.2.0, reporting drift in a file that is in sync.
        #
        # Taking the last token instead is not the fix, and the reason is
        # measurable rather than stylistic: mutated to compare the last token,
        # this guard repairs RFC-0862 and breaks nine others, taking the drift
        # count on this tree from 9 to 17. The lines disagree about their own
        # direction — RFC-0963 descends (`v3.1`, then `v2.0`, then `v3.1`) while
        # RFC-0206-v40 opens with the oldest version it mentions (`v0.1.0`,
        # `v1.0`, `v1.0`) — so no ordering rule holds across both shapes, and a
        # second guess would have traded one class of wrong verdict for another.
        # The count is reported on the finding instead: the guard shows which
        # token it compared and what else the line said, rather than presenting
        # one pick as the document's version.
        status_scan=$(python3 - "$status_line" <<'PY' 2>/dev/null || true
import re, sys
line = sys.argv[1]
toks = []
for m in re.finditer(r'v[0-9]+(?:\.[0-9]+)*', line):
    pre = line[:m.start()]
    if re.search(r'RFC-[0-9]+(-[a-z0-9]+)?-$', pre, re.IGNORECASE):
        continue  # version of a different RFC, cited on this line
    toks.append(m.group(0)[1:])  # strip the leading `v`; the report line re-adds it
# Count and list on separate single lines. Emitting one token per line made a
# finding that names five versions wrap across five lines of the report, so the
# `OUT_OF_SYNC_` line and the line after it no longer belonged together.
print(len(toks))
print(", ".join(toks))
PY
)
        status_token_count=$(head -1 <<< "$status_scan" 2>/dev/null || true)
        [ -n "$status_token_count" ] 2>/dev/null || status_token_count=0
        status_tokens=$(sed -n '2p' <<< "$status_scan" 2>/dev/null || true)
        status_version=$(cut -d, -f1 <<< "$status_tokens" 2>/dev/null | tr -d ' ' || true)
    fi

    # If no ## Status block, try inline Status field in YAML frontmatter
    if [ -z "$status_version" ]; then
        yaml_status_version=$(awk '/^---$/{c++; next} c==1 && /^version:/ {print $2; exit}' "$rfc_file" 2>/dev/null || true)
        if [ -n "$yaml_status_version" ]; then
            status_version="${yaml_status_version#v}"
            status_source="frontmatter"
        fi
    fi

    # If still no Status version, this is NO_STATUS_VERSION
    if [ -z "$status_version" ]; then
        echo "NO_STATUS_VERSION: $rfc_file (no ## Status version found)"
        NO_STATUS_VERSION=$((NO_STATUS_VERSION + 1))
        touched=1
        # Continue to check VH even if no Status
    fi

    # Check for VH block
    if [ -z "$vh_line_num" ]; then
        # Special case: Accepted RFCs without VH = NO_VH
        #
        # This test was `grep -qE '^## (Status|## Status|Status:).*Accepted'`,
        # which requires the word "Accepted" on the SAME line as the heading.
        # This corpus never writes it there — the Status block puts it on the
        # next line — so the branch could not fire on any file, and NO_VH was
        # structurally 0 rather than measured 0. A guard that cannot reach its
        # own counter is not a guard; it is a comment that ends in `fi`.
        #
        # The sibling guard `lint_no_vh_accepted.sh` has always read the Status
        # block and has reported 6 Accepted RFCs missing a Version History.
        # This one reported none. Same corpus, same rule, opposite answers.
        #
        # Read the block. The `status_line` computed above is already that
        # block's first real line, so reuse it rather than re-grepping.
        if grep -A5 -E '^## Status' "$rfc_file" 2>/dev/null | grep -q 'Accepted' \
            || awk '/^---$/{c++; next} c==1 && /^status:/ {print; exit}' "$rfc_file" 2>/dev/null | grep -qi accepted; then
            echo "NO_VH: $rfc_file (Accepted but no ## Version History)"
            NO_VH=$((NO_VH + 1))
            EXIT_CODE=1
        else
            # Reaching `tally` untouched from here means all three of these are
            # known at once: a Status version was found, the Version History
            # heading is absent, and the document is not Accepted. So this is
            # not a governance failure — the sibling guard requires a Version
            # History of Accepted RFCs only, and these are Draft or Archived.
            #
            # It counted as UNCLASSIFIED reading "read, but no category
            # applies", which is the guard reporting its own confusion instead
            # of the document's state, and left a maintainer with four files
            # and no way to tell whether they were findings or noise.
            unclassified_why="has a Status version but no ## Version History, and is not Accepted, so no history is required of it"
        fi
        tally
        continue
    fi

    # Count VH tables (some RFCs have multiple - dual-VH pattern like RFC-0126)
    vh_count=$(grep -cE "$VH_HEADING" "$rfc_file" 2>/dev/null || echo "0")
    if [ "$vh_count" -gt 1 ]; then
        DUAL_VH=$((DUAL_VH + 1))
    fi

    # Extract latest VH row version.
    #
    # This used `print; exit`, which stops at the FIRST numeric row in the
    # table. A Version History table is append-ordered oldest-first, so the
    # first row is the *oldest* entry, not the latest. Every RFC whose table
    # had more than one row therefore compared its Status version against its
    # initial version and was reported as out of sync: 8 of the 9 findings on
    # the current tree were this bug, on files that are correctly in sync
    # (e.g. Status v1.1.0 against a table whose first row is 1.0.0 and whose
    # newest row is 1.1.0).
    #
    # Take every versioned row and sort -V, newest last. Version sorting is
    # required, not lexicographic: 1.10.0 > 1.9.0 numerically but < it as a
    # string.
        # "Does this section have data rows" and "what version is the latest" are
    # two different questions, and this conflated them. A Version History is
    # not always a table: this corpus also writes dated bullets, which is the
    # rendering `lint_no_vh_accepted.sh` accepts in four forms. Because only
    # table rows were read here, those documents were reported as having no
    # rows at all — 27 of 141 reports on the current tree, verified by
    # checking each file for a bullet row the sibling guard recognises.
    #
    # So: recognise a row in any of the four renderings, and only then ask
    # whether it carries a version. A history that has dated rows but no
    # version column is not a missing table; it has nothing to compare a
    # Status version against, and saying so is more useful than claiming the
    # section is empty.
    # A Version History TABLE ends at its last row. `found` used to latch on at
    # the heading and never clear, so both row readers ran to end-of-file and
    # read every subsequent pipe table in the document as Version History rows.
    #
    # RFC-0011-w has a Version History of `| 1.0 |` and `| 1.1 |`, then ~900
    # lines later a CLI error-code slot table whose first column is `| 79 |`,
    # `| 80 |`, ... `| 91 |`. The linter took the highest number in that table as
    # the "latest version" and reported `OUT_OF_SYNC_STATUS_OLDER: Status=v1.1,
    # VH latest=v91` — a finding manufactured entirely by reading the wrong
    # table. Its Status and Version History agree.
    #
    # So the table is bounded: once a data row has been seen, the first line that
    # is neither blank nor a pipe row ends the section. Prose after the table is
    # still read by the bullet branches, since a bullet list is not a table and
    # does not terminate it.
    vh_has_rows=$(awk -v h="$VH_HEADING" '$0 ~ h {found=1; next}
        found && /^\| v?[0-9]+(\.[0-9]+)*(-[A-Za-z0-9.]+)?[ ]*\|/ {print; exit}
        found && /^[ \t]*[-*][ \t]+\*\*v[0-9]/ {print; exit}
        found && /^[ \t]*[-*][ \t]+v?[0-9]+\.[0-9]+/ {print; exit}
        found && /^[ \t]*[-*][ \t]+[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]/ {print; exit}' "$rfc_file" 2>/dev/null || true)

    if [ -z "$vh_has_rows" ]; then
        echo "NO_VH_ROWS: $rfc_file (## Version History exists but no data rows)"
        NO_VH=$((NO_VH + 1))
        touched=1
        tally
        continue
    fi

    # The row pattern must end the cell at the version. Without the trailing
    # `[ ]*\|`, a table whose first column holds a date rather than a version
    # matched on its year: `| 2026-07-20 | **Promoted to Accepted.** |` in
    # RFC-0853 yielded a "latest version" of 2026, and `sort -V` ranked it
    # above 1.1.0, manufacturing an out-of-sync finding against a document
    # that is perfectly in sync. Relaxing the pattern to fix 18 false
    # NO_VH_ROWS reports created this one; both are fixed by requiring the
    # number to be followed by the cell boundary rather than by anything.
    # Version extraction stays table-only, because a dated bullet carries no
    # version to sort. Where rows exist but no version does, the comparison a
    # Status version would feed into cannot be made, and that is reported
    # rather than silently treated as a clean pass.
    #
    # The row pattern stops at the version and does not require a space and a
    # pipe immediately after it. Requiring ` |` matched only tables whose first
    # column is exactly one character wider than its widest entry, so every
    # column-aligned table — `| 1.0     | 2026-04-11 | ...` — was read as
    # having no version at all. The optional `v` prefix matters for the same
    # reason `lint_no_vh_accepted.sh` allows it: 28 Accepted RFCs write
    # `| v2.12 |`.
    vh_latest=$(awk -v h="$VH_HEADING" '$0 ~ h {found=1; next}
        found && /^\|/ {if (match($0, /^\| v?[0-9]+(\.[0-9]+)*(-[A-Za-z0-9.]+)?[ ]*\|/)) print substr($0, RSTART, RLENGTH); next}
        found && NF {exit}' "$rfc_file" 2>/dev/null | sed -E 's/^\| v?//; s/[ ]*\|$//' | sort -V | tail -1 || true)

    if [ -z "$vh_latest" ]; then
        echo "NO_VH_VERSION: $rfc_file (Version History has dated rows but no version column to compare)"
        NO_VH_VERSION=$((NO_VH_VERSION + 1))
        touched=1
        tally
        continue
    fi

    # Compare Status version vs VH latest.
    #
    # Compared as numbers, not as strings. `2.14` and `2.14.0` are the same
    # version written two ways, and this corpus writes both: RFC-0110 declares
    # v2.14.0 in Status against a history whose latest row is 2.14. A string
    # comparison called that out of sync and has been reporting it ever since,
    # which is the failure mode of a check that cannot tell a difference from a
    # spelling.
    if [ -n "$status_version" ]; then
        # This is the only place a Status version and a Version History row are
        # put side by side, so this is where CHECKED increments.
        #
        # CHECKED used to increment once per markdown file found, so the summary
        # read `CHECKED=247 IN_SYNC=65` — a whole-corpus count beside a
        # compared-only count. The gap looked like unexamined work rather than a
        # silently skipped population, and nothing in the output said so.
        # Measured on the current tree: 74 files are actually compared, 12 have
        # rows but no version column, and 160 carry no Status version at all.
        # The old figure overstated coverage by more than six times.
        #
        # SCANNED now carries the corpus total, and the summary reports the
        # population statement rather than leaving the reader to subtract.
        CHECKED=$((CHECKED + 1))
        touched=1
        #
        # Two of the shapes that make CHECKED a number worth distrusting are
        # counted rather than left to be inferred from the findings. Both are
        # cases where the guard compared a version it recovered by guessing:
        # a Status line that names several versions, where the first was taken
        # on the evidence that the rest of the corpus ascends, and the
        # frontmatter `version:` fallback, which reads a field the RFC template
        # does not define (rfcs/draft/process/0000-template.md writes no
        # frontmatter at all) against a history table it does not reliably
        # track — 11 of the 36 RFCs carrying that key disagree with their own
        # latest VH row once `-r7`-style review markers are normalised away.
        #
        # They are attributes of compared files, not categories of them, so they
        # sit outside the reconciliation below and are bounded by CHECKED.
        status_provenance=""
        if [ "$status_token_count" -gt 1 ] 2>/dev/null; then
            status_provenance="; Status line names $status_token_count versions [$status_tokens], compared the first"
            STATUS_MULTI_TOKEN=$((STATUS_MULTI_TOKEN + 1))
        elif [ "$status_source" = "frontmatter" ]; then
            status_provenance="; Status version read from frontmatter version:, not the ## Status block"
            STATUS_FROM_FRONTMATTER=$((STATUS_FROM_FRONTMATTER + 1))
        fi
        if [ "$(pad3 "$status_version")" = "$(pad3 "$vh_latest")" ]; then
            IN_SYNC=$((IN_SYNC + 1))
        else
            # Determine direction: is Status older or newer than VH latest?
            status_major=$(grep -oE '^[0-9]+' <<< "$status_version" || echo "0")
            vh_major=$(grep -oE '^[0-9]+' <<< "$vh_latest" || echo "0")
            status_minor=$(grep -oE '\.[0-9]+' <<< "$status_version" | head -1 | sed 's/^\.//' || echo "0")
            vh_minor=$(grep -oE '\.[0-9]+' <<< "$vh_latest" | head -1 | sed 's/^\.//' || echo "0")

            if [ "$status_major" -lt "$vh_major" ] 2>/dev/null || \
               { [ "$status_major" -eq "$vh_major" ] && [ "$status_minor" -lt "$vh_minor" ] 2>/dev/null; }; then
                echo "OUT_OF_SYNC_STATUS_OLDER: $rfc_file (Status=v$status_version, VH latest=v$vh_latest$status_provenance)"
                OUT_OF_SYNC_OLDER=$((OUT_OF_SYNC_OLDER + 1))
                EXIT_CODE=1
            else
                echo "OUT_OF_SYNC_STATUS_NEWER: $rfc_file (Status=v$status_version, VH latest=v$vh_latest$status_provenance)"
                OUT_OF_SYNC_NEWER=$((OUT_OF_SYNC_NEWER + 1))
                EXIT_CODE=1
            fi
        fi
    fi
    tally
done < <(find "$RFC_ROOT" -type f -name '*.md' -print0 2>/dev/null || true)

echo ""
echo "Summary: SCANNED=$SCANNED files, CHECKED=$CHECKED Status/VH pairs compared"
echo "         IN_SYNC=$IN_SYNC"
echo "         OUT_OF_SYNC_STATUS_OLDER=$OUT_OF_SYNC_OLDER"
echo "         OUT_OF_SYNC_STATUS_NEWER=$OUT_OF_SYNC_NEWER"
echo "         NO_STATUS_VERSION=$NO_STATUS_VERSION (nothing to compare a VH row against)"
echo "         NO_VH=$NO_VH"
echo "         NO_VH_VERSION=$NO_VH_VERSION (dated rows, no version column; not comparable)"
echo "         DUAL_VH=$DUAL_VH (informational only, e.g., RFC-0126)"
echo "         NOT_RFC=$NOT_RFC (CHANGELOG/README/INDEX; scanned but not RFC content)"
echo "         CLASSIFIED=$CLASSIFIED UNCLASSIFIED=$UNCLASSIFIED (every scanned file, once)"
echo "         STATUS_MULTI_TOKEN=$STATUS_MULTI_TOKEN (Status line named several versions; first compared)"
echo "         STATUS_FROM_FRONTMATTER=$STATUS_FROM_FRONTMATTER (version read from frontmatter, not the Status block)"
#
# Both of the above describe files inside CHECKED, so neither may exceed it. A
# counter that can outrun the population it is a subset of is the same defect as
# the derived-remainder counter `lint_no_vh_accepted.sh` carried: it reports a
# number that cannot be true.
if [ $((STATUS_MULTI_TOKEN + STATUS_FROM_FRONTMATTER)) -gt "$CHECKED" ]; then
    echo "FAIL: guessed-version counters exceed the compared population:"
    echo "      STATUS_MULTI_TOKEN=$STATUS_MULTI_TOKEN + STATUS_FROM_FRONTMATTER=$STATUS_FROM_FRONTMATTER"
    echo "      is $((STATUS_MULTI_TOKEN + STATUS_FROM_FRONTMATTER)), but CHECKED=$CHECKED."
    EXIT_CODE=1
fi
#
# The three counters above must add back up to the population. They are counts
# of files that reached at least one category, so a file landing in two is
# still counted once and the arithmetic is exact — which is what makes a
# residual visible instead of something a reader has to notice is missing.
if [ $((NOT_RFC + CLASSIFIED + UNCLASSIFIED)) -ne "$SCANNED" ]; then
    echo "FAIL: population does not reconcile: NOT_RFC=$NOT_RFC + CLASSIFIED=$CLASSIFIED"
    echo "      + UNCLASSIFIED=$UNCLASSIFIED is $((NOT_RFC + CLASSIFIED + UNCLASSIFIED)),"
    echo "      but SCANNED=$SCANNED. A file was read and placed in no category."
    EXIT_CODE=1
fi
#
# CHECKED counts files where a Status version and a Version History row were
# actually put side by side. SCANNED counts markdown files found. The
# difference is the population this guard cannot decide anything about, and it
# is large: most RFCs in this corpus write no version in their Status block at
# all, so a summary reading only `CHECKED` would overstate coverage several
# times over.
if [ "$SCANNED" -gt 0 ] && [ "$CHECKED" -eq 0 ]; then
    echo "         COVERAGE=0% of $SCANNED scanned files were comparable"
elif [ "$SCANNED" -gt 0 ]; then
    echo "         COVERAGE=$((CHECKED * 100 / SCANNED))% of $SCANNED scanned files were comparable"
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
    echo "FAIL: Status/VH sync drift detected."
    echo "Fix: align ## Status version with latest ## Version History row."
    exit 1
fi

echo "PASS: Status/VH sync clean."
exit 0
