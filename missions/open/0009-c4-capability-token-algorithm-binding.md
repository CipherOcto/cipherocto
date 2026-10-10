---
name: 0009-c4-capability-token-algorithm-binding
description: RFC-0009 AC#11, blocked site 1 of 2. Give CapabilityToken the algorithm its holder signature is verified under, and stop naming ed25519_dalek::Signature in the capability token's public API.
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

# Mission `0009-c4-capability-token-algorithm-binding` — OPEN 2026-10-09

## Status

Open (2026-10-09). **Unblocked** as of mission `0009-c`: `octo_ident::AlgorithmId`
carries serde, which was the prerequisite. See §Prerequisite, now met.

This is one of the two sites mission `0009-c` explicitly could not dispatch.
The other is `octo-protocol` §`verify_ed25519_signature`; see
§The other blocked site.

## Dependencies

`depends_on: 0009-b-algorithm-registry, 0009-c-verification-dispatch`.
`0009-c` landed the registry, the singleton, the Ed25519 registration and
`AlgorithmId` serde; this mission consumes all four.

## Prerequisite, now met

`0009-c` recorded this site as blocked because the algorithm would have to
travel on the token, and `AlgorithmId` had no serde impl — so the field
could not be added without a Layer B wire-form change underneath it. That
change has landed: `AlgorithmId`, `VerificationMethodId` and `CircuitId`
serialise as the **textual** UUID, while the signable prefix continues to
read the newtype's own 16 raw bytes. Both forms are in use and RFC-0009
§Domain separation makes picking differently a domain-separation bug, so
**the field must be read from bytes, never from a serde round-trip.**

## Scope

`octo-cap-macaroon` §`CapabilityToken`:

- The token must carry the algorithm its `holder_sig` is verified under.
- `verify_holder_sig` dispatches on that field rather than assuming Ed25519.
- `holder_sig`'s type stops being `ed25519_dalek::Signature`.

### Why the type change is not optional

Mission `0009-c` §Layer direction says: "If `octo-cap-macaroon` still names
`ed25519_dalek::Signature` in its public API after this mission, the
abstraction did not do its job." A capability token is a **capability
signature** — the exact category AC#11 names — and its signature field is
the public face of that coupling.

Adding an algorithm field while leaving `holder_sig` as a dalek type buys
nothing: the type already says Ed25519, so a non-Ed25519 token is
unrepresentable regardless of what the algorithm field says.

**Preferred shape:** a `HolderSignature` newtype carrying
`algorithm_id: AlgorithmId` plus the 64 signature bytes, with its own serde
module replacing the current `ed25519_sig_serde`. That removes the dalek
type from the API, gives the algorithm a home, and keeps the Debug
redaction property.

## Blast radius — size this before starting

| Surface                                    | Count |
| ------------------------------------------ | ----- |
| `CapabilityToken { … }` construction sites | 18    |
| Files containing a construction site       | 10    |
| Files reading `holder_sig` or `holder_pub` | 12+   |
| Crates with a declared ownership conflict  | 2     |

Construction sites: `octo-cap-macaroon` §`token`/`wire`/`holder_persist`/
`discharge`, plus `octo-cli` §`commands/capability`, `octo-governance-core`
§`capability`, `octo-governance` §`vote`, `octo-protocol` §`authorization`,
and `octo-wallet` §`capability/verify` with its `redemption_subgraph` test.

`octo-cap-zk` pins `holder_sig` as `ed25519_dalek::Signature` in seven
places and is owned by `0957-ext-zk-crate`. `octo-cap-macaroon` is owned by
`0957-ext-macaroon-crate`.

**The coordination question, answered rather than assumed.** The earlier
draft of this file said "coordinate both before landing" as though work
were in flight. Checked: `0957-ext-macaroon-crate` is **closed** (Phase 2,
2026-08-09) and `0957-ext-zk-crate` has sat Open since 2026-08-08 with no
activity. There is no concurrent work to collide with.

