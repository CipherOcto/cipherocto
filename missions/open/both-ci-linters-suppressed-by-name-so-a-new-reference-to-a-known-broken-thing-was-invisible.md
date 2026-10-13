---
name: both-ci-linters-suppressed-by-name-so-a-new-reference-to-a-known-broken-thing-was-invisible
description: "lint-rfc-anchors.py and lint-rfc-type-refs.py are both wired into .github/workflows/lint.yml, and both keyed their baselines on the NAME of a finding rather than the name and place of it. A new document citing something the linter already knew was broken therefore produced exit 0 and no mention of the file: the newest violation of the exact class each lint exists to catch was the one thing neither reported. Verified by planting, then fixed. The anchor lint separately could not see a numeric anchor at the end of a sentence."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `both-ci-linters-suppressed-by-name-so-a-new-reference-to-a-known-broken-thing-was-invisible`

## Status

**Resolved** in `8cf9e7fd`. Kept as the record of what was found and what
proved it, because "we fixed it" is not the useful part — the useful part is
that a plausible-looking calibration said there was no bug.

## The finding

Both linters run in CI (`.github/workflows/lint.yml`). Both suppress known
findings through a baseline file. Both baselined by **name**:

```python
new_names = sorted(set(by_name) - baseline)
```

So a **new place** citing something the linter already knew was broken produced
nothing at all. The name was in the baseline, and the baseline was never asked
about lines.

That is the newest violation of the exact class each lint exists to catch, and
it is the one thing either lint was guaranteed to miss.

### Proven, not argued

**Type references.** A new RFC section citing `ActionPayload` — a type defined
nowhere in the corpus, already in the 576-name baseline:

|        | exit | file mentioned |
| ------ | ---- | -------------- |
| before | 0    | no             |
| after  | 1    | yes            |

**Anchors.** Same shape. A section citing an already-baselined dangling anchor
gave `exit 0`, no mention of the file, before the change.

### The second defect, found by accident while calibrating

`NUMBERED_ANCHOR = §(\d+(?:\.\d+)*)(?![\w.])`

The lookahead excludes a period as well as a word character, so `§9.9.9.` — a
numeric anchor at the end of a sentence, which is how prose normally writes one
— matches **nothing**: the regex backtracks from `9.9.9` to `9.9` to `9` and
every candidate is rejected by the dot that follows.

Both forms were planted in the same run against the same file:

| planted                                       | exit | named the file |
| --------------------------------------------- | ---- | -------------- |
| `Settled in RFC-0909 §9.9.9 and nothing more` | 1    | yes            |
| `Settled in RFC-0909 §9.9.9.`                 | 0    | no             |

The word-character exclusion still rejects `§3.5.7a`, which is what the
lookahead was for. The change is `(?![\w])`.

## What the calibration got wrong first

**It is recorded because the wrong answer looked authoritative.**

The first calibration planted a citation and got `exit 0`, which reads as
"the linter is blind". But the planted count never moved — 599 dangling
references before and after — so the linter had not _missed_ the violation, it
had never _seen_ it. Three things were wrong with the plant in turn: it was
placed in `docs/` while the anchor lint only scans `rfcs/`; it used a
lowercase-leading anchor name while `ANCHOR` requires an uppercase first
character; and it used a one-word name while `is_section_anchor` requires two.

The same run then reported, with a table and a count, that **584 of 590
baseline entries were unreachable by the collector**. That was also wrong. The
reachability check applied the reference regex to the _normalised_ name; the
regex matches _raw_ text. Comparing the baseline against what the collector
actually produces:

```
collector findings : 590
baseline entries   : 590
baseline NOT produced by the collector (dead): 0
findings missing from the baseline:              0
```

**A mutation that does not change what the check measures proves nothing**,
and a calibration that reports a count is not thereby a measurement. Both false
answers here were produced by scripts written to verify the scripts.

## What changed

- Both baselines are now `(name, site)` pairs, tab-separated.
- Both accept a bare name from an older baseline as a name-only suppression, so
  the migration did not turn the whole corpus red in one run — and **both print
  a note on every run** until `--write-baseline` is re-run, so the migration
  state is visible rather than silently permanent.
- Baselines were regenerated: anchors 395 names → 590 references, type refs
  576 names → 729 references. Same debt, counted by place rather than by word.
- `NUMBERED_ANCHOR`'s lookahead no longer excludes a trailing period.
- One reporting bug fixed alongside: the anchor lint printed
  `baselined: N of M previously known are now fixed` using `baseline - set(by_name)`,
  which is meaningless once the baseline holds pairs and reported every entry as
  fixed — the opposite of what the line said.

## Verification

Four plants against the anchor lint after the change, all as expected:

| planted                          | exit | expected |
| -------------------------------- | ---- | -------- |
| new anchor, no period            | 1    | 1        |
| new anchor, with period          | 1    | 1        |
| baselined name at a new site     | 1    | 1        |
| an anchor that actually resolves | 0    | 0        |

The last one matters as much as the first three: a lint that fires on
everything is not the opposite of one that fires on nothing, it is the same
defect wearing a different hat.

Both linters exit 0 on the clean corpus; the guard suite is 65 green; format
and Prettier are clean.

## Why this was not found earlier

Because `cargo test`, clippy, the linters themselves and a reader all agree
with a linter that is blind. Nothing distinguishes "checked and clean" from
"never looked" from the outside — which is why the check has to be planted
against rather than read.

## Method note

Every claim above is a plant-and-observe, and both were run against the
committed baseline and the working baseline. No number here is a count of what
the code appears to say.
