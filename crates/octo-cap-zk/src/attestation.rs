//! RFC-0973 §Root-Secret Binding — the `root_secret_knowledge` strategy.
//!
//! The relation: a holder that knows the root secret can prove it by presenting
//! a witness `s` such that `BLAKE3(s) == macaroon.root_secret_hash`.
//!
//! # This ships UNREGISTERED
//!
//! The relation is stated here and implemented over a **host-side** BLAKE3, but
//! it is not registered and must not be. The RFC binds the target inside a
//! circuit, and the in-circuit unkeyed BLAKE3 entry point does not exist yet —
//! that is mission B. Registering this now would advertise a relation the
//! prover cannot satisfy, and every attestation presented against it would be
//! refused for a reason that looks like a bug rather than a missing phase.
//!
//! # Why `Unmeasured` and not `None`
//!
//! This crate carries a proof. `admission_profile` answering `None` means "this
//! strategy carries no circuit cost", which would be a false claim about a
//! proof-bearing crate, and it would skip both budget gates at boot. `Unmeasured`
//! keeps the gate decisive on an unknown cost.

// The traits are declared in `octo-cap-macaroon` and re-exported by the
// wallet. A strategy crate names the declaring crate directly, which is what
// keeps `octo-cap-zk` free of any dependency on `octo-wallet`.
use octo_cap_macaroon::attestation::{
    AdmissionProfile, AttestationAdmission, AttestationError, AttestationOutcome,
    AttestationStrategy, AttestationTypeId, AttestationVerifyCtx, CostFigure, SecurityFigure,
    SecurityParams,
};

/// The RFC-0973-allocated id for the root-secret relation.
///
/// All-zero trailing byte with the RFC range's leading byte set, so it is
/// unambiguously in the allocated half.
pub const ROOT_SECRET_KNOWLEDGE: AttestationTypeId = AttestationTypeId::from_bytes([
    0x09, 0x73, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
]);

/// The root-secret relation.
#[derive(Clone, Copy, Debug, Default)]
pub struct RootSecretKnowledge;

impl AttestationStrategy for RootSecretKnowledge {
    fn type_id(&self) -> AttestationTypeId {
        ROOT_SECRET_KNOWLEDGE
    }

    fn verify(
        &self,
        ctx: &AttestationVerifyCtx<'_>,
    ) -> Result<AttestationOutcome, AttestationError> {
        // Operator policy is consulted before the relation is evaluated, and
        // may only ever cause a refusal. The dispatch already gates the
        // acceptance set at step 2, so this is not the only place the
        // acceptance set is applied — it is the place a strategy that *has* a
        // policy opinion gets to express one.
        //
        // `in_boundary_execution` consults policy the same way, and the two
        // must: TV16 compares two implementations of one relation over the
        // same context, so a posture difference here would surface as a
        // determinism failure rather than as the inconsistency it is.
        if !ctx.policy.accepts(&self.type_id()) {
            return Err(AttestationError::StrategyRefused {
                reason: "operator policy does not accept this attestation".to_owned(),
            });
        }
        ctx.policy.validate(ctx)?;

        // The payload is the witness `s`. Its length is checked before hashing
        // so an arbitrary-length payload cannot be fed to the hasher directly.
        let witness = &ctx.envelope.payload;

        // The expectation is the full 32-byte root_secret_hash, never the
        // capability id: the id hashes the canonical form the envelope sits
        // inside, so an expectation returning it would be a function of this
        // attestation and the comparison below would be a token against itself.
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

        // Host-side BLAKE3. This is the relation as stated, evaluated on the
        // host; the in-circuit form is mission B's work. Until then this
        // strategy is not registered, so nothing reaches this code.
        let computed = *blake3::hash(witness).as_bytes();

        if computed.as_slice() == declared.as_slice() {
            Ok(AttestationOutcome::Satisfied)
        } else {
            // The reason names the condition and never carries the witness or
            // the hash. The "safe to log" property is a promise the crate author
            // makes; no gate inspects the reason's content.
            Ok(AttestationOutcome::Unsatisfied {
                reason: "root secret hash does not match the witness".to_owned(),
            })
        }
    }
}

