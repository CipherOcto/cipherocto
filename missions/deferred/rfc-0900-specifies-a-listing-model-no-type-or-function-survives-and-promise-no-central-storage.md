---
name: rfc-0900-specifies-a-listing-model-no-type-or-function-survives-and-promise-no-central-storage
description: "RFC-0900 §Specification defines QuotaListing, QuotaPurchase, QuotaRouter and RouterConfig, plus listQuota, purchaseQuota and routePrompt. None of the three types has an implementation and none of the three functions exists anywhere in the tree — but v1.1 retracts the reading of that as a defect, because §Version History v1.1 already declared the TypeScript interfaces a wire-shape reference with the Rust impl canonical. What survives is sharper: under the RFC's own precedence rule the one interface with a Rust counterpart is RouterConfig, and that counterpart belongs to RFC-0927 with zero field overlap, so RFC-0900's marketplace routing config has no canonical representation. Separately §Security & Privacy promises no central storage while §Slash Ledger Substrate specifies SQL migrations and a Stoolap fork driver. Credit where due: the prompt-privacy row is an honest, correctly-scoped TRUST ASSUMPTION, and both §References paths exist."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.1"
  depends_on:
    - RFC-0900
---

# Mission `rfc-0900-specifies-a-listing-model-no-type-or-function-survives-and-promise-no-central-storage`

## Status

Deferred. Type and function names, one security-table cell, and two reference
entries are RFC edits. Nothing was actioned.

## v1.1 — I filed v1.0 without reading §Version History, and it changed the finding

RFC-0900 v1.1 says this, at the end of its version-history row:

> The TypeScript interfaces in this RFC remain a **wire-shape reference**; the
> **Rust impl is the canonical source for the protocol**.

I swept §Specification, §Security & Privacy, §Related RFCs and §References,
verified every symbol against the tree, and did not read §Version History. The
section I was measuring had already been explicitly subordinated by its own
document, three versions ago.

**That materially weakens v1.0's headline, and it should not stand.** `QuotaListing`,
`QuotaPurchase`, `QuotaRouter`, `listQuota`, `purchaseQuota` and `routePrompt`
having no Rust implementation is **declared, not a defect**. The RFC told the
reader to expect exactly that and named where the truth lives. A finding that
presents a disclosed, intentional design as an omission is worse than no finding,
because it spends the reader's attention on a non-issue.

Two things survive, and one of them gets _worse_.

**The `RouterConfig` collision gets worse, not better.** v1.0 called it a name
collision. But the RFC's own rule is that Rust is canonical — and Rust's
`RouterConfig` belongs to RFC-0927, whose mission is archived, with zero field
overlap. So the document that promised "the Rust impl is the canonical source"
points, for one of its four interfaces, at a type implementing a different RFC's
extension. Under the RFC's own precedence rule there is **no canonical
representation of RFC-0900's marketplace routing config at all**. That is a real
gap, and the disclosure makes it sharper rather than excusing it.

**The design question is real even though the absence is declared.** The
wire-shape reference describes listings and purchases. The canonical Rust
implementation is `Ask`-shaped. The RFC designates one as truth and documents the
other as reference, and the two describe **different protocols** — there is no
mapping between a `QuotaListing` and an `Ask`, and no statement that one is the
other. That is the owner decision v1.0 already pointed at, now carrying the added
weight of the RFC having explicitly made Rust canonical in writing.

**What is retracted.** The "core model does not exist" framing, and any reading of
the absent functions as unimplemented promises. v1.0's _evidence_ is all still
true — the symbol counts are unchanged and every reproduction command still
produces what it claims. What was wrong is the interpretation placed on top of
it.

The rule this is an instance of, and the one I keep relearning: **read the whole
document before filing on any part of it.** §Version History was nine lines above
the section I swept.

## Finding

RFC-0900 §Specification is the marketplace RFC's normative core, and almost none
of it exists. What makes it worth filing separately from the recorded RFC-0900
work is that the _absence_ is not uniform: most of it is honestly absent, and one
part of it is invisibly absent behind a real type with the same name.

## §Specification: the model does not exist — declared, not defective (v1.1)

