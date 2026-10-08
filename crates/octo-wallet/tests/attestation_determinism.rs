//! RFC-0973 §Phase 2 — TV16, the determinism fixture.
//!
//! # What this is
//!
//! A **second, independent implementation of one relation**. TV16 requires the
//! same capability, attestation, and context verified by *two independently
//! built strategy implementations* to yield the same verdict. A single
//! implementation cannot satisfy that, and neither can two implementations of
//! two *different* relations.
//!
//! The distinction is the whole point, and it is easy to get wrong: the two
//! reference strategies (`root_secret_knowledge` and `in_boundary_execution`)
//! establish two different facts. Verifying one attestation with each produces
//! two strategies disagreeing about different things, which is expected and
//! proves nothing about determinism. This fixture instead re-implements
//! `root_secret_knowledge`'s relation from the same specification, so a verdict
//! difference cannot be blamed on the relation.
//!
//! # Independent how
//!
//! Independence here means written from the specification rather than by calling
//! the strategy under test. This file does not invoke
//! `RootSecretKnowledge::verify`; it re-derives the relation itself. Sharing the
//! implementation would make agreement trivially true and TV16 vacuous.
//!
//! The relation is exactly what RFC-0973 §Root-Secret Binding states:
//! `BLAKE3(witness) == macaroon.root_secret_hash`. There is deliberately **no
//! domain separator** — the RFC calls out that this binding "needs no new
//! commitment and no second hash primitive". An earlier draft of this fixture
//! added one, which made it a *different* relation and disagreed with the
//! strategy on a witness that was in fact valid. That failure is the fixture
//! working: it caught a divergence that a single-implementation test cannot see.
//!
//! # The user-range id
//!
//! `FIXTURE_ROOT_SECRET` is in the user range. It is a fixture, not a shipped
//! strategy, so it must never enter an operator's acceptance set — an RFC-range
//! id would let it be configured into production by accident.
//!
//! ## Per `no-line-refs-anywhere` convention
//!
//! References use `§symbol-name` form.

use octo_cap_macaroon::attestation::AttestationEnvelope;
use octo_cap_macaroon::attestation::{
    AdmissionProfile, AttestationAdmission, AttestationError, AttestationOutcome,
    AttestationStrategy, AttestationTypeId, AttestationVerifyCtx, CostFigure, Expectation,
    PolicySource, SecurityFigure, SecurityParams,
};
use octo_cap_macaroon::macaroon::Macaroon;
use octo_cap_zk::attestation::{RootSecretKnowledge, ROOT_SECRET_KNOWLEDGE};

/// The fixture's own id, in the **user range**.
///
/// User range is the point: this is a test fixture, and an operator must not be
/// able to configure it into a production acceptance set by naming an
/// RFC-allocated id.
pub const FIXTURE_ROOT_SECRET: AttestationTypeId = AttestationTypeId::from_bytes([
    0x80, 0xF1, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x97,
]);

/// A second implementation of the root-secret-knowledge relation.
#[derive(Clone, Copy, Debug, Default)]
pub struct FixtureRootSecret;

impl AttestationStrategy for FixtureRootSecret {
    fn type_id(&self) -> AttestationTypeId {
        FIXTURE_ROOT_SECRET
    }

    fn verify(
        &self,
        ctx: &AttestationVerifyCtx<'_>,
    ) -> Result<AttestationOutcome, AttestationError> {
        // Policy first, exactly as the strategy under test does. TV16 fixes the
        // context, so both implementations must consult it the same way or the
        // comparison is not the one the vector describes.
        if !ctx.policy.accepts(&self.type_id()) {
            return Err(AttestationError::StrategyRefused {
                reason: "operator policy does not accept this attestation".to_owned(),
            });
        }
        ctx.policy.validate(ctx)?;

        let Some(declared) = ctx.expectation.bindable_value(ctx) else {
            return Ok(AttestationOutcome::Unsatisfied {
                reason: "capability carries no bindable value".to_owned(),
            });
        };
        if declared.len() != 32 {
            return Ok(AttestationOutcome::Unsatisfied {
                reason: "declared binding target is not 32 bytes".to_owned(),
            });
        }

        // Re-derived from the specification rather than delegated: the relation
        // is BLAKE3(witness) == binding target. No domain separator — RFC-0973
        // states this binding needs no new commitment and no second hash
        // primitive.
        if octo_cap_macaroon::blake3_hash(&ctx.envelope.payload) == declared[..] {
            Ok(AttestationOutcome::Satisfied)
        } else {
            // Never carries the witness or the hash: the reason is a promise the
            // crate author makes, and no gate inspects it.
            Ok(AttestationOutcome::Unsatisfied {
                reason: "root secret hash does not match the witness".to_owned(),
            })
        }
    }
}

impl AttestationAdmission for FixtureRootSecret {
    /// Mirrors the strategy under test exactly. The profile is part of the
    /// context TV16 fixes, so a fixture answering something different would be
    /// comparing two different contexts.
    fn admission_profile(&self) -> Option<AdmissionProfile> {
        Some(AdmissionProfile {
            casm_cost_hint_words: CostFigure::Unmeasured,
            security_bits: SecurityFigure::from_prover_parameters(SecurityParams {
                min_bits: 128,
                num_queries: 102,
            }),
        })
    }
}

/// An expectation supplying a fixed binding target.
struct Fixed(Vec<u8>);

impl Expectation for Fixed {
    fn bindable_value(&self, _ctx: &AttestationVerifyCtx<'_>) -> Option<Vec<u8>> {
        Some(self.0.clone())
    }
}

/// A policy accepting everything, so both implementations see the same context.
struct Allow;