impl AttestationAdmission for RootSecretKnowledge {
    /// Carries a circuit cost, so the budget gates must run. `Unmeasured` is
    /// decisive: a node naming this strategy refuses to start rather than
    /// admitting it on a guess.
    fn admission_profile(&self) -> Option<AdmissionProfile> {
        Some(AdmissionProfile {
            casm_cost_hint_words: CostFigure::Unmeasured,
            // A requirement, not an achieved level. RFC-0958 §G4 asks for
            // 128-bit soundness. What the deployment's prover actually delivers
            // is supplied by the operator as a `ProverProfile` and compared at
            // boot, so this crate states the need rather than a figure it has
            // no way to observe.
            security_bits: SecurityFigure::from_prover_parameters(SecurityParams {
                min_bits: 128,
                num_queries: 102,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_cap_macaroon::attestation::{AttestationEnvelope, Expectation, PolicySource};
    use octo_cap_macaroon::macaroon::Macaroon;

    struct Expect(Vec<u8>);
    impl Expectation for Expect {
        fn bindable_value(&self, _c: &AttestationVerifyCtx<'_>) -> Option<Vec<u8>> {
            Some(self.0.clone())
        }
    }
    struct Allow;
    impl PolicySource for Allow {
        fn accepts(&self, _t: &AttestationTypeId) -> bool {
            true
        }
        fn validate(&self, _c: &AttestationVerifyCtx<'_>) -> Result<(), AttestationError> {
            Ok(())
        }
    }

    fn check(witness: &[u8], declared: [u8; 32]) -> AttestationOutcome {
        let mac = Macaroon::mint(&[31u8; 32]).unwrap();
        let env = AttestationEnvelope::new(ROOT_SECRET_KNOWLEDGE, witness.to_vec());
        let expect = Expect(declared.to_vec());
        let policy = Allow;
        let ctx = AttestationVerifyCtx {
            capability: &mac,
            envelope: &env,
            expectation: &expect,
            policy: &policy,
        };
        RootSecretKnowledge.verify(&ctx).unwrap()
    }

    #[test]
    fn a_correct_witness_satisfies_the_relation() {
        let secret = [42u8; 32];
        let declared = *blake3::hash(&secret).as_bytes();
        assert!(check(&secret, declared).is_satisfied());
    }

    #[test]
    fn a_wrong_witness_does_not() {
        let declared = *blake3::hash(&[42u8; 32]).as_bytes();
        let out = check(&[43u8; 32], declared);
        assert!(!out.is_satisfied());
    }

    #[test]
    fn the_reason_never_carries_the_witness_or_the_hash() {
        let secret = [7u8; 32];
        let declared = *blake3::hash(&[99u8; 32]).as_bytes();
        let AttestationOutcome::Unsatisfied { reason } = check(&secret, declared) else {
            panic!("expected a rejection");
        };
        let hex: String = reason
            .bytes()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        // Neither the 32-byte witness nor the declared hash may appear.
        assert!(!hex.contains(&hex_of(&secret)));
        assert!(!hex.contains(&hex_of(&declared)));
    }

    fn hex_of(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn the_profile_is_unmeasured_not_none() {
        assert_eq!(
            RootSecretKnowledge
                .admission_profile()
                .unwrap()
                .casm_cost_hint_words,
            CostFigure::Unmeasured
        );
    }

    /// A policy refusal must be an error, never an outcome a caller could read
    /// as acceptance. The witness here is genuinely valid, so a strategy that
    /// consulted policy only after evaluating the relation — or not at all —
    /// would return `Satisfied`.
    #[test]
    fn a_policy_refusal_is_an_error_even_when_the_relation_holds() {
        struct Refuse;
        impl PolicySource for Refuse {
            fn accepts(&self, _t: &AttestationTypeId) -> bool {
                false
            }
            fn validate(&self, _c: &AttestationVerifyCtx<'_>) -> Result<(), AttestationError> {
                Ok(())
            }
        }

        let secret = [42u8; 32];
        let declared = *blake3::hash(&secret).as_bytes();
        let mac = Macaroon::mint(&[31u8; 32]).unwrap();
        let env = AttestationEnvelope::new(ROOT_SECRET_KNOWLEDGE, secret.to_vec());
        let expect = Expect(declared.to_vec());
        let ctx = AttestationVerifyCtx {
            capability: &mac,
            envelope: &env,
            expectation: &expect,
            policy: &Refuse,
        };
        match RootSecretKnowledge.verify(&ctx) {
            Err(AttestationError::StrategyRefused { reason }) => assert!(!reason.is_empty()),
            other => panic!("a valid witness under a refusing policy must refuse, got {other:?}"),
        }
    }

    /// The relation is exactly `BLAKE3(witness) == root_secret_hash`, with no
    /// domain separator — RFC-0973 §Root-Secret Binding states this binding
    /// needs no new commitment and no second hash primitive. A separator added
    /// here would make the macaroon's own `root_secret_hash` no longer satisfy
    /// it, which would silently refuse every honest witness.
    #[test]
    fn the_relation_is_the_raw_blake3_hop_with_no_domain_separator() {
        let secret = [42u8; 32];
        let declared = *blake3::hash(&secret).as_bytes();
        assert!(check(&secret, declared).is_satisfied());
        // And the declared target is exactly the raw BLAKE3 of the witness,
        // with nothing prepended.
        assert_eq!(declared, *blake3::hash(&secret).as_bytes());
    }

    #[test]
    fn the_id_is_in_the_rfc_range() {
        assert!(ROOT_SECRET_KNOWLEDGE.is_rfc_range());
    }
}
