//! RFC-0973 §Reference Strategies — the `in_boundary_execution` strategy.
//!
//! # The relation
//!
//! "The attested inference ran inside the protocol boundary", sourced from
//! RFC-0630 Proof-of-Inference and carried here as a *commitment to the PoI
//! execution trace, bound to the capability*.
//!
//! RFC-0973 does not re-implement RFC-0630. RFC-0630 supplies the fact; this
//! strategy adapts it to the attestation interface. What this crate therefore
//! checks is the **binding**: that the PoI trace commitment a holder presents
//! is bound to *this* capability's root secret, and not transplanted onto
//! another one.
//!
//! # What this crate does NOT do, and why
//!
//! **It does not fold the trace.** The PoI trace is folded to a commitment by
//! Poseidon over felt252, in Cairo (`verify_inference_fold_flat`). There is no
//! host-side Rust mirror of that fold: `derive_trace_hash` and `derive_step_hash`
//! are named in `octo-cap-zk`'s documentation but were never implemented
//! (mission 0958-a AC-6 records this as a deferred disclosure, deferred to
//! 0958-b). A host-side reimplementation of a Poseidon fold that must agree
//! bit-for-bit with corelib Poseidon across every node in the fleet is not a
//! thing to improvise while landing a dispatch.
//!
//! **It does not verify a STARK.** That is `zk-verifier`'s job, and the real
//! prover path returns `ProverError::Internal("full path unimplemented")`.
//!
//! **This ships UNREGISTERED.** RFC-0973 makes registration conditional on two
//! preconditions — the profile measured, and the relation verified against the
//! trace the boundary actually produces. Neither holds: the fold has no host
//! side, and no production path emits a trace to check against. Registering now
//! would advertise a relation whose second half no node can evaluate, and every
//! attestation presented against it would be refused for a reason that reads as
//! a bug rather than a missing phase. RFC-0973 explicitly provides for this —
//! an unregistered id named in an acceptance set fails boot as unresolvable,
//! which is the fail-closed behaviour, not a hole in it.
//!
//! # Why `Unmeasured` and not `None`
//!
//! This crate carries a proof. Per RFC-0973 §Attestation Traits, answering
//! `None` from `admission_profile` means "this strategy carries no circuit
//! cost", which would be a false claim about a proof-bearing crate and would
//! skip both boot budget gates. `Unmeasured` keeps the gate decisive on an
//! unknown cost, which is the honest answer until the figure is measured on the
//! real program — RFC-0973 §Performance Targets names "unmeasured" as the
//! correct entry until then, and assigns the measurement to this phase.

use octo_cap_macaroon::attestation::{
    AdmissionProfile, AttestationAdmission, AttestationError, AttestationOutcome,
    AttestationStrategy, AttestationTypeId, AttestationVerifyCtx, CostFigure, Expectation,
    PolicySource, SecurityFigure, SecurityParams,
};

/// The RFC-0973-allocated id for the in-boundary-execution relation.
///
/// RFC range, distinct trailing byte from `root_secret_knowledge`.
pub const IN_BOUNDARY_EXECUTION: AttestationTypeId = AttestationTypeId::from_bytes([
    0x09, 0x73, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
]);

/// The crate identity the registration host supplies for ownership checks.
///
/// The allocation record compares against the claimant the **host observes at
/// link time**, never anything a strategy reports about itself. This constant
/// is what `octo-wallet-node` passes as that claimant; it is not a
/// self-assertion and the registry does not read it from the strategy.
pub const CLAIMANT: &str = "octo-cap-poi";

/// Domain separator for this relation's binding commitment.
///
/// Domain separation is what keeps this commitment from ever colliding with the
/// root-secret relation's digest, so a witness valid under one strategy can
/// never be replayed as valid under the other.
pub const IN_BOUNDARY_DOMAIN: &[u8] = b"cipherocto/rfc-0973/in-boundary-execution/v1";

