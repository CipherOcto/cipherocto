---
name: 0009-c2-verification-dispatch-new-dependency-sites
description: RFC-0009 AC#11, sibling slice. Dispatch 3 sites whose crates do not yet depend on octo-ident, and settle whether a third-party protocol signature belongs in the registry at all.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-09T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-b-algorithm-registry
    - 0009-c-verification-dispatch
---

# Mission `0009-c2-verification-dispatch-new-dependency-sites` — OPEN 2026-10-09

## Status

**Claimed and implemented (2026-10-09).** `quota-router-storage` 156,
`octo-settlement-core` 12 lib + 88 integration, `octo-adapter-nostr` 18,
`octo-cli` security guards 44 — all passing. Clippy clean across
`quota-router-storage`, `octo-settlement-core`, `octo-adapter-nostr`,
`octo-cli` and `octo-settlement` with `--all-targets --all-features`.

Both dispatchable sites are done. The nostr site is **decided out of scope**
rather than left unexamined — see §The nostr decision.

## Dependencies

`depends_on: 0009-b-algorithm-registry, 0009-c-verification-dispatch`. This
slice is defined against the pattern `0009-c` lands, not against the registry
alone.

## Scope

The `from_bytes` sites in crates that **do not** depend on `octo-ident`, so
reaching the registry means adding a **new dependency edge**.

| Crate                  | Sites | What it verifies                         |
| ---------------------- | ----- | ---------------------------------------- |
| `quota-router-storage` | 2     | `asker_public_key` on a signed ask       |
| `octo-settlement-core` | 1     | router signature on a settlement receipt |
| `octo-adapter-nostr`   | 1     | a **nostr event** signature              |

Four sites. Small — the size is not why it is separate.

## Why this is its own mission

**Each edge needs its own justification.** The repository convention is that
every non-trivial `Cargo.toml` dependency carries a comment explaining which
layer it sits in and which RFC mandates it. Three new edges to a Layer B crate,
across three crates with three different owners, is a per-edge argument — not
something to slip into a 20-site diff where nobody reads the manifest diff.

`0009-c` deliberately has **zero** new edges. That is what makes its green run
trustworthy: it cannot have silently created an upward dependency. Preserving
that property is the reason this slice exists separately.

## The nostr site needs a decision, not a dispatch

`octo-adapter-nostr` verifies a **nostr event** signature. NIP-01 specifies
Ed25519; there is no alternative to select. Routing it through
`AlgorithmRegistry` would be a **semantic error**, not dispatch.

`AlgorithmRegistry` answers "which algorithm does _this identity document_
declare for this key". A nostr event carries a third-party protocol's fixed
key format that has no identity document behind it. Those are different
questions, and the registry is not the answer to the second one.

**Decide this on the record before implementing.** The defensible outcome is
that this site is **out of AC#11's scope** and stays on `ed25519_dalek`,
with the reasoning written down — because AC#11 reaches "holder- or
capability-signature verification sites", and a nostr event is neither. If you
conclude otherwise, write down what a nostr event's "declared algorithm"
would even mean before touching the code.

Do not silently dispatch it because it appeared in the same grep as the two
sites that genuinely need dispatch.

## The nostr decision

`octo-adapter-nostr`'s site verifies a **nostr event** signature. It is a
test-local check, and NIP-01 specifies Ed25519 with no alternative — there
is nothing to select.

`AlgorithmRegistry` answers "which algorithm does _this identity document_
declare for this key". A nostr event has no CipherOcto identity document
behind it; its key format is a third-party protocol's, fixed and
non-negotiable. Routing it through the registry would be a **semantic
error**, not dispatch.

**Decided out of AC-11's scope**, and recorded rather than omitted. AC#11
reaches "holder- or capability-signature verification sites"; a nostr event
is neither. If a genuine holder-signature site is ever added to that crate,
it **is** in scope and must dispatch — which is why the entry lives in a
table with a guard rather than in a comment.

## Acceptance Criteria

Verified by the §Test Vectors list.

## Test Vectors

- `no_verification_site_hardcodes_an_algorithm` — the AC#11 vector, scoped to
  this mission's crates. If the nostr site is ruled out of scope, the vector
  must **allowlist it explicitly with the reason inline**. An allowlist entry
  with no reason is indistinguishable from the defect it hides.
- `new_identity_dependency_edges_are_justified` — every `Cargo.toml` line added
  by this mission carries a layer-and-RFC comment per the repository
  convention. This is a read check, not a test; state it as such.

## Layer direction (per `cipherocto-design-principles`)

- `octo-ident` (Layer B) — the registry; direction is B into the consumer
- `octo-adapter-nostr` (Layer D) — D depends on B, which is permitted
- `octo-settlement-core`, `quota-router-storage` — **layer not yet asserted in
  their manifests.** Establish and record it before adding the edge. A new edge
  into a crate whose layer is undocumented cannot be audited.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p quota-router-storage -p octo-settlement-core -p octo-adapter-nostr --all-targets --all-features -- -D warnings
cargo test -p quota-router-storage --lib
cargo test -p octo-settlement-core --lib
cargo test -p octo-adapter-nostr --lib
```

Do not run the full workspace suite.

## Backward compat

None required. Nothing is deployed.

## Cross-references

- `rfcs/accepted/process/0009-identity-evolution-v12.md` — AC#11
- `missions/claimed/0009-c-verification-dispatch` — the pattern to copy
- `missions/claimed/0009-b-algorithm-registry` — the registry
- `missions/archived/completed/0009-c3-network-verification-dispatch` — the third slice

## Claimant

@unassigned

## Pull Request

#
