---
name: rfc-0959-test-vectors-contain-no-expected-values
description: "RFC-0959 §Test Vectors opens by making cross-implementation verification a gate on the RFC's own promotion: at least two independent implementations producing identical digests, REQUIRED before promotion to Accepted. The RFC is Accepted, no digest in any of the four vectors was ever written down, and no reference implementation exists. Test vector 1 leaves every expected value as an implementer placeholder. Test vector 3 is advertised as byte-exact reproducible across implementations and defers its expected digest to crates/octo-core/tests/fixtures/cache_key_tv3.json, a path in a crate that has no tests directory, while the implementation it would pin lives in quota-router-storage. The gate is nevertheless satisfiable in two lines, and I discharged it: Rust cache_key([100,200,300,400]) and an independent Python blake3 both produce de460af64e413eadc814d27184fc92c7b430371e2a406b70011d30ea45078a2c. Two vectors name substrate that does not exist: TV1 asserts an error ReceiptReplay that the tree explicitly documents as never having been a substrate variant, and TV4's R4 overflow guard is implemented as a panic where the RFC specifies Err(OCTO_WAmountOverflow), measured. Separately, crates/quota-router-storage/tests/tv_0959_settlement_wire.rs implements none of the four vectors under a filename that promises they are covered."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0126
    - RFC-0853
---

# Mission `rfc-0959-test-vectors-contain-no-expected-values`

## Status

Deferred. Publishing the digests is a content-address decision that moves with
the other `ask_id` migrations already queued, and TV4's API names need an RFC
call. Nothing was actioned beyond two probes, both reverted.

Fourth systematic sweep of a structure RFC-0959 points at, after the roles
table, the acceptance gates and the implementation assumptions.

## Finding

§Test Vectors contains four vectors and a twelve-bullet property matrix. **Not
one expected output value exists anywhere in the tree.** Every digest is a
placeholder reading "implementer computes", and the one vector that promises to
be byte-exact defers its answer to a file that is not there.

Worse, the section header makes verification a condition of the RFC's own
promotion:

> Cross-implementation verification (≥ 2 independent implementations producing
> identical digest) is REQUIRED before RFC-0959 promotion to Accepted.

RFC-0959 is Accepted. The gate was never run.

## TV3 — the vector that could have been checked, and nearly was

TV3 is the only vector with no `canonical_ser` indirection. It gives a 32-byte
key, sixteen explicit input bytes, and the exact BLAKE3 keyed-hash call. It
then writes:

```
expected = 0x<32-byte hex digest; see `crates/octo-core/tests/fixtures/cache_key_tv3.json`>
```

Three things are wrong with that line, and each was checked:

- `crates/octo-core/` has **no `tests/` directory at all**. The fixture path
  does not exist.
- The implementation is not in `octo-core`. `cache_key` and
  `CACHE_KEY_DOMAIN` live in `quota-router-storage`'s `cache_key` module, which
  is also where the one test file for this behaviour already is.
- The `cache_key` tests that do exist are all _relational_ — same input gives
  same output, different input gives different output. Not one pins an absolute
  digest.

So the vector whose entire purpose is a cross-implementation byte comparison
compares nothing.

## The gate, discharged

Because the inputs are fully specified and the construction is
`canonical_prompt_bytes` followed by `blake3::Hasher::new_keyed`, the digest is
computable without any further decision. Two independent implementations:

```text
Rust   cache_key([100, 200, 300, 400])
     = de460af64e413eadc814d27184fc92c7b430371e2a406b70011d30ea45078a2c

Python blake3(b"\x64\x00\x00\x00\xc8\x00\x00\x00\x2c\x01\x00\x00\x90\x01\x00\x00",
              key=b"cipherocto/cache-key/v1.........")
     = de460af64e413eadc814d27184fc92c7b430371e2a406b70011d30ea45078a2c
```

**They agree.** The promotion gate RFC-0959 required is satisfiable in two
lines and was simply never exercised. The value above is not committed as a
fixture here — writing it is an implementer decision, because pinning a digest
is exactly the kind of thing that should land with the content-address
migration already queued. But the point stands without it: the gate costs
minutes and currently costs nothing because it was skipped.

## TV1 — asserts an error the substrate does not have

TV1 ends with the replay assertion:

```
assert ConsumedReceiptIndex.try_insert(&router_did, receipt_id).is_err()  # second insert = ReceiptReplay
```

`ReceiptReplay` appears **once** in the whole tree, in a comment:

> `ReceiptReplay` was NEVER a substrate variant. Audit\* variants live in
> `SettlementAuditError`, NOT here.