| Symbol          | Occurrences in `crates/`         |
| --------------- | -------------------------------- |
| `QuotaListing`  | 0                                |
| `QuotaPurchase` | 0                                |
| `QuotaRouter`   | 1 — and it is a comment          |
| `RouterConfig`  | a real struct, wrong one (below) |
| `listQuota`     | 0                                |
| `purchaseQuota` | 0                                |
| `routePrompt`   | 0                                |

`QuotaRouter`'s single hit is in the PyO3 bindings and reads `// QuotaRouter
exceptions for PyO3 bindings` — that is the _project_ named in a comment, not the
RFC's `QuotaRouter` interface. Counting it would have been the error this audit
keeps making, so it is worth stating that the interface itself has zero hits.

The implemented marketplace is a different model. `Marketplace` exposes `put`,
`cheapest`, `cheapest_with_ranking`, `list_by_asker`, `record_outcome`,
`attach_org_policy` and the store constructors. It is `Ask`-shaped. RFC-0900
specifies a listing-and-purchase model with a `status` of
`'active' | 'exhausted' | 'cancelled'` and three async operations. None of the six
functions or four interfaces has a counterpart.

This is consistent with the recorded finding that the listing lifecycle has no
representation anywhere. This mission adds the part that finding does not cover:
the listing lifecycle is missing, **and so are the types and the entry points that
would have carried it.**

## The dangerous case: `RouterConfig` exists, and is not this one

RFC-0900 §Specification defines:

```typescript
interface RouterConfig {
  max_price_per_prompt;
  preferred_providers;
  fallback_enabled;
  fallback_timeout_ms;
  require_minimum_balance;
  auto_recharge_enabled;
  auto_recharge_source;
}
```

A `RouterConfig` struct **does** exist in `quota-router-core/src/router.rs`, used
at forty-odd sites. Its fields are `routing_strategy`, `latency_window`,
`latency_config`, `verbose`, `weights`.

**The overlap between the two field sets is zero.** Not one of RFC-0900's seven
fields is present, including `require_minimum_balance`, which is the security
half of the marketplace routing config.

The lineage is documented in the tree: `quota-router-core/src/config.rs` heads
that block "RFC-0927 Types (RouterConfig Extension for LiteLLM Compatibility)",
and `latency_config` is annotated RFC-0925 / RFC-0926. The mission that built it,
`0927-a-router-config-extension`, is in `missions/archived/`.

So this is a **type-name collision between two RFCs**, resolved in favour of
RFC-0927, where RFC-0900 is Accepted and RFC-0927's mission is archived. The
consequence is worse than a plain absence. A reader auditing RFC-0900 greps for
`RouterConfig`, finds a real struct with forty call sites, and has every reason
to conclude the section shipped. A missing type is visible; a wrong type wearing
the right name is not.

## "Minimum listing: 10 prompts" is a table cell with no gate

§Token Economics is three rows. Two are the obvious (+1 / -1 OCTO-W). The third
is:

| Minimum listing | 10 prompts |

There is no minimum-listing gate in the tree. A search for the concept under its
plausible names returns nothing, while the control — `Ask` itself — resolves in
three crates. This is a number a seller-facing rule depends on, stated in an
Accepted RFC with nothing behind it.

## §Security & Privacy contradicts §Slash Ledger Substrate

The security table's data-residency row reads:

| Data residency | **No central storage** |

Fifteen lines below the slash-ledger subsection begins:

> RFC-0900 v2.0 ... specifies the **storage + in-memory substrate** shape that
> backs §Slashing Model.

And that subsection is a specification of central storage, in detail: a
`chain_id BLOB` column, a composite UNIQUE INDEX, a BIGINT amount column with a
`dqa_to_i64` bridge, a three-name index drop, and an `UPDATE ... WHERE chain_id IS
NULL` backfill. It names a **Stoolap fork** and its missing Dqa driver. A fork of
a database is a database.

The tree agrees with the substrate section and not with the security table. Four
crates carry SQL schemas (`quota-router-storage`, `octo-vault`,
`quota-router-sm-engine`, `octo-reputation`), and `quota-router-storage` alone
carries **eighteen** migrations covering `asks`, `spend_ledger`,
`settlement_events`, `holder_registry`, `outbox` and `did_registry`. The numbering
runs v001–v020 with v013 and v014 absent, so the sequence has a visible gap that
would want explaining before the table is corrected.

**The honest reading is that §Security & Privacy is stale rather than aspirational.**
It looks like text written for the off-chain MVE and never revisited after the
v2.0 substrate landed. Either way, a security table is the worst place in a
document to carry a claim the same document refutes, because the table is what a
reviewer reads for assurance and what a reader trusts about deployment.

## §Related RFCs

- **RFC-0901 is a Draft.** It is listed as a bare peer of an Accepted RFC, with
  no status qualifier. Its own header reads `Draft`. The §Dependencies convention
  elsewhere in the corpus is careful about this, which makes the omission here
  look like drift rather than house style.
- **`RFC-XXXX: Token Swap Protocol (future)`** is an unfilled placeholder in the
  references list of an Accepted RFC.

Worth narrowing the scope of the second, because a raw count overstates it. Five
accepted RFC files contain the string `RFC-XXXX`, but three of those are using it
as a _convention name_ in prose — a cite-hygiene note about "`RFC-XXXX`
`§section_name` style", and a changelog entry "RFC-XXXX Round 2 M15". Those are
legitimate. The genuine unfilled references are three, across two documents:
two in `numeric/0110-deterministic-bigint` for a Block Header Layout, and this
one.

## What this document gets right

- **The prompt-privacy row is the best sentence in RFC-0900.** `⚠️ TRUST
ASSUMPTION - Sellers see prompt content when executing API calls`, followed by
  "This is a trust-based model, not cryptographic. See Research doc for future
  options (TEE/ZK)." That is the correct disclosure: it names the exposure, states
  the model's class, and points at the research without overclaiming. It is also
  the only place in the document that flags a limitation with a warning glyph
  rather than quietly omitting it.
- **Both §References paths exist** — the use-case document and the research
  document, both present. Unlike the substrate anchors of RFC-0960, nothing to
  fix here.
- **§Implementation does the right thing with superseded material.** Rather than
  deleting it, it names the legacy missions as archived and superseded with a
  date, and points at the current canonical location. That is the model the rest
  of this document's stale sections should follow.

## What the owner should do

- **Decide whether RFC-0900's listing model is the marketplace or the `Ask` is.**
  This is the decision every other item here waits on. The implemented facade is
  `Ask`-shaped and the RFC is listing-shaped; a reader cannot tell which is
  current. Answering that makes `QuotaListing`, the three operations, the
  `'active' | 'exhausted' | 'cancelled'` status and the recorded missing
  lifecycle one question instead of five.
- **Rename or explicitly disambiguate `RouterConfig`.** If RFC-0900's version is
  wanted, it needs its own name; if `Ask`-based routing supersedes it, the RFC
  should say so rather than leaving a name that resolves to a different type in
  an archived RFC's lineage. **This is the highest-value fix on the list**, because
  it is the one item that can make a reader wrongly believe work shipped.
- **Fix the data-residency cell.** Either "no central storage" or a description of
  the storage that §Slash Ledger Substrate specifies. Both are true of different
  parts of the system today; only one is true of the marketplace.
- **Qualify RFC-0901 as Draft** and give `RFC-XXXX` a number or move it to
  §Future Work.
- **Decide whether the 10-prompt minimum is a rule or an idea.** If a rule, it
  needs a gate; if an idea, it belongs beside `MarketTelemetry` in the record of
  intentions.
- **Leave the prompt-privacy row exactly as it is.** It is the section working.

## Reproduction

```bash
# v1.1 — the disclosure that reclassifies everything below as declared, not
# defective. It lives in the v1.1 version-history row, dated 2026-07-24.
grep -o 'The TypeScript interfaces in this RFC remain a wire-shape reference; the Rust impl is the canonical source for the protocol' \
  rfcs/accepted/economics/0900-ai-quota-marketplace.md

