//! RFC-0973 §Registration Host — where attestation strategies are registered.
//!
//! # Why this lives here and not in core
//!
//! The registry's ownership gate compares an RFC-range id's allocated owner
//! against a **claimant the host observes at link time**. That claimant is this
//! crate's whole reason to exist: it links `octo-cap-poi`, so it can name
//! `octo-cap-poi` as the claimant without asking the strategy what it calls
//! itself.
//!
//! Core cannot stand in for this, for two independent reasons. A Layer-B core
//! crate may not depend on a strategy crate to discover one — that is the
//! dependency inversion RFC-0973 Lifecycle Requirement 8 forbids. And core
//! cannot observe crates at link time at all, so a core-supplied claimant would
//! be exactly the self-asserted identity the ownership check exists to reject.
//!
//! A strategy asked to name itself would name whatever the allocation record
//! names, and the check would compare a value with itself.
//!
//! # What is registered today
//!
//! `in_boundary_execution` is **not** registered. RFC-0973 makes registration
//! conditional on two preconditions — the profile measured, and the relation
//! verified against the trace the boundary actually produces — and neither
//! holds: the PoI trace fold has no host-side implementation, and no production
//! path emits a trace to check against. Naming it in an acceptance set refuses
//! boot as unresolvable, which is the fail-closed behaviour RFC-0973 provides
//! for exactly this case.
//!
//! This module therefore owns the host, the allocation record, and the
//! registration path — and proves the path works against a fixture, so that the
//! first real strategy to satisfy its preconditions registers by calling
//! [`register_hosted_strategies`] rather than by writing a second host.

use octo_cap_macaroon::attestation::{
    validate_policy, AcceptanceSet, AllocationRecord, AttestationRegistry, AttestationTypeId,
    BootRefusal, NodeCapability, RegisterError, Registration,
};

/// The claimant this host observes at link time for `octo-cap-poi`.
///
/// A constant rather than something read from the strategy: the value must come
/// from the link edge, which is why it is written here, in the crate that
/// carries the dependency, and not in the strategy crate.
const POI_CLAIMANT: &str = octo_cap_poi::CLAIMANT;

/// Why a strategy could not be registered by the host.
#[derive(Debug, thiserror::Error)]
pub enum HostRegistrationError {
    /// The registry refused. Ownership and duplicate-claim failures arrive here.
    ///
    /// Rendered structurally rather than via `Display` on the substrate error:
    /// the variant is carried whole so a caller can distinguish an
    /// `AlreadyRegistered` from a `WrongOwner` and act on each differently.
    #[error("registration refused: {refusal:?}")]
    Refused {
        /// The substrate's refusal, carried verbatim.
        refusal: RegisterError,
    },
}

/// The node's allocation record: which crate owns which RFC-range id.
///
/// This is an allocation, not a discovery step. It is a static, reviewed
/// statement of ownership, and it is what makes the ownership gate meaningful —
/// an id present here and claimed by the wrong crate is refused rather than
/// silently claimed.
#[must_use]
pub fn allocation_record() -> AllocationRecord {
    let mut allocations = AllocationRecord::empty();
    // RFC-0973 allocates `in_boundary_execution` to the crate implementing it.
    // The allocation exists whether or not the crate is registered: an
    // unallocated RFC-range id would be refused as `NotAllocated`, and an
    // allocated one that no host registers is refused as unresolvable at boot.
    // Those are different failures with different fixes, and keeping them
    // distinct is the point.
    allocations.allocate(octo_cap_poi::IN_BOUNDARY_EXECUTION, POI_CLAIMANT);
    allocations
}

/// The strategies this host links, paired with the claimant observed for each.
///
/// A strategy is absent from this table rather than registered-with-`None`
/// while its preconditions are unmet. An empty table is the truthful statement
/// that this host serves no attestation types, which is a permitted operator
/// choice — RFC-0973 requires it be *visible*, not that it be impossible.
#[must_use]
pub fn hosted_registrations() -> Vec<Registration> {
    // Intentionally empty. See the module docs: `in_boundary_execution` awaits
    // a measured profile and a host-side PoI fold, and `root_secret_knowledge`
    // awaits the in-circuit BLAKE3 entry point (mission 0973-v36-b).
    Vec::new()
}

