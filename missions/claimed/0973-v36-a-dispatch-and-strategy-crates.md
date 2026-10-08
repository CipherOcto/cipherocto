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
   gate decisive on an unknown cost. `octo-cap-poi` carries no proof, so it
   either omits the trait or answers `None`; both are truthful and equivalent.

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

- [ ] Step 0b runs on both branches — a forged macaroon on an attested token
      rejects
- [ ] Step 5 rejects when the capability form carries no bindable value, with
      its own visible reason rather than a panic naming the strategy
- [ ] The strategy call is wrapped in `catch_unwind` inside `AssertUnwindSafe`
- [ ] A panic becomes a rejection, not a propagated fault
- [ ] Boot refuses an unresolvable strategy id, a missing payload kind, an
      unmeasured cost, and an exceeded budget — each naming what was missing
- [ ] The producing-side branch does not consume the verification-side gate
- [ ] The registration host is a binary or node crate, not core
- [ ] Strategy crates are linked from the host
- [ ] `in_boundary_execution` is registered with the profile measured in this
      phase, and `root_secret_knowledge` implements `AttestationAdmission`
      answering `CostFigure::Unmeasured` rather than `None`
- [ ] `root_secret_knowledge` ships unregistered
- [ ] Neither reference strategy declares payload kinds
- [ ] A second implementation of one relation exists as a fixture
- [ ] A node accepting no attestations **starts and advertises an empty
      acceptance set**. Refusing to start is not the behaviour: running a node
      that verifies nothing is a permitted operator choice, and the mitigation
      is that it is visible rather than silent, not that it is impossible
- [ ] The dispatch is reachable from the wallet's verification path, not only
      from its own tests: `VerifyContext` gains the registry and acceptance-set
      slots and `VerifyError` gains the attestation reason, so an attested token
      presented to the production entry point goes through the dispatch
- [ ] The operator policy, its acceptance-set type, and the `PolicySource`
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
cargo test -p octo-wallet --test dispatch_tv
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