The real discriminant is `SettlementDecision::AlreadyConsumed`. So the vector's
_behaviour_ is the intended one and its _name_ is a variant the substrate
rejected. A reader checking the tree for `ReceiptReplay` finds only the comment
saying it does not exist — which is a fair description of the RFC's relationship
to its own substrate here.

## TV4 — the R4 overflow guard is a panic

TV4 is described as byte-exact and pure arithmetic, and its last assertion is
the R4 critical fix:

```rust
let overflow = MicroOCTO_W(u128::MAX).to_micro_checked();
assert overflow.is_err();  // OCTO_WAmountOverflow
```

None of that API exists. Measured and confirmed absent: `MicroOCTO_W`,
`to_octow`, `to_octow_ceil`, `to_micro_checked`, and the error variant
`OCTO_WAmountOverflow`. What exists is `OCTO_WAmount(pub Dqa)` over the general
`Dqa` newtype, whose `to_micro` is `expect("non-overflow")`.

Probed at the substrate's real bound, `Dqa.value` being `i64`:

```text
PROBE tv4 = PANICKED (RFC expects Err(OCTO_WAmountOverflow))
PROBE tv4_roundtrip = Dqa { value: 7000000, scale: 0 }
```

The happy path is correct — 7 OCTO-W becomes 7_000_000 micro, as specified. The
guard is the problem: the RFC specifies a fallible `Result` and the code
`expect`s, so the overflow case a `Result` was introduced to handle **aborts
the process instead**. TV4 is also the vector that specifies a `u128` wire
unit where the substrate uses `i64` plus a `u8` scale, so the overflow bound in
the vector is two orders of magnitude away from the real one.

## TV2 — the arithmetic does not close

TV2 states its expected cost and then derives a different one, in the same
line, ending mid-thought:

```
cost = MicroOCTO_W(150_000)   # cached(500) @ 100_000 + output(300) @ 0 +
                             # input(0) @ 500_000 = 50_000 + 0 + 0... (axis rates per TOML)
```

`150_000`, `100_000`, `50_000` and `500_000` appear in one line and none
reconciles. The comment trails off with an ellipsis and defers to
`pricing-axes.toml` "at test time". This is the only place in the four vectors
where a human was evidently mid-calculation, and it was accepted that way.

## A file named for these vectors implements none of them

`crates/quota-router-storage/tests/tv_0959_settlement_wire.rs` holds 25
`#[test]` fixtures, in four families:

| Fixtures               | Family                                                                   |
| ---------------------- | ------------------------------------------------------------------------ |
| `tv_0959_01..10`       | `dqa_*` — decimal-quantisation wire round-trip, canonicalisation pinning |
| `tv_0959_11..15`, `20` | `cost_vault_id_*` — cost-to-vault identity binding                       |
| `tv_0959_16..19`       | `cross_chain_*` — cross-chain vault matching and rejection               |
| `tv_0959_21..25`       | `vault_lookup_*` — `VaultLookup` trait-surface invariants                |

**None of them implements TV1, TV2, TV3 or TV4.** No fixture computes a
`settlement_hash` or a `receipt_id`; the `settlement_hash: [0u8; 32]` field
appears throughout only because `SettlementEnvelope` carries it, and the
assertions never read it.

To be fair to the file: the fixtures are good work, and the Dqa family pins
real byte values from a capture run. The defect is the **filename**, which
promises RFC-0959 test-vector coverage the file does not provide — the same way
an empty `is_snake_case` read like a validation path in IA-6.

## The property matrix — what genuinely holds

Twelve bullets, mostly of the form "N random X → Y". One of them is a real
property test and deserves to be named as such:

`settlement_hash_property_10k_replays` runs **10,000 real iterations** with a
seeded xorshift RNG, rebuilding a full `SettlementEvent` each time and asserting
two independent recomputations agree. It is not a 10K-iteration test in name
only.

Its one shortfall is that the matrix asks for replay "across 2 nodes" and the
test recomputes in a single process. Same-process determinism is a strictly
weaker property: it cannot catch a serialisation or platform difference between
nodes, which is the failure the matrix is reaching for.

The other bullets are covered as single examples rather than properties —
`cached_axis_requires_cache_key_hash`, `tampered_payload_breaks_verify`,
`settlement_replay_defense` — which is a reasonable thing for a test suite to
contain and not a reason to call them covered against a matrix that asks for
100 to 10,000.

### The one bullet with no test at all

The matrix's eleventh bullet is the sharp one:

> 100 random cost-tampered events → `verify_receipt` fails (cost bound into
> hash breaks tampering)

There is no such test, and the reason is that the property is false. Measured
against `compute_settlement_hash` by varying only `cost`:

```text
cost = 90_000        -> 8a1b526a2cb7f55096495aa982b456ecb27fee5d3b380434cec76e264b234ffa
cost = 1             -> 8a1b526a2cb7f55096495aa982b456ecb27fee5d3b380434cec76e264b234ffa
cost = 999_999_999   -> 8a1b526a2cb7f55096495aa982b456ecb27fee5d3b380434cec76e264b234ffa
settled_at_unix = 42 -> 8a1b526a2cb7f55096495aa982b456ecb27fee5d3b380434cec76e264b234ffa
```

Byte-identical throughout. A cost-tampered event cannot be caught by the
settlement hash, so a test asserting the hash catches it could not be written.
The existing `receipt_tampered_event_rejected` passes — but it recomputes the
expected hash from the _tampered_ event, so the hash check passes and the
rejection comes from the router's Ed25519 signature.

That finding is already recorded in
`settlement-hash-does-not-bind-the-cost`, which distinguishes the two settlement
surfaces and notes the same signature-bound behaviour. It is confirmed here,
not re-argued. What is new is the RFC angle: **the matrix assigns the
tamper-detection property to the hash, so the bullet can never be satisfied by
the design as specified** — it needs the preimage change the other mission
already defers.

## What the RFC owner should do

- **Publish the digests.** TV3 and TV4's inputs are fully specified and TV3's
  digest is computed above. Pinning them turns four placeholders into four
  assertions, and turns the promotion gate from a claim into a check.
- **Create the Python reference implementation** the matrix names
  (`pyca/cryptography` + `blake3`) as a dev-only tool, so the cross-impl gate is
  runnable by anyone rather than by whoever remembers to hand-compute one.
- **Fix TV4's API names and make the overflow a `Result`.** The panic is the
  substantive half. A `Result`-returning `to_micro_checked` with a named error
  is a small, non-breaking addition if added alongside `to_micro`.
- **Correct TV1's `ReceiptReplay` to `AlreadyConsumed`,** and reconcile TV2's
  three inconsistent numbers.
- **Rename or repoint `tv_0959_settlement_wire.rs`.** The fixtures in it are
  good; the filename claims coverage they do not provide.

## Reproduction

```bash
# TV3's expected digest is deferred to a path that does not exist
ls crates/octo-core/tests/ || echo "no tests dir in octo-core"
find . -name 'cache_key_tv3.json' -not -path './target/*' || true

# the implementation is here, not there, and pins no absolute digest
grep -rn 'pub fn cache_key(' --include=*.rs crates/quota-router-storage/

# the promotion gate is satisfiable right now, in two lines
python3 -c "
from blake3 import blake3
k=b'cipherocto/cache-key/v1.........'
i=b''.join(t.to_bytes(4,'little') for t in (100,200,300,400))
print('py ', blake3(i, key=k).hexdigest())"
# rust side: cache_key(&[100, 200, 300, 400]) in quota-router-storage
# both give de460af64e413eadc814d27184fc92c7b430371e2a406b70011d30ea45078a2c

# TV1's error variant, per the tree's own comment
grep -rn 'ReceiptReplay' --include=*.rs crates/
grep -n 'AlreadyConsumed' crates/quota-router-sm-engine/src/settlement_event.rs

# TV4's API is absent (MicroOCTO_W survives only in one doc comment)
grep -rn 'OCTO_WAmountOverflow\|fn to_octow\|fn to_micro_checked' --include=*.rs crates/ \
  || echo "none of TV4's names exist"

# the file named for the vectors implements other families entirely
grep -n 'fn tv_0959' crates/quota-router-storage/tests/tv_0959_settlement_wire.rs \
  | grep -vE 'dqa|cost_vault_id|cross_chain|vault_lookup' \
  || echo "every fixture is dqa / cost_vault_id / cross_chain / vault_lookup"
grep -c 'fn tv_0959' crates/quota-router-storage/tests/tv_0959_settlement_wire.rs
```

## Cross-references

- RFC-0959 §Test Vectors — the section swept
- Mission `four-of-rfc-0959s-ten-implementation-assumptions-fail` — the
  assumptions sweep; IA-5 names the same `canonical_ser` the vectors defer to
- Mission `four-of-rfc-0959s-six-acceptance-gates-are-not-met` — the gates
  sweep; the same "documented as a condition, never evaluated" shape
- Mission `settlement-hash-does-not-bind-the-cost` — independently measured
  here that the settlement hash is blind to `cost` and `settled_at_unix`; that
  mission already records the finding, so this one does not restate it
- Mission `the-same-ask-has-two-different-content-addresses` — the other
  `ask_id` divergence