/// A PoI trace commitment bound to a capability.
///
/// Fixed-length and self-describing enough to reject a malformed payload
/// without allocating from it: the node reads the header, learns the declared
/// length, and refuses anything that does not fit exactly.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PoiBinding {
    /// Provider slot the inference ran on (RFC-0009 §Vault slot id).
    pub provider_slot: [u8; 16],
    /// Model digest the inference ran.
    pub model_id: [u8; 32],
    /// The PoI execution-trace commitment, as folded by RFC-0630.
    pub trace_root: [u8; 32],
    /// Binding of the fields above to a capability's root secret.
    pub commitment: [u8; 32],
}

impl PoiBinding {
    /// Bytes of packed binding, excluding the commitment itself.
    const fn packed_len() -> usize {
        16 + 32 + 32
    }

    /// Total payload length, commitment included.
    const PAYLOAD_LEN: usize = Self::packed_len() + 32;

    fn pack_into(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.provider_slot);
        buf.extend_from_slice(&self.model_id);
        buf.extend_from_slice(&self.trace_root);
    }

    /// Parse a binding from an envelope payload.
    ///
    /// Returns `None` for anything that is not exactly one well-formed binding.
    /// A trailing byte is a malformed payload rather than something ignored, so
    /// a commitment cannot be smuggled past the length check with extra data.
    #[must_use]
    pub fn from_payload(payload: &[u8]) -> Option<Self> {
        if payload.len() != Self::PAYLOAD_LEN {
            return None;
        }
        let mut provider_slot = [0u8; 16];
        provider_slot.copy_from_slice(&payload[..16]);
        let mut model_id = [0u8; 32];
        model_id.copy_from_slice(&payload[16..48]);
        let mut trace_root = [0u8; 32];
        trace_root.copy_from_slice(&payload[48..80]);
        let mut commitment = [0u8; 32];
        commitment.copy_from_slice(&payload[80..Self::PAYLOAD_LEN]);
        Some(Self {
            provider_slot,
            model_id,
            trace_root,
            commitment,
        })
    }

    /// The commitment these fields produce against a binding target.
    ///
    /// Exposed so a holder-side producer and the determinism fixture can
    /// recompute it rather than restating the layout.
    #[must_use]
    pub fn commit_against(&self, binding_target: &[u8]) -> [u8; 32] {
        let mut buf = Vec::with_capacity(IN_BOUNDARY_DOMAIN.len() + 32 + Self::packed_len());
        buf.extend_from_slice(IN_BOUNDARY_DOMAIN);
        buf.extend_from_slice(binding_target);
        self.pack_into(&mut buf);
        octo_cap_macaroon::blake3_hash(&buf)
    }

    /// Encode this binding as an envelope payload.
    #[must_use]
    pub fn to_payload(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(Self::PAYLOAD_LEN);
        self.pack_into(&mut out);
        out.extend_from_slice(&self.commitment);
        out
    }

    /// Build a binding that is correctly bound to `binding_target`.
    #[must_use]
    pub fn bound_to(
        provider_slot: [u8; 16],
        model_id: [u8; 32],
        trace_root: [u8; 32],
        binding_target: &[u8],
    ) -> Self {
        let unbound = Self {
            provider_slot,
            model_id,
            trace_root,
            commitment: [0u8; 32],
        };
        Self {
            commitment: unbound.commit_against(binding_target),
            ..unbound
        }
    }
}

/// The in-boundary-execution relation.
#[derive(Clone, Copy, Debug, Default)]
pub struct InBoundaryExecution;

impl AttestationStrategy for InBoundaryExecution {
    fn type_id(&self) -> AttestationTypeId {
        IN_BOUNDARY_EXECUTION
    }