# ...in exactly one accepted RFC, so it is a local decision and not house style
grep -rln 'canonical source for the protocol' rfcs/accepted/ --include='*.md' | wc -l
# 1

# CONTROL — Ask exists, so a zero-hit search is distinguishable from a broken one
grep -rn --include=*.rs 'pub struct Ask\b' crates/ | head -3

# the specified model, absent
for s in QuotaListing QuotaPurchase listQuota purchaseQuota routePrompt; do
  printf '%-14s %s\n' "$s" \
    "$(grep -rc --include=*.rs "\b$s\b" crates/ | grep -vE ':0$' | wc -l)"
done
# all 0

# QuotaRouter's only hit is a comment about the project, not the interface
grep -rn --include=*.rs '\bQuotaRouter\b' crates/

# the name collision: RFC-0900's seven fields against the real five.
# -A9 is too narrow here — doc comments push the last two fields out of it.
grep -A20 'pub struct RouterConfig {' crates/quota-router-core/src/router.rs \
  | grep -oE '^\s+pub [a-z_]+' | tr -d ' '
# routing_strategy, latency_window, latency_config, verbose, weights

grep -nE 'max_price_per_prompt|preferred_providers|fallback_enabled|fallback_timeout_ms|require_minimum_balance|auto_recharge' \
  rfcs/accepted/economics/0900-ai-quota-marketplace.md

