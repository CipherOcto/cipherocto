---
name: rfc-citations-in-rust-source-name-sections-that-do-not-exist
description: "Systematic hand-verification of RFC section citations across the marketplace Rust crates found 17 citations naming sections that do not exist: RFC-0959 §CLI at 10 sites and RFC-0959 §Cache Classification at 7. All 17 are corrected in this commit. The section-cite linter in run-local-checks.sh only reads Markdown, which is why none of them was caught; this is the concrete evidence for that recorded gap."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.1"
  depends_on:
    - RFC-0959
    - RFC-0900
---

# Mission `rfc-citations-in-rust-source-name-sections-that-do-not-exist`

## Status

Deferred as a class. The 17 citations found are corrected in this
commit; the underlying gap — that Rust doc comments are never checked
against the RFCs they cite — is a check-script decision and is not the
auditor's to make.

**v1.1** withdraws an overstated coverage claim in the Method section,
and records a phantom citation this audit introduced in its own
missions rather than only in the source tree.

This mission supersedes nothing. It is the concrete, measured
counter-example to mission `the-section-cite-linter-never-reads-rust-source`,
which recorded the gap without evidence of what it costs.

## Method, and what it does not claim

Hand-verified, deliberately. No checker was built.

Every named `RFC-XXXX §Section` citation was extracted from the
marketplace crates and deduplicated. Each section name was then checked
against the actual headings in the cited RFC.

> **Corrected at v1.1.** The v1.0 Method section said "roughly 40
> distinct citations checked by hand; two failed". That overstated what
> was done, in the same class of error this audit exists to catch: a
> document claiming coverage that no work established. Extraction in fact
> yields on the order of 250 fragments, many of them prose rather than
> citations, and the sections actually opened and read were roughly ten,
> all in RFC-0959.

The two failures below were found by checking the highest-frequency
citations first, which is why they were found at all. **The remaining
citations are unverified.** That is stated rather than papered over: the
gap is real and known, and the honest claim is "two found in the slice
that was checked", not "two exist".

A fuller sweep is listed under what the owner needs to decide.

Building a linter for this was considered and rejected — the same
reason the §-citation work earlier in this audit was done by hand. A
checker that reports "section not found" invites exactly the response
this audit has been avoiding, which is to believe the tool rather than
read the RFC.

## Finding 1: `RFC-0959 §CLI` does not exist — 10 sites

RFC-0959 has no heading containing "CLI". The string appears in the RFC
only in prose:

```text
line 115   | `CLI Operator` | human invoking `octo-wallet ask
           |   publish/list/show/revoke`, `quota-router-cli
           |   settle/settle-replay` | ...
line 508   Setted --> Revoked: Asker explicit revoke (operator-initiated CLI)
line 615   | CLI ask list | < 100ms | ...
```

The last row of the RFC's own heading list is `§Appendices`. There is
no CLI specification section at all.

The citations appeared on both CLI surfaces and on the Ask subcommands,
reading as though RFC-0959 specified them. It does not — it names them
in a role-coverage table and leaves the specification to
§Implementation Phases.

**Correction:** all ten repointed at §Roles, the coverage table that is
the RFC's only normative statement of the CLI surface. A single
explanatory note at the canonical `octo-wallet ask` command says why,
rather than repeating the caveat at all ten sites.

Sites: `quota-router-cli` `cli.rs` and `commands.rs`, `octo-wallet`
`octo-wallet.rs` and `ask_publish_cli.rs`.

## Finding 2: `RFC-0959 §Cache Classification` does not exist — 7 sites

Also absent as a heading. The RFC's `####` list under §Algorithms is
Ask identity derivation, Ask signature, Settlement hash,
Settlement-Time Vault Row Lookup, Cross-Chain Settlement Reject, Wire
Format v2.0, Compute cost, Receipt build, Verify receipt. No cache
section.

Cache appears in the RFC as the A5 adversary row and as the
`cache_key_hash` field. The normative requirement lives in
§Data Structures:

```rust
pub cache_key_hash: Option<[u8; 32]>,   // present iff
                                         // CachedInputTokensPer1k axis active
```

That single line is the whole of the requirement the code implements.

**Correction:** repointed to two different anchors, because the seven
sites were not making the same claim.