    fn verify(
        &self,
        ctx: &AttestationVerifyCtx<'_>,
    ) -> Result<AttestationOutcome, AttestationError> {
        // Operator policy first. Refusal is a verdict; it is never upgraded into
        // acceptance, and the policy may only ever cause a refusal.
        if !ctx.policy.accepts(&self.type_id()) {
            return Err(AttestationError::StrategyRefused {
                reason: "operator policy does not accept this attestation".to_owned(),
            });
        }
        ctx.policy.validate(ctx)?;

        // The payload must be a well-formed PoI binding. A malformed payload is
        // an unsatisfied relation, not a malformed context: the context was
        // fine, the claim in it was not.
        let Some(binding) = PoiBinding::from_payload(&ctx.envelope.payload) else {
            return Ok(AttestationOutcome::Unsatisfied {
                reason: "payload is not a PoI trace binding".to_owned(),
            });
        };

        // The binding target is the full 32-byte root_secret_hash, never the
        // capability id: the id hashes the canonical form the envelope sits
        // inside, so binding to it would make the check a function of this very
        // attestation.
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

        if binding.commit_against(&declared) == binding.commitment {
            Ok(AttestationOutcome::Satisfied)
        } else {
            // The reason names the condition. It never carries the commitment
            // bytes, the trace root, or the binding target.
            Ok(AttestationOutcome::Unsatisfied {
                reason: "PoI trace commitment is not bound to this capability".to_owned(),
            })
        }
    }
}

impl AttestationAdmission for InBoundaryExecution {
    /// Carries a proof, so both budget gates must run. `Unmeasured` is decisive:
    /// a node naming this strategy refuses to start rather than admitting it on
    /// a guess. It is NOT `None`, which would assert the crate carries no
    /// circuit cost and would skip both gates entirely.
    fn admission_profile(&self) -> Option<AdmissionProfile> {
        Some(AdmissionProfile {
            casm_cost_hint_words: CostFigure::Unmeasured,
            // A **requirement**, not a claim about what this crate produces.
            // RFC-0958 §Design Goals G4 asks for 128-bit soundness, so 128 is
            // what the relation declares it needs. Whether the deployment
            // actually delivers it is a separate fact the operator supplies as
            // a `ProverProfile`, and boot refuses when the two disagree.
            //
            // This crate used to declare 128 as though it were an achieved
            // level. It cannot know that: it does not own the prover, and the
            // configured parameters yield 96. The declaration that survives
            // audit is the requirement, so that is what it carries.
            security_bits: SecurityFigure::from_prover_parameters(SecurityParams {
                min_bits: 128,
                num_queries: 102,
            }),
        })
    }
}

/// An expectation bound to a capability's `root_secret_hash`.
///
/// Provided so a host or a fixture can build one without re-deriving it. The
/// wallet's verification path has its own, which reads the same field.
#[derive(Clone, Copy, Debug, Default)]
pub struct RootSecretExpectation;

impl Expectation for RootSecretExpectation {
    fn bindable_value(&self, ctx: &AttestationVerifyCtx<'_>) -> Option<Vec<u8>> {
        Some(ctx.capability.root_secret_hash.to_vec())
    }
}

/// A policy that accepts everything. Provided for hosts with no operator policy
/// configured beyond the acceptance set.
#[derive(Clone, Copy, Debug, Default)]
pub struct AllowAll;

impl PolicySource for AllowAll {
    fn accepts(&self, _type_id: &AttestationTypeId) -> bool {
        true
    }
    fn validate(&self, _ctx: &AttestationVerifyCtx<'_>) -> Result<(), AttestationError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_cap_macaroon::attestation::{
        admission_of, Admission, AllocationRecord, AttestationEnvelope, AttestationRegistry,
        RegisterError, Registration,
    };
    use octo_cap_macaroon::macaroon::Macaroon;
    use std::sync::Arc;

    /// A policy that refuses every type, for the operator-refusal path.
    struct RefuseAll;

    impl PolicySource for RefuseAll {
        fn accepts(&self, _type_id: &AttestationTypeId) -> bool {
            false
        }
        fn validate(&self, _ctx: &AttestationVerifyCtx<'_>) -> Result<(), AttestationError> {
            Ok(())
        }
    }

    fn mac() -> Macaroon {
        Macaroon::mint(&[77u8; 32]).unwrap()
    }

