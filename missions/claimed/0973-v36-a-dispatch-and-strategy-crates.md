# 0973-v36-a-dispatch-and-strategy-crates — Dispatch algorithm, operator policy, registration host, reference strategies, determinism fixture

**Status:** Claimed — @mmacedoeu (agent), 2026-10-07
**RFC:** RFC-0973 (Economics): Pluggable Proof Attestation for Capabilities — §Implementation Phases §Phase 2: Enhanced
**Substrate:** `octo-wallet` (dispatch, boot validation, wallet verification entry point) + `octo-wallet-node` (registration host) + one new strategy crate (`octo-cap-poi`) and one existing crate gaining a strategy (`octo-cap-zk`)
**Depends on:** Mission `0973-v36-attestation-core-substrate` — provides `AttestationTypeId`, both traits, `Admission`, `AdmissionProfile`, `CostFigure`, `SecurityFigure`, `Expectation`, `PolicySource`, `AttestationVerifyCtx`, `AttestationOutcome`, the envelope wire form and its decode bounds, and the allocation record with its named owner.

## Scope

Land the dispatch that decides acceptance, the boot validation that refuses a
node it cannot serve, the host that makes registration possible, and the two
reference strategies plus a fixture that lets determinism be measured at all.

1. **`verify_capability_attestation`** — the fail-closed dispatch, in order:

   - step 0 — capability registry lookup on the capability id. Authorisation
     precedes attestation: everything past this point can be satisfied by an
     attacker who minted their own capability, so this is the gate that makes
     acceptance mean anything.
   - step 0b — the RFC-0957 macaroon verification: holder signature, caveat
     evaluation, every check the token format already defines. **This runs on
     both branches.** An attested capability is still a capability: the
     attestation is an additional check layered on top, never a replacement. An
     implementation that runs it only for unattested tokens accepts a copied
     token whose signature was stripped.
   - step 1 — absent attestation id accepts, having already run step 0b. A
     token carrying an id never falls back to the unattested path alone.
   - step 2 — the operator acceptance set.
   - step 3 — strategy resolution, fail-closed.
   - step 4 — the verification-side coverage gate, reachable per call.
   - step 5 — build the expectation from the capability, rejecting if there is
     none, then `verify`.

   The strategy call is wrapped in `catch_unwind` inside an `AssertUnwindSafe`
   boundary. A panic is a rejection carrying a visible reason, never an
   acceptance and never a propagated fault. The unwind-safety wrapper is
   required by the borrow checker, but note what it is: an assertion the
   compiler cannot check. It discharges a compile-time obligation and leaves
   the runtime question open, which is why the guarantee is scoped to an
   unwinding build.

2. **`validate_policy`** — boot-time operator policy validation covering
   resolvability, producing-side coverage, and the admission budget. An
   unmeasured cost fails the boot rather than passing on a guess, as does a
   measured cost exceeding the node's budget. Both are decisive; note that a
   strategy omitting the admission trait also skips both gates, which is the
   fail-open path Lifecycle Requirement 1 closes by review.

   The producing-side branch runs only when
   `required_payload_kinds` is non-empty. The verification-side requirement is a
   different gate and is deliberately **not** checked at boot, so that step 4
   stays reachable for a node that boots but cannot verify.

3. **Registration host** — the node or binary crate that composes a node. It is
   what supplies the claimant the allocation-ownership gate compares against,
   since it observes crates at link time. Core must not stand in for it: it may
   not link a strategy crate, and it cannot observe crates at link time, so a
   core-supplied claimant would be exactly the self-asserted identity the check
   exists to reject. The strategy links land **here**, because registration
   cannot happen without a host to register from. The lint that keeps the host
   the only such linker is mission D.

4. **Reference strategy crates** — `octo-cap-zk` (`root_secret_knowledge`) and
   `octo-cap-poi` (`in_boundary_execution`, new).

   These are not in the same state. `crates/octo-cap-zk` **already exists**
   (the RFC-0958 ZK capability subclass, with its mint API, `ProofBundle` and
   `ZkMintError`); this mission **extends** it with the `root_secret_knowledge`
   strategy rather than creating it, and a second crate of that name would
   collide. `crates/octo-cap-poi` does not exist and is created here.

   Both start with no registry entry. Neither declares payload kinds and both
   answer `false` for `verifier_needs_producing_kinds`, which is a decision:
   the root-secret relation is a pure computation over the token and the
   in-boundary trace is verified against material the node already holds, so
   neither imposes a deployment requirement on an operator.

   `octo-cap-zk` **does** carry a circuit cost, so it implements
   `AttestationAdmission` and answers `admission_profile` with
   `CostFigure::Unmeasured` — not `None`. A `None` means "this strategy carries
   no circuit cost", which would be a false claim about a proof-bearing crate
   and would skip both budget gates at boot. `Unmeasured` is what makes the
   gate decisive on an unknown cost.

   `octo-cap-poi` also carries a circuit cost — it is a proof-of-inference trace
   bound to the token — so it answers `Unmeasured` too. An earlier revision of
   this mission said it "either omits the trait or answers `None`; both are
   truthful"; that is wrong, and the reasoning is recorded under
   §Implementation notes. `None` would only be truthful of a strategy carrying
   no proof.

   Register `in_boundary_execution` in this phase once its profile is measured
   and its relation is verified against the trace the boundary actually
   produces. `root_secret_knowledge` stays unregistered — its relation does not
   hold yet, and that is mission B.

