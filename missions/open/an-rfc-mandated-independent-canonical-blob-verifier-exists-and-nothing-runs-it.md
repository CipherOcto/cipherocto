---
name: an-rfc-mandated-independent-canonical-blob-verifier-exists-and-nothing-runs-it
description: "RFC-0955-R1 states an independent Python implementation MUST reproduce the canonical anchor bytes. scripts/verify_canonical_blobs.py is that implementation. It works, exits 0, and takes about 37 seconds. It is referenced by no workflow, no hook, no mission and no document, so the RFC requirement is unenforced. Mutation testing confirms it is genuinely independent for the digest path: it stays green against a Rust digest mutation that cargo test rejects, which is exactly the drift it exists to catch."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `an-rfc-mandated-independent-canonical-blob-verifier-exists-and-nothing-runs-it`

## Status

Open. Nothing has been wired, moved, or deleted. `.github/` is owner policy
surface, so the wiring decision belongs to a maintainer.

## The finding

RFC-0955-R1 states a verification requirement:

> **Byte-deterministic property test.** Two independent implementations running
> the same tuple MUST produce the same 32-byte `anchor_digest`. An independent
> Python implementation using the `hashlib.blake3` library MUST reproduce the
> same expected bytes.

`scripts/verify_canonical_blobs.py` is that implementation. It parses the three
pinned vectors from `crates/octo-reputation/tests/canonical_blobs.rs`, rebuilds
the batches in Python, and asserts byte-equality.

It is referenced by **nothing**:

- no workflow in `.github/` mentions it or `canonical_blobs`
- no file under `scripts/hooks/` invokes it
- no mission, RFC, or document references it

An accepted RFC carries a MUST, an implementation satisfying it exists in the
tree, and no process ever executes it. The requirement is satisfied in the
sense that the code is present, and unenforced in the sense that nobody would
notice if it stopped working.

## It is genuinely independent, and that was proven rather than assumed

The script's own failure message says "independent Python impl diverges from
Rust impl", which invites the question of whether it is independent at all.
`rust_dfp_blob` does shell out to `cargo run --bin _dfp_helper`, and that helper
calls `octo_reputation::types::dfp_to_blob` — the same function under test.

So the Dfp _encoding_ is not independent. The digest _composition_ is, and that
is the part that matters. Three mutations, each restored immediately:

| Mutation                                               | Script     | `cargo test` |
| ------------------------------------------------------ | ---------- | ------------ |
| `dfp_to_blob` corrupts one byte                        | fails      | fails        |
| `ReputationAnchorBatch::digest` adds 1 to `batch_size` | **passes** | fails        |
| Pinned `CANONICAL_ANCHOR_BLOB_0_LEAVES` byte flipped   | fails      | fails        |

The second row is the reason the script exists. The Rust tests compare Rust
output to Rust-adjacent constants, so they cannot detect a change to the digest
composition that also moved the constants. The Python implementation reproduces
the composition from the documented layout and stays green against exactly that
mutation.

A script that merely duplicated `cargo test` would be a maintenance liability.
This one catches something `cargo test` structurally cannot.

## Cost

Measured, not estimated:

| Condition                | Wall time | Peak RSS |
| ------------------------ | --------- | -------- |
| Warm cargo cache         | 38.2s     | 243 MB   |
| After `touch`ing the lib | 37.5s     | 198 MB   |

Roughly 37 seconds regardless of cache state, because the cost is dominated by
the `cargo run` of the Dfp helper rather than by compilation. That is cheap
enough for a pull-request gate and expensive enough that it should not run on
every commit.

Two earlier cost figures in this repository were written down as constraints
before being measured and were both wrong. This one is measured.

## Known limitations of the verifier

Recorded so the wiring decision is made with full information:

- The Dfp 24-byte encoding is obtained from Rust, not reimplemented, so a
  `Dfp` layout change is caught only as a divergence against the pinned
  constants rather than as an independent second opinion.
- RFC-0955-R1 names three checks. The pinned-vector check is covered. The
  other two are not implemented anywhere:
  - **Cross-DID distinctness** — two unrelated DIDs with identical scores
    producing different digests. `canonical_blobs.rs` has no such test; the
    only occurrence of "distinct" in that file is a comment about
    `GOVERNANCE_QUORUM`.
  - **Dfp score-byte-equality** — replicating the RFC-0968 §23 TV1 anchor
    envelope with `Dfp::from_f64(0.961)`. The literal `0.961` appears in no
    `.rs`, `.py`, or `.md` file outside the RFC that specifies it.

  Both were searched rather than assumed. The Dfp bytes do flow through the
  verifier, via the Rust helper, but the _comparison the RFC asks for_ is not
  written down anywhere as an executable check.

- It depends on a Python interpreter with `hashlib.blake3` (3.13+) or the
  `blake3` package, so a CI runner needs one of those provisioned.

## Acceptance criteria

- [ ] The wiring decision is made by a maintainer: add a CI step, or record
      why the requirement is satisfied without one.
- [ ] If wired, the step runs on pull requests and its failure is not
      suppressed. A deliberately broken pinned vector must turn the step red.
- [ ] The three limitations above are either resolved or explicitly accepted in
      the RFC's verification status.
- [ ] If it stays unwired, the RFC's MUST is annotated so a future reader does
      not assume the requirement is enforced.

## Notes

Found by censusing `scripts/` for entry points that nothing invokes. Seven
scripts have zero references anywhere in the tree; this is the one among them
that an accepted RFC requires. The other six are unreferenced utilities and are
not proposed for deletion here.