    fn binding_for(mac: &Macaroon) -> PoiBinding {
        PoiBinding::bound_to([3u8; 16], [5u8; 32], [9u8; 32], &mac.root_secret_hash)
    }

    fn check(payload: Vec<u8>, mac: &Macaroon) -> AttestationOutcome {
        check_with_policy(payload, mac, &AllowAll)
    }

    fn check_with_policy(
        payload: Vec<u8>,
        mac: &Macaroon,
        policy: &dyn PolicySource,
    ) -> AttestationOutcome {
        let env = AttestationEnvelope::new(IN_BOUNDARY_EXECUTION, payload);
        let expect = RootSecretExpectation;
        let ctx = AttestationVerifyCtx {
            capability: mac,
            envelope: &env,
            expectation: &expect,
            policy,
        };
        InBoundaryExecution.verify(&ctx).unwrap()
    }

    #[test]
    fn a_correctly_bound_trace_satisfies_the_relation() {
        let m = mac();
        let b = binding_for(&m);
        assert!(check(b.to_payload(), &m).is_satisfied());
    }

    /// The transplant case: a binding valid for one capability presented
    /// against another must not verify. This is the whole point of binding to
    /// the root secret rather than to the capability id.
    #[test]
    fn a_binding_for_another_capability_does_not() {
        let a = mac();
        let b = Macaroon::mint(&[78u8; 32]).unwrap();
        assert!(!check(binding_for(&a).to_payload(), &b).is_satisfied());
    }

    /// Changing any bound field must break the binding.
    #[test]
    fn changing_any_bound_field_breaks_the_commitment() {
        let m = mac();
        let base = binding_for(&m);
        assert!(check(base.to_payload(), &m).is_satisfied());

        let original = base;
        let variants = [
            PoiBinding {
                provider_slot: [1u8; 16],
                ..original
            },
            PoiBinding {
                model_id: [1u8; 32],
                ..original
            },
            PoiBinding {
                trace_root: [1u8; 32],
                ..original
            },
        ];
        for v in variants {
            assert!(
                !check(v.to_payload(), &m).is_satisfied(),
                "a changed field must not still satisfy"
            );
        }
    }

    /// Domain separation: the same material must not produce the same digest
    /// under the root-secret relation, or a witness could be replayed across
    /// relations.
    #[test]
    fn the_domain_separator_differs_from_the_root_secret_relation() {
        let m = mac();
        let b = binding_for(&m);
        assert_ne!(
            b.commitment,
            octo_cap_macaroon::blake3_hash(&m.root_secret_hash)
        );
    }

    #[test]
    fn a_malformed_payload_is_rejected() {
        let m = mac();
        assert!(!check(Vec::new(), &m).is_satisfied());
        assert!(!check(vec![0u8; 10], &m).is_satisfied());
        // One byte short and one byte long.
        let p = binding_for(&m).to_payload();
        let mut short = p.clone();
        short.pop();
        assert!(!check(short, &m).is_satisfied());
        let mut long = p;
        long.push(0);
        assert!(!check(long, &m).is_satisfied());
    }

    /// An operator policy refusal must be an error, never an outcome a caller
    /// could mistake for a verdict about the relation.
    #[test]
    fn a_policy_refusal_is_an_error_not_an_outcome() {
        let m = mac();
        let env = AttestationEnvelope::new(IN_BOUNDARY_EXECUTION, binding_for(&m).to_payload());
        let expect = RootSecretExpectation;
        let ctx = AttestationVerifyCtx {
            capability: &m,
            envelope: &env,
            expectation: &expect,
            policy: &RefuseAll,
        };
        match InBoundaryExecution.verify(&ctx) {
            Err(AttestationError::StrategyRefused { reason }) => assert!(!reason.is_empty()),
            other => panic!("expected a policy refusal, got {other:?}"),
        }
    }