5. **Measured profiles** — record the `in_boundary_execution` admission profile
   here, measured rather than estimated, since this is the phase that lands the
   crate it describes. For `octo-cap-zk` this phase records the **`Unmeasured`**
   profile described above; the measured value lands in mission C, which is the
   phase that builds the real prover path.

6. **Determinism fixture** — a second, independent implementation of _one_
   relation, as a test fixture rather than a shipped strategy, registered under
   its own user-range id so it never enters an operator's acceptance set. This
   is not what the two reference strategies are: they establish two _different_
   relations, so verifying one attestation with each is two strategies
   disagreeing about different facts, which is expected. Without this fixture no
   phase produces the pair determinism needs, and the one property rated high
   for forking acceptance has no carrier anywhere.

## Acceptance Criteria

- [x] Step 0b runs on both branches — a forged macaroon on an attested token
      rejects
- [x] Step 5 rejects when the capability form carries no bindable value, with
      its own visible reason rather than a panic naming the strategy
- [x] The strategy call is wrapped in `catch_unwind` inside `AssertUnwindSafe`
- [x] A panic becomes a rejection, not a propagated fault
- [x] Boot refuses an unresolvable strategy id, a missing payload kind, an
      unmeasured cost, and an exceeded budget — each naming what was missing
- [x] The producing-side branch does not consume the verification-side gate
- [x] The registration host is a binary or node crate, not core
- [x] Strategy crates are linked from the host
- [ ] `in_boundary_execution` is registered with the profile measured in this
      phase, and `root_secret_knowledge` implements `AttestationAdmission`
      answering `CostFigure::Unmeasured` rather than `None`
- [x] `root_secret_knowledge` ships unregistered
- [x] Neither reference strategy declares payload kinds
- [x] A second implementation of one relation exists as a fixture
- [ ] A node accepting no attestations **starts and advertises an empty
      acceptance set**. Refusing to start is not the behaviour: running a node
      that verifies nothing is a permitted operator choice, and the mitigation
      is that it is visible rather than silent, not that it is impossible
- [x] Back-compat corpus re-verified through the dispatch, not just as bytes
- [x] The dispatch is reachable from the wallet's verification path, not only
      from its own tests: `VerifyContext` gains the registry and acceptance-set
      slots and `VerifyError` gains the attestation reason, so an attested token
      presented to the production entry point goes through the dispatch
- [x] The operator policy, its acceptance-set type, and the `PolicySource`
      implementation the dispatch calls are all owned by this mission

### Type Coverage

| RFC item                                                          | Implemented by                  |
| ----------------------------------------------------------------- | ------------------------------- |
| `verify_capability_attestation`                                   | This mission                    |
| `validate_policy`                                                 | This mission                    |
| `register_strategy` execution                                     | This mission (declared in base) |
| `octo-cap-zk` reference strategy (extends the **existing** crate) | This mission                    |
| `octo-cap-poi` reference strategy (new crate)                     | This mission                    |
| Operator policy type and its `PolicySource` implementation        | This mission                    |
| Acceptance-set type read by step 2 and by boot validation         | This mission                    |
| `Admission` value construction for a strategy without the trait   | This mission                    |
| Registration host and strategy links                              | This mission                    |
| Determinism fixture (second implementation of one relation)       | This mission                    |
| `in_boundary_execution` measured profile                          | This mission                    |
| Back-compat corpus re-verification                                | This mission                    |
| Node-metadata advertisement                                       | Mission D                       |
| Dependency-direction lint                                         | Mission D                       |

### Test Vectors

This mission closes: **TV1**, **TV1b**, **TV2**, **TV3**, **TV4**, **TV6**,
**TV6b** (dispatch side), **TV7**, **TV7b**, **TV7c**, **TV7d**, **TV7g**,
**TV7h**, **TV8**, **TV10**, **TV13**, **TV13b**, **TV16**.

## Layer direction

- `octo-wallet` — Layer B years-stable substrate. The dispatch itself is new code
  in `octo-wallet::capability`, but landing it in the real verification path
  does modify two existing types: `VerifyContext` gains the registry and
  acceptance-set slots and `VerifyError` gains the attestation reason. Both are
  additive fields and variants, so the change is backward compatible.