The real coupling is not a schedule conflict, it is that `octo-cap-zk`
reads `holder_sig` as a dalek type in seven places. That is a **compile
dependency this change invalidates**, and it has to be updated in the same
change or the workspace does not build. It is listed above as blast radius
for exactly that reason.

Nothing is deployed, so there is no migration cost. That is not a reason to
skip the coordination; it is a reason not to add a compatibility shim.

## Domain separation — audited, and the answer is reassuring

`AlgorithmId` now serialises as the textual UUID while the signable prefix
must use 16 raw bytes (RFC-0009 §Domain separation). The obvious hazard is
a deserialised id reaching a prefix as text.

**There is no production signable-prefix builder.** The only construction
anywhere is in the `raw_prefix_is_self_delimiting` test, and it reads
`as_bytes()`. So no code path can produce a textual prefix today, and
adding serde introduced no reachable hazard.

**When the builder lands, it must read `as_bytes()`.** Not `to_uuid()`,
not `to_string()`, not a serde round-trip. Two vectors already pin the
distinction: one asserts serde emits 38 characters while storage stays 16
raw bytes, the other asserts an upper-case document yields the same 16
bytes. This file restates it because the builder does not exist yet, and
the rule has to be read at the moment someone writes it.

## Acceptance Criteria

Verified by the §Test Vectors list.

## Test Vectors

- `holder_sig_dispatches_on_the_tokens_algorithm` — a genuine signature
  verifies under the algorithm the token names. **Control:** refusing
  everything must not pass this.
- `capability_token_with_an_unregistered_algorithm_is_refused` — the
  fail-closed half. With the algorithm on the token, an identifier this
  deployment does not implement must be refused, not treated as Ed25519.
  This is the control that the field is actually read.
- `holder_signature_round_trips_through_the_wire_form` — the newtype
  survives `serialize_wire` / parse, with the algorithm intact.
- `capability_token_debug_still_redacts_the_signature` — the existing
  redaction property must survive the type change. A newtype is exactly
  where a `#[derive(Debug)]` would leak the bytes.
- `no_capability_api_names_ed25519_dalek` — grep guard: no
  `ed25519_dalek::Signature` in `octo-cap-macaroon`'s public surface. This
  is the check mission `0009-c` could not write, because it would have
  failed on arrival.

## Layer direction (per `cipherocto-design-principles`)

`octo-cap-macaroon` is Layer E consuming the Layer B registry. No new
upward edge: the algorithm arrives from `octo-ident` on the token.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-cap-macaroon -p octo-cap-zk -p octo-governance -p octo-governance-core -p octo-cli --all-targets --all-features -- -D warnings
cargo test -p octo-cap-macaroon --lib
cargo test -p octo-cap-zk --lib
cargo test -p octo-governance --lib
```

Plus a workspace compile check: 18 construction sites across 10 crates means
a missed one is a broken build elsewhere, not a local failure.

## Backward compat

None. Nothing is deployed. **Do not add a default that falls back to
Ed25519** — a silent default is precisely what AC#11 forbids, and a
`#[serde(default)]` here would reintroduce it invisibly.

## The other blocked site

`octo-protocol` §`verify_ed25519_signature` is blocked differently:
`Authorization::Signature` carries no algorithm and the `did:octo:z` payload
is a bare 32 bytes. Fixing it is a Layer 1 envelope wire change under
RFC-0870, not a capability-token change — a separate mission with a
separate review. **Do not fold it in here**; the two touch different wire
formats owned by different RFCs.

Note the function's current name would become misleading the moment it
dispatches, since a caller-supplied algorithm behind a name reading
`verify_ed25519_signature` is worse than the honest Ed25519-only name it
has today.

## Cross-references

- `rfcs/accepted/process/0009-identity-evolution-v12.md` — AC#11, §Domain separation
- `missions/claimed/0009-c-verification-dispatch` — the slice that dispatched 18 sites and recorded this one
- `missions/claimed/0009-b-algorithm-registry` — the registry
- `missions/claimed/0957-ext-macaroon-crate`, `0957-ext-zk-crate` — crate ownership conflicts

## Claimant

@unassigned

## Pull Request

#