| Site                                                                                                    | Claim                                                                  | Correct anchor           |
| ------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- | ------------------------ |
| `CacheClassification` type, `cache_key_hash` field, `CacheStrategyRequired` message, `compute_cost` doc | the field exists and is required iff a cached axis is consumed         | §Data Structures         |
| `classify.rs` cache class and cache-hit types, `eleven_step` ingress note                               | the classification _approach_ — hashing the prompt to derive the class | §Adversary Analysis (A5) |

A blanket repoint would have been wrong at four of the seven sites.

## Finding 3: this audit introduced one of its own

Checking this audit's own missions, rather than only the source tree,
turned up a phantom citation **written by this audit**:

`missions/deferred/replay-defense-is-a-no-op-in-the-default-cli-configuration.md`
cited "RFC-0959 §Replay Defense". RFC-0959 has no such heading. The
replay machinery lives in §Data Structures (`ConsumedReceiptIndex`,
`SettlementError::ReceiptReplay`) and §Algorithms (`Receipt build`,
`Verify receipt`), with §Security Considerations covering `Replay
attacks`.

Corrected in the same commit, with the mission bumped to v1.1 and the
withdrawal stated in the file rather than quietly edited.

It is recorded here rather than omitted because the audit's credibility
on this exact class of error depends on disclosing its own instances. An
audit that reports other people's phantom citations while carrying its
own is worth less than one that does not.

## Why none of this was caught

`scripts/run-local-checks.sh` runs a `section cites` guard over the
Markdown tree. It has never read a `.rs` file. Every one of these 17
citations is in a Rust doc comment or test module doc, so the guard
reads past all of them.

This is the concrete cost of the gap recorded in
`the-section-cite-linter-never-reads-rust-source`. That mission recorded
the mechanism; this one records seventeen instances.

The pattern is worth naming: a citation that is checked by nothing tends
to be copied. `§CLI` appeared ten times partly because the first four
were plausible.

## What the RFC owner needs to decide

- Should the section-cite linter read Rust doc comments as well as
  Markdown? That is a check-script change and belongs to the owner.
- If not, is there a convention for Rust comments that should be — e.g.
  a narrower marker that a guard does check?
- Should RFC-0959 gain a CLI section? Ten citations across two binaries
  and a test file assumed one existed. If the CLI surface is meant to be
  specified, it is currently only an implementation-phase checklist item.
- Are there phantom citations in the slice that was not checked? The
  Method section now says plainly that only the highest-frequency RFC-0959
  citations were opened, so the rest are unverified rather than clean.
- Are there phantom citations outside the marketplace crates? Only the
  marketplace crates were searched. The frequency of `RFC-0959 §CLI`
  across the tree suggests a wider sweep is worth doing.

## Reproduction

```bash
# the two phantom sections, as they were
grep -rn 'RFC-0959 §CLI' --include=*.rs crates/
grep -rn 'RFC-0959 §Cache' --include=*.rs crates/

# a third one, introduced by this audit and now corrected
grep -nE '^#{2,4} .*Replay [Dd]efense' \
  rfcs/accepted/economics/0959-ask-settlement-chain.md

# neither exists as a heading in the RFC
grep -nE '^#{2,4} .*CLI' rfcs/accepted/economics/0959-ask-settlement-chain.md
grep -nE '^#{2,4} .*[Cc]ache [Cc]lassification' \
  rfcs/accepted/economics/0959-ask-settlement-chain.md

# the correct anchors do exist
grep -nE '^#{2,3} .*Roles' rfcs/accepted/economics/0959-ask-settlement-chain.md
grep -nE '^#{2,3} .*Adversary Analysis' rfcs/accepted/economics/0959-ask-settlement-chain.md
grep -n 'present iff CachedInputTokensPer1k' \
  rfcs/accepted/economics/0959-ask-settlement-chain.md

# the linter that reads only Markdown
grep -n 'section.cites\|section_cites' scripts/run-local-checks.sh
```

## Cross-references

- Mission `the-section-cite-linter-never-reads-rust-source` — the gap
  this supplies evidence for
- Mission `the-settlement-audit-trail-is-never-written` — the other
  phantom section found this session (`§Event Sourcing`), corrected in
  that commit
- RFC-0959 §Roles and Authorities — the CLI surface's only normative
  statement
- RFC-0959 §Data Structures — `cache_key_hash` and its iff requirement
- RFC-0959 §Adversary Analysis — A5, cache classification by BLAKE3
- RFC-0900 §Market Operations — cited from the marketplace crates and
  verified present
