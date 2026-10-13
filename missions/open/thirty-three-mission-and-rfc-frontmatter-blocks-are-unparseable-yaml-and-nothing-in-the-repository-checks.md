---
name: thirty-three-mission-and-rfc-frontmatter-blocks-are-unparseable-yaml-and-nothing-in-the-repository-checks
description: "Three linters read document frontmatter for status and version fields, and all three do it with awk line matching rather than a YAML parser, so malformed frontmatter is invisible to every check in the repository. Measured with scripts/lint_frontmatter.py: 242 documents carry frontmatter, 33 of which no YAML loader accepts. The causes are unquoted values containing a colon-space (29), a leading asterisk read as an alias (3), and a bare at-sign in an Accepted RFC. The line-oriented readers happen to survive all three, so this is latent rather than active damage."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-13T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `thirty-three-mission-and-rfc-frontmatter-blocks-are-unparseable-yaml-and-nothing-in-the-repository-checks`

## Status

Open. Two documents have been repaired as part of filing this; the remaining
33 are corpus work and are named in the tool's `--report` output.

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

`python3 scripts/lint_frontmatter.py --report`:

| Measure              | Value  |
| -------------------- | ------ |
| markdown scanned     | 1920   |
| carrying frontmatter | 242    |
| parsed               | 209    |
| **failed**           | **33** |

Causes, which need four different fixes and so are not one bucket:

| Count | Cause                                          | Fix             |
| ----- | ---------------------------------------------- | --------------- |
| 29    | unquoted value containing `: `                 | quote the value |
| 3     | value beginning with `*`, read as a YAML alias | quote the value |
| 1     | bare `@`, a reserved indicator                 | quote the value |

The 29 share one shape, an unquoted `description` carrying a reference like
`§Mission Decomposition M10 row: extend existing ...`. In a plain YAML scalar
a colon followed by a space opens a nested mapping, so the value cannot be
read.

The single Accepted RFC affected is `RFC-0907`, whose frontmatter carries an
unquoted author line, `- @cipherocto`.

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
- [ ] Repair the remaining 33, smallest fix first: 30 need a value quoted, 3
      need a leading `*` quoted.
- [ ] Decide whether `scripts/lint_frontmatter.py` is wired into CI. Adopting it
      as a gate is only reasonable once the corpus is clean, or the gate lands
      red on a first run and gets muted.
- [ ] If it is not wired, decide whether the three readers should use a real
      parser instead, since a line match is what makes the class invisible.

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
