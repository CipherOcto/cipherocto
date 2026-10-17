---
name: thirty-three-mission-and-rfc-frontmatter-blocks-are-unparseable-yaml-and-nothing-in-the-repository-checks
description: "Three linters read document frontmatter for status and version fields, and all three do it with awk line matching rather than a YAML parser, so malformed frontmatter was invisible to every check in the repository. Measured with scripts/lint_frontmatter.py: 242 documents carried frontmatter and 33 of them were accepted by no YAML loader, from an unquoted value containing a colon-space, a closing fence placed after body text, and a bare at-sign in an Accepted RFC. All 33 have since been repaired and the count is 0; the figures in the population table are marked superseded where they appear. The line-oriented readers survived all three causes, so this was latent rather than active damage."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-13T00:00:00.000Z
  v: "1.1"
  depends_on:
    - RFC-0001
---

# Mission `thirty-three-mission-and-rfc-frontmatter-blocks-are-unparseable-yaml-and-nothing-in-the-repository-checks`

## Status

Repaired. All 33 documents parse and `scripts/lint_frontmatter.py` reports 0
failures. The figures recorded below describe the state when the finding was
made and are marked where they appear.

## How this was found

Not by reading frontmatter. The superseded mission
`widening-the-anchor-linter-to-docs-and-missions-needs-a-qualifier-rule-before-it-is-useful`
had to be annotated in place, and an `edit` call against its `description`
failed to match a string the reader could plainly see. The bytes at the end of
that line were inspected with `cat -A` and the line ended without a closing
quote.

An unterminated double-quoted scalar in YAML does not stop at the end of the
line. The parser keeps consuming until it finds a matching quote, which
swallows every subsequent key, so the document's metadata becomes one long
string and its `depends_on` list disappears.

## The population

> **These figures are superseded.** They describe the corpus when the finding
> was made. All 33 have been repaired; `scripts/lint_frontmatter.py` now reports
> 243 of 243 parsing and 0 failures.

`python3 scripts/lint_frontmatter.py --report`:

| Measure              | Value  |
| -------------------- | ------ |
| markdown scanned     | 1920   |
| carrying frontmatter | 242    |
| parsed               | 209    |
| **failed**           | **33** |

Causes, which need four different fixes and so are not one bucket:

| Count | Cause                                       | Fix               |
| ----- | ------------------------------------------- | ----------------- |
| 27    | unquoted value containing `: `              | quote the value   |
| 5     | closing `---` fence placed after body prose | move the fence up |
| 1     | bare `@`, a reserved indicator              | quote the value   |

The 27 share one shape, an unquoted `description` carrying a reference like
`§Mission Decomposition M10 row: extend existing ...`. In a plain YAML scalar
a colon followed by a space opens a nested mapping, so the value cannot be
read.

The 5 open a frontmatter block and close it one to nine lines too late, so the
block swallows a body paragraph beginning `**Retro-supersession ...**`. The
loader reports these as a YAML _alias_ error about the leading `**`, which
names a symptom rather than the defect. The defect is the fence, and it needs
a different fix.

The single Accepted RFC affected is `RFC-0907`, whose frontmatter is correctly
fenced but carries an unquoted author line, `- @cipherocto`.

The 5 misfenced documents also carry a `: ` in their own descriptions, so
moving the fence is necessary and not sufficient. Each still fails to parse
until its description is quoted as well.

## Why nothing caught it

Three linters read frontmatter:

| Linter                              | Field read |
| ----------------------------------- | ---------- |
| `scripts/lint_status_vh_sync.sh`    | `version:` |
| `scripts/lint_no_vh_accepted.sh`    | `status:`  |
| `scripts/lint_plan_yaml_vh_sync.sh` | `version:` |

All three extract it with `awk`, matching a line beginning with the key name.
That is sufficient for their purpose and blind to everything above: they never
construct a parser, so they cannot observe that the document is unparseable.

**Checked, and it matters: the line-oriented readers survive all three causes.**
Their patterns are anchored at the start of a line and read a single field, so
a mangled `description` is never consulted by them, and the `L###`-style key
they match is unaffected by an unterminated quote in a neighbouring key. So the
33 are latent, not active misreports. That is the honest severity, and it is
the reason this is a mission rather than an incident.