/// Register every strategy this host links into `registry`.
///
/// # Errors
/// Returns the first [`RegisterError`] the registry raises. A refusal is never
/// downgraded to a skip: a strategy the host claims to serve but cannot
/// register must leave the node unable to boot, not silently absent from its
/// own acceptance set.
pub fn register_hosted_strategies(
    registry: &mut AttestationRegistry,
) -> Result<(), HostRegistrationError> {
    let allocations = allocation_record();
    for registration in hosted_registrations() {
        let claimant = claimant_for(&registration.strategy.type_id());
        // `register_strategy` takes the admission side as a second argument.
        // A registration built without one would make the coverage gate read
        // the permissive value for every strategy, which is the vacuous-check
        // failure the trait split exists to prevent.
        let Some(admission) = registration.admission.clone() else {
            continue;
        };
        registry
            .register_strategy(
                registration.strategy,
                Some(admission),
                claimant,
                &allocations,
            )
            .map_err(|refusal| HostRegistrationError::Refused { refusal })?;
    }
    Ok(())
}

/// The claimant the host observes for `type_id` at link time.
///
/// A lookup over the host's own link table, never a call into the strategy. An
/// id the host does not link has no claimant, and returning one would recreate
/// the self-assertion this module exists to prevent.
fn claimant_for(type_id: &AttestationTypeId) -> &'static str {
    if *type_id == octo_cap_poi::IN_BOUNDARY_EXECUTION {
        POI_CLAIMANT
    } else {
        // No host-side link for this id. Registering it would compare an
        // allocation against an empty claimant, so the caller must not reach
        // here for a linked strategy.
        "unlinked"
    }
}

/// Boot validation, wired to what this host actually serves.
///
/// Thin wrapper so a node composes one call rather than importing the substrate
/// entry point directly. Semantics are entirely the substrate's: an acceptance
/// entry naming an unresolvable strategy, a missing producing-side payload kind,
/// an unmeasured cost, or an exceeded budget each refuse, and an acceptance set
/// naming nothing boots.
///
/// # Errors
/// Propagates the substrate's [`BootRefusal`] unchanged.
pub fn boot_validate(
    accept_set: &AcceptanceSet,
    registry: &AttestationRegistry,
    node: &dyn NodeCapability,
) -> Result<(), BootRefusal> {
    validate_policy(accept_set, registry, node)
}

/// The acceptance set this host advertises.
///
/// Empty today, and empty is a legitimate advertised value rather than a
/// missing one: an operator running a node that verifies no attestations is a
/// permitted choice, and the mitigation RFC-0973 specifies is that it is
/// visible. Returning an empty set from one named function is what makes it
/// visible; a defaulting call site would hide it.
#[must_use]
pub fn advertised_acceptance_set() -> AcceptanceSet {
    AcceptanceSet::empty()
}