impl PolicySource for Allow {
    fn accepts(&self, _type_id: &AttestationTypeId) -> bool {
        true
    }
    fn validate(&self, _ctx: &AttestationVerifyCtx<'_>) -> Result<(), AttestationError> {
        Ok(())
    }
}

/// A policy refusing everything, for the context-agreement check.
struct Refuse;

impl PolicySource for Refuse {
    fn accepts(&self, _type_id: &AttestationTypeId) -> bool {
        false
    }
    fn validate(&self, _ctx: &AttestationVerifyCtx<'_>) -> Result<(), AttestationError> {
        Ok(())
    }
}

/// Verdict shape, so the two implementations are compared on the *verdict* and
/// not on whether their reason strings happen to match.
///
/// RFC-0973 requires the same **verdict**. Reason text is strategy-chosen and
/// carries no protocol meaning, so demanding string equality would be a
/// stronger claim than the vector makes — and one that would fail the moment a
/// strategy improved its diagnostics.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Satisfied,
    Unsatisfied,
    Refused,
}

impl From<Result<AttestationOutcome, AttestationError>> for Verdict {
    fn from(r: Result<AttestationOutcome, AttestationError>) -> Self {
        match r {
            Ok(AttestationOutcome::Satisfied) => Self::Satisfied,
            Ok(AttestationOutcome::Unsatisfied { .. }) => Self::Unsatisfied,
            Err(_) => Self::Refused,
        }
    }
}

fn macaroon() -> Macaroon {
    Macaroon::mint(&[31u8; 32]).unwrap()
}

/// Run the strategy under test and the fixture over the same three inputs.
fn both_verdicts(
    payload: Vec<u8>,
    target: [u8; 32],
    policy: &dyn PolicySource,
) -> (Verdict, Verdict) {
    let mac = macaroon();
    let expect = Fixed(target.to_vec());

    // The strategy under test, under its own RFC-allocated id.
    let strategy_env = AttestationEnvelope::new(ROOT_SECRET_KNOWLEDGE, payload.clone());
    let strategy_ctx = AttestationVerifyCtx {
        capability: &mac,
        envelope: &strategy_env,
        expectation: &expect,
        policy,
    };
    let strategy = Verdict::from(RootSecretKnowledge.verify(&strategy_ctx));

    // The fixture, under its own user-range id. Same capability, same
    // attestation bytes, same policy — the three terms TV16 fixes.
    let fixture_env = AttestationEnvelope::new(FIXTURE_ROOT_SECRET, payload);
    let fixture_ctx = AttestationVerifyCtx {
        capability: &mac,
        envelope: &fixture_env,
        expectation: &expect,
        policy,
    };
    let fixture = Verdict::from(FixtureRootSecret.verify(&fixture_ctx));

    (strategy, fixture)
}

#[test]
fn tv16_the_two_implementations_agree_on_a_satisfied_relation() {
    let secret = [42u8; 32];
    let target = *blake3::hash(&secret).as_bytes();
    let (strategy, fixture) = both_verdicts(secret.to_vec(), target, &Allow);
    assert_eq!(strategy, Verdict::Satisfied, "the strategy under test");
    assert_eq!(fixture, Verdict::Satisfied, "the fixture");
    assert_eq!(strategy, fixture, "TV16: two builds must agree");
}

#[test]
fn tv16_the_two_implementations_agree_on_an_unsatisfied_relation() {
    let target = *blake3::hash(&[42u8; 32]).as_bytes();
    let (strategy, fixture) = both_verdicts(vec![43u8; 32], target, &Allow);
    assert_eq!(strategy, Verdict::Unsatisfied);
    assert_eq!(fixture, Verdict::Unsatisfied);
    assert_eq!(strategy, fixture, "TV16: two builds must agree");
}

/// The context term is load-bearing. A strategy permitted to consult
/// `PolicySource` may refuse on one node and accept on another; TV16 fixes the
/// context precisely so that agreement is not demanded across differing
/// contexts. Both implementations must see the same policy verdict.
#[test]
fn the_context_term_is_load_both_implementations_read_the_same_policy() {
    let secret = [42u8; 32];
    let target = *blake3::hash(&secret).as_bytes();

    // Same capability, same attestation, policy refusing: both must refuse.
    let (strategy, fixture) = both_verdicts(secret.to_vec(), target, &Refuse);
    assert_eq!(strategy, Verdict::Refused);
    assert_eq!(fixture, Verdict::Refused);
    assert_eq!(strategy, fixture, "a refusal must not become an acceptance");
}

/// A property TV16 does **not** cover, asserted so the vector is not
/// over-claimed: the two implementations have different type ids, so an
/// envelope carrying one id is not an attestation the other would accept. This
/// is why the fixture is never a substitute for the strategy in an acceptance
/// set.
#[test]
fn the_fixture_id_is_distinct_and_in_the_user_range() {
    assert_ne!(FIXTURE_ROOT_SECRET, ROOT_SECRET_KNOWLEDGE);
    assert!(
        !FIXTURE_ROOT_SECRET.is_rfc_range(),
        "a fixture must not be configurable as an RFC-allocated strategy"
    );
}

/// The fixture must not be mistaken for evidence that the strategy under test
/// is verified. Both ship unregistered, and the fixture's presence changes
/// nothing about either.
#[test]
fn the_fixture_claims_no_registry_entry() {
    // A fixture is a test artifact. Nothing here registers it, and the RFC's
    // registration path is exercised by the host instead.
    let registry = octo_cap_macaroon::attestation::AttestationRegistry::empty();
    assert!(registry.resolve(&FIXTURE_ROOT_SECRET).is_none());
    assert!(registry.resolve(&ROOT_SECRET_KNOWLEDGE).is_none());
}