    /// The reason must never carry the commitment, the trace root, or the
    /// binding target. The "safe to log" property is a promise the crate author
    /// makes; no gate inspects the reason.
    #[test]
    fn the_reason_carries_no_attestation_bytes() {
        let a = mac();
        let b = Macaroon::mint(&[78u8; 32]).unwrap();
        let binding = binding_for(&a);
        let AttestationOutcome::Unsatisfied { reason } = check(binding.to_payload(), &b) else {
            panic!("expected a rejection");
        };
        let hex: String = reason.bytes().map(|x| format!("{x:02x}")).collect();
        for secret in [
            &binding.commitment,
            &binding.trace_root,
            &a.root_secret_hash,
        ] {
            let want: String = secret.iter().map(|x| format!("{x:02x}")).collect();
            assert!(!hex.contains(&want), "reason leaked attestation bytes");
        }
    }

    /// The profile must be `Unmeasured`, never `None`. `None` asserts no
    /// circuit cost, which is false of a proof-bearing crate, and would skip
    /// both boot budget gates.
    #[test]
    fn the_profile_is_unmeasured_not_none() {
        let profile = InBoundaryExecution
            .admission_profile()
            .expect("a proof-bearing strategy must answer Some; None would skip both budget gates");
        assert_eq!(profile.casm_cost_hint_words, CostFigure::Unmeasured);
        assert_eq!(profile.security_bits.bits(), 128);
    }

    /// This strategy declares no payload kinds. That is a decision recorded in
    /// RFC-0973 §Reference Strategies, not an omission.
    #[test]
    fn it_declares_no_payload_kinds() {
        assert!(InBoundaryExecution.required_payload_kinds().is_empty());
        assert!(!InBoundaryExecution.verifier_needs_producing_kinds());
    }

    /// Ownership binding is non-vacuous: an RFC-range id allocated to another
    /// crate must be refused, and a second claim on a taken id never replaces
    /// the incumbent.
    #[test]
    fn registration_enforces_ownership() {
        let mut allocations = AllocationRecord::empty();
        allocations.allocate(IN_BOUNDARY_EXECUTION, "some-other-crate");
        let mut registry = AttestationRegistry::empty();
        let err = registry
            .register_strategy(Arc::new(InBoundaryExecution), None, CLAIMANT, &allocations)
            .unwrap_err();
        assert_eq!(
            err,
            RegisterError::WrongOwner {
                type_id: IN_BOUNDARY_EXECUTION,
                allocated_to: "some-other-crate".to_owned(),
                claimant: CLAIMANT.to_owned(),
            }
        );

        // Correctly allocated, the host's own claim succeeds.
        let mut allocations = AllocationRecord::empty();
        allocations.allocate(IN_BOUNDARY_EXECUTION, CLAIMANT);
        let mut registry = AttestationRegistry::empty();
        registry
            .register_strategy(Arc::new(InBoundaryExecution), None, CLAIMANT, &allocations)
            .unwrap();
        // A second claim on the same id is refused rather than replacing it.
        let again =
            registry.register_strategy(Arc::new(InBoundaryExecution), None, CLAIMANT, &allocations);
        assert!(matches!(again, Err(RegisterError::AlreadyRegistered(_))));
    }

    /// What a boot gate reads for this strategy. It must NOT be the permissive
    /// value: permissive means Measured(0), a claim of "no circuit cost".
    #[test]
    fn a_full_registration_is_not_read_as_permissive() {
        let registration = Registration {
            strategy: Arc::new(InBoundaryExecution),
            admission: Some(Arc::new(InBoundaryExecution)),
        };
        let admission = admission_of(&registration);
        assert_ne!(admission, Admission::permissive());
        assert_eq!(
            admission.profile.casm_cost_hint_words,
            CostFigure::Unmeasured
        );
    }

    #[test]
    fn the_id_is_in_the_rfc_range_and_distinct_from_root_secret() {
        assert!(IN_BOUNDARY_EXECUTION.is_rfc_range());
        assert_ne!(IN_BOUNDARY_EXECUTION.as_bytes()[15], 0x01);
    }
}
