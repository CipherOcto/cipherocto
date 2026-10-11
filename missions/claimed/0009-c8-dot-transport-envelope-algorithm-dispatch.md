---
name: 0009-c8-dot-transport-envelope-algorithm-dispatch
description: RFC-0009 AC#11, octo-network dot/ slice. Dispatch the 25 transport-envelope verifications that take an already-typed VerifyingKey parameter.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-10T00:00:00.000Z
  v: "2.0"
  depends_on:
    - RFC-0009
    - 0009-b-algorithm-registry
    - 0009-c-verification-dispatch
    - 0009-c7-network-caller-supplied-key-dispatch
---

# Mission `0009-c8-dot-transport-envelope-algorithm-dispatch` — COMPLETE 2026-10-10

## Status

Complete 2026-10-10. Split out of `0009-c3`. Found by
`ac11_pending_table_is_complete_for_every_crate_it_covers`, not by hand.

All 25 sites dispatch. The six files are recorded in `AC11_PENDING` at a count
of **zero** rather than removed, so a future site in any of them fails the gate
instead of reopening a gap nobody is watching.

### The hand-written table was wrong

The scope table below originally read `binding.rs` 4 and `handover.rs` 6. The
completeness guard measured 5 and 5. The guard was right and the prose was not,
which is the third time this arc that a hand count disagreed with the
instrument — so the numbers below are the measured ones and the claim that they
are measured is checked by a test, not trusted.

## Scope

25 public functions under `crates/octo-network/src/dot/` that take an
already-typed `&VerifyingKey` and verify an Ed25519 signature.

| File                | Sites |
| ------------------- | ----- |
| `dc_envelopes.rs`   | 7     |
| `kick_envelopes.rs` | 5     |
| `binding.rs`        | 5     |
| `handover.rs`       | 5     |
| `decommission.rs`   | 2     |
| `sub_group.rs`      | 1     |

The count is measured by the completeness guard, not asserted here.
`0009-c3` §The 27 typed-key sites said 27 by hand; two have since been
refactored.

## Why the recorded blocker does not apply

`0009-c3` §Why this is blocked argued these sites were cosmetic — "an Ed25519
type in a public signature is a real coupling... it is just a different defect
from AC#11's."

That reasoning about _scope_ was right and its conclusion about _difficulty_ was
wrong. Nothing here parses a DID: the caller supplies an already-typed
`VerifyingKey`, so the caller is the only party that knows which algorithm it
belongs to. Adding an `AlgorithmId` parameter is the same edit
`0009-c7` made for byte-parameter sites, and it is exactly the change that
un-asserts Ed25519 from the public signature.

## What this is NOT

This is **not** AC#11's literal text — those name `VerifyingKey::from_bytes`
and `Signature::from_bytes`, and these call neither. It is the coupling
AC#11 exists to prevent, arrived at from the other side, and the mission
should be named for what it does rather than folded into a criterion it does
not satisfy.

## The change

Each `verify(&self, key: &VerifyingKey)` becomes
`verify(&self, algorithm: &AlgorithmId, key: &VerifyingKey)`, and dispatches
through `verify_dispatch` instead of `key.verify(...)`.

`octo-network` already gained both dependencies in `0009-c7`, so no manifest
change is needed here.

## Blast radius — measured, not estimated

Three transport adapters call these envelopes with the same shape `p2p` did:
`octo-adapter-wechat`, `octo-adapter-udp`, `octo-adapter-lora`. All three
verify against the envelope's **own claimed** `source_peer` and have no peer
registry, so they name Ed25519 explicitly at the call site — the same honest
limitation `0009-c7` recorded for `octo-adapter-p2p`.

**The M-15 vector covers those adapters** and must be updated, as it was for
p2p. Read its current per-call form before editing: it checks every `.verify(`
call individually, and the earlier "at least one" version was mutation-caught
passing with a hardcoded key.

## Acceptance Criteria

Verified by the §Test Vectors list.

## Test Vectors

- `dot_envelope_dispatch_on_declared_algorithm_verifies` — happy
  path. **Control:** refusing everything must not pass this.
- `dot_envelope_refuses_an_unregistered_algorithm` — the
  fail-closed half. **This is the only vector that distinguishes dispatch from
  a hardcoded Ed25519**: on `octo-vault`, `octo-coordinator-types` and
  `octo-network`'s byte-parameter sites, a hardcoded verifier passed the happy
  path _and_ the bad-signature check, and only this control went red.
  Hardcoding the algorithm here turns exactly that one vector red.
- `dot_envelope_still_refuses_a_bad_signature` — dispatch did not
  degrade the check into a lookup.
- existing `dot/` vectors must pass unchanged. 1528 lib vectors pass.

### The gate found a hole in its own zeroed counts

The six files were recorded at zero, and a `Signature::from_slice` planted in
`sub_group.rs` left **all 47 vectors green**. The zeroed counts exist precisely
to make a new site fail; this constructor walked past them.

The cause was two matchers for one question. The pending count summed two
constructors with a raw substring match, while the completeness scan looked for
three with path-boundary matching. The count also disagreed in the other
direction — `GovernanceSignature::from_bytes` would have inflated a count.

Both now call one `ac11_site_count`, so the count and the completeness check
cannot drift apart again. Merging them moved `quota-router-core`'s
`receipt.rs` from 3 to 4: its serde `Deserialize` rebuilds a signature with
`from_slice`, which the old two-constructor count could not see. That entry was
wrong before this mission touched it.

Re-running the same plant after the fix turns `ac11_pending_sites_are_counted`
red with the count and the table disagreeing.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-network -p octo-adapter-wechat -p octo-adapter-udp -p octo-adapter-lora -p octo-cli --all-targets --all-features -- -D warnings
cargo test -p octo-network --lib
cargo test -p octo-cli --test security_regressions
```

`octo-network` is long to build. Do not add it to another mission's gate.

## Backward compat

None required. Nothing is deployed. These are public functions with no caller
outside the workspace.

## Cross-references

- `rfcs/accepted/process/0009-identity-evolution-v12.md` — AC#11
- `missions/archived/completed/0009-c3-network-verification-dispatch` — where the
  categorization lives
- `missions/claimed/0009-c7-network-caller-supplied-key-dispatch` — the same
  edit for byte-parameter sites, and the adapter pattern to copy
- `crates/octo-cli/tests/security_regressions.rs` — `AC11_PENDING`

## Claimant

@unassigned

## Pull Request

#