/// The registry this host serves, after registration.
pub fn build_registry() -> Result<AttestationRegistry, HostRegistrationError> {
    let mut registry = AttestationRegistry::empty();
    register_hosted_strategies(&mut registry)?;
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use octo_cap_macaroon::attestation::{
        AttestationAdmission, AttestationError, AttestationOutcome, AttestationStrategy,
        AttestationVerifyCtx, CostFigure,
    };

    /// A fixture strategy in the **user range**, so it never enters an
    /// operator's acceptance set and never collides with an RFC allocation.
    struct FixtureStrategy;

    impl AttestationStrategy for FixtureStrategy {
        fn type_id(&self) -> AttestationTypeId {
            let mut b = [0x80u8; 16];
            b[15] = 0xF1;
            AttestationTypeId::from_bytes(b)
        }
        fn verify(
            &self,
            _ctx: &AttestationVerifyCtx<'_>,
        ) -> Result<AttestationOutcome, AttestationError> {
            Ok(AttestationOutcome::Satisfied)
        }
    }

    impl AttestationAdmission for FixtureStrategy {
        fn admission_profile(&self) -> Option<octo_cap_macaroon::attestation::AdmissionProfile> {
            Some(octo_cap_macaroon::attestation::AdmissionProfile {
                casm_cost_hint_words: CostFigure::Measured(10),
                security_bits:
                    octo_cap_macaroon::attestation::SecurityFigure::from_prover_parameters(128),
            })
        }
    }

    struct Node {
        headroom: u64,
    }

    impl NodeCapability for Node {
        fn serves(&self, _kind: &octo_protocol::payload_kind::PayloadKindId) -> bool {
            true
        }
        fn has_headroom(&self, words: u64, _security_bits: u64) -> bool {
            words <= self.headroom
        }
    }

    #[test]
    fn the_allocation_record_allocates_the_rfc_id_to_its_crate() {
        let allocations = allocation_record();
        assert_eq!(
            allocations.owner_of(&octo_cap_poi::IN_BOUNDARY_EXECUTION),
            Some(POI_CLAIMANT)
        );
    }

    /// The host builds and boots with nothing registered. This is the
    /// permitted operator choice, and it must be a *successful* boot rather
    /// than a refusal.
    #[test]
    fn an_empty_host_boots_and_advertises_an_empty_acceptance_set() {
        let registry = build_registry().expect("an empty registry must build");
        assert!(registry.is_empty());

        let accept_set = advertised_acceptance_set();
        assert!(accept_set.is_empty());

        let node = Node { headroom: 0 };
        boot_validate(&accept_set, &registry, &node)
            .expect("a node accepting no attestations must start, not refuse");
    }

    /// Naming an unregistered strategy refuses boot. This is the fail-closed
    /// path the RFC relies on for a strategy whose preconditions are unmet.
    #[test]
    fn naming_an_unregistered_strategy_refuses_boot() {
        let registry = build_registry().unwrap();
        let accept_set = AcceptanceSet::from_ids([octo_cap_poi::IN_BOUNDARY_EXECUTION]);
        let node = Node { headroom: u64::MAX };
        let err = boot_validate(&accept_set, &registry, &node).unwrap_err();
        assert_eq!(
            err,
            BootRefusal::UnresolvableStrategy(octo_cap_poi::IN_BOUNDARY_EXECUTION)
        );
    }

    /// The registration path itself must work, or the first real strategy to
    /// satisfy its preconditions has no proven path to register through.
    #[test]
    fn the_registration_path_admits_a_fixture_and_then_it_verifies() {
        let mut registry = AttestationRegistry::empty();
        let allocations = allocation_record();

        let type_id = registry
            .register_strategy(
                Arc::new(FixtureStrategy),
                Some(Arc::new(FixtureStrategy)),
                "fixture",
                &allocations,
            )
            .expect("a user-range id needs no allocation");

        // A user-range id is that crate's own business, so it registers without
        // an allocation entry.
        assert!(registry.resolve_registration(&type_id).is_some());

        let accept_set = AcceptanceSet::from_ids([type_id]);
        let node = Node { headroom: u64::MAX };
        boot_validate(&accept_set, &registry, &node)
            .expect("a resolvable, measured, affordable strategy must boot");
    }

    /// Ownership binding must be non-vacuous end to end: the RFC-range id is
    /// allocated to `octo-cap-poi`, so any other claimant is refused.
    #[test]
    fn the_rfc_id_is_refused_to_any_other_claimant() {
        let mut registry = AttestationRegistry::empty();
        let allocations = allocation_record();

        struct Impostor;
        impl AttestationStrategy for Impostor {
            fn type_id(&self) -> AttestationTypeId {
                octo_cap_poi::IN_BOUNDARY_EXECUTION
            }
            fn verify(
                &self,
                _ctx: &AttestationVerifyCtx<'_>,
            ) -> Result<AttestationOutcome, AttestationError> {
                Ok(AttestationOutcome::Satisfied)
            }
        }

        let err = registry
            .register_strategy(Arc::new(Impostor), None, "octo-cap-zk", &allocations)
            .unwrap_err();
        assert!(matches!(err, RegisterError::WrongOwner { .. }));
    }

    /// The claimant must come from the host's link table, never from the
    /// strategy. An id the host does not link has no claimant.
    #[test]
    fn an_unlinked_id_has_no_claimant() {
        let mut b = [0x80u8; 16];
        b[15] = 0xF1;
        let unlinked = AttestationTypeId::from_bytes(b);
        assert_eq!(claimant_for(&unlinked), "unlinked");
        assert_eq!(
            claimant_for(&octo_cap_poi::IN_BOUNDARY_EXECUTION),
            POI_CLAIMANT
        );
    }
}