- `octo-wallet-node` — Layer D deployment crate. It composes the node and links
  strategy crates; it is the registration host.
- `octo-cap-zk`, `octo-cap-poi`, and the fixture — Layer E user extensions.
  Core reaches them through the registry and through registration only, never
  by naming them.

The dependency direction is the point of the host. A Layer-B crate that linked a
Layer-E crate would invert the model the layer table forbids, and mission D
enforces that as a lint.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-wallet -p octo-wallet-node -p octo-cap-zk -p octo-cap-poi --all-targets --all-features -- -D warnings
cargo test -p octo-wallet --lib capability
# `dispatch_tv` is RFC-0969's gateway dispatch, a different artifact.
# The RFC-0973 dispatch vectors live here.
cargo test -p octo-wallet --test attestation_dispatch
```

The dispatch is Class A under the execution-class mapping, so `verify` decides
acceptance of a monetary claim and must be protocol-deterministic. TV16 is the
evidence, and it is the reason the fixture exists.

## Backward compatibility

Every capability minted before the envelope lands must still verify on the
macaroon path. The frozen corpus from the base mission is re-run here rather
than assumed, because this is the phase where the dispatch first meets real
traffic.

## Cross-references

- RFC-0973 §Algorithms — the dispatch, boot validation, and registration
- RFC-0973 §Error Handling — one rejection row per gate, each distinguishable
- RFC-0973 §Lifecycle Requirements — requirement 1 (registration), requirement
  2 (boot validation), requirement 3 (reject never downgrade), requirement 8
  (registration host)
- RFC-0973 §Determinism Requirements — what Class A means for `verify`
- RFC-0973 §RFC-0008 Execution Class Mapping
- RFC-0973 §Adversarial Analysis — A4 (panic), A5 (determinism), A9 and A13
  (attestation is not authorisation), A15 (verifier clock)
- Mission `0973-v36-attestation-core-substrate` — substrate this builds on
- Mission `0973-v36-b-root-secret-binding` — unblocks `root_secret_knowledge`
- Mission `0973-v36-d-dependency-direction` — lint and advertisement

## Claimant

@mmacedoeu (agent, 2026-10-07) — autonomous implementation, PR(s) for human review per BLUEPRINT.md Agent Workflow.

## Pull Request

#

## Implementation notes (Phase 2)

Landed: the dispatch, boot validation, the operator policy and acceptance-set
types, the production wiring, both strategy crates, the registration host, the
TV16 determinism fixture, and the back-compat corpus re-verification.

**The registry was changed to store the full `Registration`, not a bare
strategy.** Phase 1 registered `Arc<dyn AttestationStrategy>`, which left the
dispatch reconstructing a `Registration` with `admission: None` at step 4. That
would have made the coverage gate read the permissive value for every strategy
and therefore never fire — a vacuous check of exactly the kind this RFC is
about. `resolve_registration` is now the accessor step 4 uses, and
`register_strategy` takes the admission side as a second argument.

**Step 0 and step 0b run before the dispatch body, inside
`verify_with_resolve`.** The registry lookup, the active check, and the
holder-signature verify are the same work the dispatch's step 0 and 0b call for,
so they are performed once in the production entry point and passed to the
dispatch as the results they already are. The gate order is preserved without
the macaroon checks being skippable or run twice.

**No wire fixture was needed for the production-path proof.** A test that
built a full wire token would prove the fixture, not the wiring. The integration
test drives `verify_attestation_for_verified_token` on a real `VerifyContext`
and asserts the refusals, which is the property in question.

**Boot's producing-side gate is asserted not to consume the verification-side
gate.** A strategy that produces nothing but needs a producing node to verify
boots successfully and is refused later, at step 4. Collapsing the two would
make step 4 unreachable.

### Declared where the traits live (deviation, recorded)

The RFC places the attestation traits and the registry in
`octo-wallet::capability`. A Layer-E strategy crate has to _implement_ those
traits, so declaring them in the wallet means every strategy crate depends on
the wallet — while the wallet depends on `octo-cap-zk` for its ZK mint
substrate. That closes a dependency cycle, and mission D removes the wallet's
dependency on `octo-cap-zk` only in **Phase 5**, two phases after this one.

**The stated phase order is therefore not executable as written.** The strategy
crates this mission adds cannot be added before the removal Phase 5 performs.

Rather than reorder the missions or leave the strategy crates until last, the
traits are declared in `octo-cap-macaroon`, alongside `AttestationTypeId` which
the RFC already puts there, and re-exported from
`octo_wallet::capability::attestation` unchanged. Both import paths resolve to
the same trait objects, so:

- the RFC's stated import path still works and is what core uses;
- `octo-cap-zk` depends on `octo-cap-macaroon` alone, with no cycle and no
  dependency on the wallet;
- mission D's removal is still Phase 5's job, on schedule;
- `octo-cap-macaroon` gains a dependency on `octo-protocol` for
  `PayloadKindId`, which is a Layer-A edge and already present transitively.

Mission D's dependency-removal work is unaffected and still outstanding.

### `in_boundary_execution` is NOT registered — the profile cannot be measured

The RFC makes registration conditional on two preconditions, "the profile
measured in this phase, and the relation verified against the trace the boundary
actually produces". **Neither holds, and the first one cannot be discharged by
any amount of work in this phase.**

The relation is RFC-0630's proof-of-inference trace. Folding that trace to a
commitment is a Poseidon fold over felt252, performed in Cairo by
`verify_inference_fold_flat`. There is **no host-side Rust implementation**:
`derive_trace_hash` and `derive_step_hash` are named in `octo-cap-zk`'s
documentation but were never written — mission 0958-a AC-6 records this as a
deferred disclosure, deferred to 0958-b. The STARK prover is likewise not
reachable: `prove_batch_signature` returns `ProverError::Internal("full path
unimplemented")`.

So there is no trace to verify against and no program on which to measure a
CASM figure. Writing a host-side Poseidon that must agree bit-for-bit with
corelib Poseidon across every node in the fleet is not something to improvise
while landing a dispatch, and inventing a cheaper relation to have something to
measure would be worse: it would register a strategy that establishes a
different fact from the one the RFC names.

The strategy therefore ships **unregistered**, and answers
`CostFigure::Unmeasured` — never `None`. It is a proof-bearing crate, so `None`
would assert it carries no circuit cost and would skip both budget gates. An
RFC-range id with no registry entry, named in an acceptance set, refuses boot as
unresolvable. That is the fail-closed behaviour the RFC provides for exactly
this case, not a hole in it.

**The mission text said `octo-cap-poi` "either omits the trait or answers
`None`; both are truthful". That is wrong** and was corrected during
implementation. The truthfulness of `None` rests on the crate carrying no
proof, and this crate carries one. The RFC's own §Reference Strategies states
the circuit cost is "unmeasured — Phase 2", and §Performance Targets names
"unmeasured" as the correct entry until a figure is measured on the real
program.

### The determinism fixture found two real bugs

TV16 requires two _independently built_ implementations of **one** relation. The
fixture re-derives `root_secret_knowledge`'s relation from the specification
rather than delegating to it, so agreement means something. Writing it caught
two defects that no single-implementation test could have found:

1. **The fixture invented a domain separator.** RFC-0973 §Root-Secret Binding
   states the relation as `BLAKE3(s) == macaroon.root_secret_hash` and says
   explicitly that the binding "needs no new commitment and no second hash
   primitive". A separator in the fixture made it a _different_ relation, and the
   two disagreed on a witness that was in fact valid. Removed.

2. **`root_secret_knowledge` never consulted `PolicySource`.** The dispatch
   already gates the acceptance set at step 2, so this was not a hole in the
   dispatch — but it made the two reference strategies behave differently under
   the same context, which is precisely the divergence TV16 exists to detect. It
   surfaced as a test failure rather than as a code review finding. The strategy
   now consults policy and may only ever refuse, matching
   `in_boundary_execution`.

Both are regressions the fixture now prevents.

### The fixture lives in the wallet's test surface, not in a strategy crate

It depends on `octo-cap-zk`, which the wallet's library must not depend on —
that edge is exactly what mission D removes from the library, and re-adding it
as a normal dependency to host a test fixture would undo Phase 5's work. It is a
`dev-dependency` instead, which is the one Layer-E-to-Layer-B-by-test direction
mission D's lint must permit.

The fixture registers under a **user-range id**, so it can never be configured
into an operator's acceptance set by naming an RFC-allocated id.

### Back-compat corpus re-verified as behaviour, not bytes

The base mission froze the corpus and pinned its canonical bytes. Re-running
those byte assertions here would prove nothing new — this phase touches
`Macaroon`, so what needed re-verifying is that a pre-RFC capability still
**verifies**, end to end, through the new dispatch. The corpus is run across
three context shapes, including one where the operator's acceptance set names an
unrelated strategy: the shape an operator actually runs. A dispatch that
consulted the acceptance set on the unattested branch would refuse these, and a
byte-only test would pass regardless.

### Still open in this mission

- `in_boundary_execution`'s measured profile and its registration. Both are
  blocked on a host-side PoI fold and a reachable prover, neither of which this
  phase can produce. The RFC assigns the measurement to Phase 2; it cannot be
  discharged until mission 0973-v36-c builds the real prover path.