# and each of them, checked against the real struct: all zero
for f in max_price_per_prompt preferred_providers fallback_enabled \
         fallback_timeout_ms require_minimum_balance \
         auto_recharge_enabled auto_recharge_source; do
  printf '%-24s %s\n' "$f" "$(grep -rc "$f" crates/quota-router-core/src/router.rs)"
done

# ...and its real lineage
grep -n 'RFC-0927 Types (RouterConfig Extension' crates/quota-router-core/src/config.rs
ls missions/archived/ | grep 0927

# the minimum listing gate, absent
grep -rn --include=*.rs -iE 'min_prompts|minimum_listing|MIN_LISTING|min_quantity' crates/

# the self-contradiction, both sides
grep -n 'No central storage' rfcs/accepted/economics/0900-ai-quota-marketplace.md
grep -n 'specifies the storage + in-memory substrate' \
  rfcs/accepted/economics/0900-ai-quota-marketplace.md
grep -rl --include=*.sql 'CREATE TABLE' crates/ | sed 's|crates/\([^/]*\)/.*|\1|' | sort -u
ls crates/quota-router-storage/migrations/ | wc -l
# 18, running v001-v020 with v013 and v014 absent

# §Related RFCs
grep -n 'RFC-0901' rfcs/accepted/economics/0900-ai-quota-marketplace.md
sed -n '1,5p' rfcs/draft/economics/0901-quota-router-agent.md

# the placeholder, and which of the five hits are real
grep -rn 'RFC-XXXX' rfcs/accepted/ --include='*.md'

# what the document gets right
grep -n 'TRUST ASSUMPTION' rfcs/accepted/economics/0900-ai-quota-marketplace.md
for p in docs/use-cases/ai-quota-marketplace.md docs/research/ai-quota-marketplace-research.md; do
  [ -e "$p" ] && echo "EXISTS $p" || echo "MISSING $p"
done
```

## Cross-references

- Mission `rfc-0900-specifies-a-listing-lifecycle-that-does-not-exist-anywhere`
  — the missing lifecycle this extends, amended at v1.2 for the slash-ledger
  mirror
- Mission `rfc-0900-anchors-its-substrate-specification-to-a-gitignored-scratchpad`
  — the same section's unreachable §20.3 anchor
- Mission `the-marketplace-facade-has-no-production-caller` — the `Ask`-shaped
  facade this measures RFC-0900's listing model against
- Mission `documentation-that-describes-a-mechanism-nobody-built` — the class
  `QuotaListing` and the three operations join
- Mission `sixty-nine-accepted-rfcs-use-mermaid-thirty-two-still-use-ascii-and-nothing-checks`
  — the same document carrying both diagram conventions
- Mission `archived/0927-a-router-config-extension` — the lineage that owns the
  real `RouterConfig`, and why the name collision is not a merge in progress