## The two repaired here

| Document                                                                                                                  | Defect                                                              |
| ------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| `missions/open/anchor-lint-coverage-gap-claude-md-pointers-found-and-fixed`                                               | unterminated quote, authored earlier in the session that found this |
| `missions/archived/superseded/widening-the-anchor-linter-to-docs-and-missions-needs-a-qualifier-rule-before-it-is-useful` | unterminated quote, present before this session                     |

The second is its own correction: that mission asserted in its frontmatter that
"the ordering recommendation stands", and the measurement filed alongside it
contradicts the ordering. Both the missing quote and the stale claim are marked
at the point where they appear rather than only in a trailing note.

## Acceptance criteria

- [x] Reproduce the figure with a committed tool rather than a scratch script.
- [x] Prove the tool detects a real defect. A first attempt did not, because the
      mutation was applied to an unquoted scalar and removed nothing; the
      mutation was then made genuinely invalid and moved the count from 33 to
      34 with exit 1.
- [x] Prove the tool does not report a green result when it examines nothing. A
      nonexistent tree exits 2 rather than 0, because four scripts in this
      repository once reported success on an empty run.
- [x] Check each cause rather than reporting one bucket. The first version of
      the classifier trusted the loader's message and recorded 3 of the
      misfenced documents as alias errors; they are fence errors. A second
      version used a proxy that over-matched, because a blank line followed by
      a top-level key is ordinary YAML. The corrected test is structural: a
      column-0 line that is neither a key nor a list item is body prose.
- [x] Repair the remaining 33. 28 values quoted, 5 fences moved up and their
      descriptions quoted, and the author line in `RFC-0907` quoted. One further
      shape turned up during the repair that the classifier had folded into the
      unquoted-value bucket: a list item whose value is a quoted scalar followed
      by trailing text, in `0855p-b-state-machine-types`. A quoted YAML scalar
      cannot be followed by anything, so the parenthetical was folded into the
      string rather than deleted. The tool now reports 0 failures over 243.
- [x] Prove no content was lost. Every non-fence line of all 33 files was
      compared against `HEAD` with quoting neutralised, and all 33 are
      character-identical. The two frontmatter guards that read these documents
      produce byte-identical output before and after, which is the check that
      matters, since they are the readers this finding is about.
- [ ] Decide whether `scripts/lint_frontmatter.py` is wired into CI. The corpus
      is clean, so this is now a gate that would land green rather than red, but
      wiring it is a change to CI and belongs to a maintainer.
- [ ] Decide whether the three readers should use a real parser regardless of
      whether the new gate is adopted. A line match is what made the class
      invisible, and the new gate catches it only while it is run by hand.

## Notes

The interesting property is not that 33 documents are malformed. It is that
every tool that reads this metadata reads it in a way that cannot notice.

That is the same shape as the findings recorded elsewhere in this session's
mission set: a check that confirms belief rather than a property, a counter
that describes a different population from the one being reported, a named
constant referenced by nothing. Here the reader is correct and the defect is
invisible to it by construction.

A reader that parses the block would have caught these the day they were
written. The reason it does not is that the reader was written for a field,
not for a document.

## Correction: this tool mislabelled five documents before it counted them

The first version of `scripts/lint_frontmatter.py` classified each failure from
the loader's message text. That reported 29 unquoted values, 3 alias errors and
1 reserved indicator.

It was wrong about the 3, and wrong by omission as well. Five documents are
misfenced, and the loader describes four of them as alias errors and one as an
unquoted value, because a misplaced fence produces whatever symptom the first
body character happens to cause. Checking the five individually against their
fence positions showed each swallows one to nine lines of prose beginning
`**Retro-supersession`.

So the cause split is 27 / 5 / 1, not 29 / 3 / 1, and the totals agreed at 33
throughout — which is the point. A total that stays correct while its
composition is wrong is the most dangerous kind of measurement error, because
every arithmetic cross-check still passes. The only thing that caught it was
printing one document per line and noticing that the error pointed at a line
of prose.

The structural test replaced the message test. A column-0 line that is neither a
key nor a list item is body, and a block containing one is misfenced. The first
structural attempt used "a blank line followed by a top-level line" instead,
which over-matched by 2 because that is ordinary YAML.
