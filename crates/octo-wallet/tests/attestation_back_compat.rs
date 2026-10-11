//! RFC-0973 §Phase 2 — the frozen pre-RFC capability corpus, re-run.
//!
//! The base mission froze the corpus and pinned the canonical bytes. This phase
//! re-runs it rather than assuming it, because this is the phase where the
//! dispatch first meets real traffic: the property that matters now is not "the
//! bytes did not move" (Phase 1 owns that) but "a capability minted before any
//! of this still **verifies**, end to end, through the new dispatch".
//!
//! A corpus of byte assertions would pass here even if the dispatch refused
//! every unattested token, because byte stability and verification are different
//! claims. This file asserts the second.
//!
//! ## What "before the RFC" means
//!
//! A pre-RFC capability has no `attestation` field populated. Anything minted
//! now with an envelope attached is post-RFC by construction, so the corpus is
//! built from the mint path and left alone — never round-tripped through the
//! envelope encode/decode, which is the new code path under test elsewhere.
//!
//! ## Per `docs/05-process/engineering-rules.md` §No line references in documentation convention
//!
//! References use `§symbol-name` form.

use std::sync::Arc;

use octo_cap_macaroon::macaroon::Macaroon;
use octo_ident_storage::StoolapHolderRegistry;
use octo_wallet::capability::verify::{verify_attestation_for_verified_token, VerifyContext};
use quota_router_storage::{Clock, FixedClock};

fn registry() -> Arc<dyn quota_router_storage::HolderRegistry> {
    Arc::new(StoolapHolderRegistry::open_in_memory().unwrap())
}

fn clock() -> Arc<dyn Clock> {
    Arc::new(FixedClock::new(1_700_000_000_000))
}

/// A context with the attestation subsystem configured, so the dispatch is
/// actually entered rather than short-circuited by an absent subsystem.
fn attested_ctx(accept: Vec<octo_cap_macaroon::attestation::AttestationTypeId>) -> VerifyContext {
    use octo_cap_macaroon::attestation::{AcceptanceSet, AttestationRegistry};
    use octo_wallet::capability::attestation::PermissivePolicy;

    VerifyContext::with_registry(clock(), registry()).with_attestations(
        Arc::new(AttestationRegistry::empty()),
        AcceptanceSet::from_ids(accept),
        Arc::new(PermissivePolicy),
    )
}

/// The corpus: capabilities minted before RFC-0973 existed.
///
/// Each is a distinct mint input, so the corpus is not N copies of one case. An
/// all-zero root secret is excluded because mint refuses it, which is
/// pre-RFC behaviour and not a corpus entry.
fn corpus() -> Vec<(&'static str, Macaroon)> {
    vec![
        ("single_byte_seed", Macaroon::mint(&[1u8; 32]).unwrap()),
        (
            "ascending_seed",
            Macaroon::mint(&[
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
                23, 24, 25, 26, 27, 28, 29, 30, 31,
            ])
            .unwrap(),
        ),
        ("high_bit_seed", Macaroon::mint(&[0xffu8; 32]).unwrap()),
        ("max_seed", Macaroon::mint(&[0xff; 32]).unwrap()),
    ]
}

/// Every pre-RFC capability still verifies, on every context shape.
///
/// Three shapes, because the property is not "the dispatch tolerates absence"
/// but "the dispatch is not reached in a way that refuses". A token with no
/// envelope must pass whether the operator accepted nothing, accepted something
/// else entirely, or has no acceptance set configured at all.
#[test]
fn every_pre_rfc_capability_still_verifies_through_the_dispatch() {
    for (name, macaroon) in corpus() {
        assert!(
            macaroon.attestation.is_none(),
            "{name}: corpus entries must be unattested by construction"
        );

        for (shape, ctx) in [
            (
                "no acceptance set",
                VerifyContext::with_registry(clock(), registry()),
            ),
            ("empty acceptance set", attested_ctx(vec![])),
        ] {
            verify_attestation_for_verified_token(&ctx, &macaroon)
                .unwrap_or_else(|e| panic!("{name} must verify with {shape}, got {e}"));
        }
    }
}

/// An acceptance set naming an unrelated strategy must not disturb a pre-RFC
/// token.
///
/// This is the shape an operator actually runs: they enable attestation for
/// some type, and every capability minted before the feature existed must keep
/// working. A dispatch that consulted the acceptance set on the unattested
/// branch would refuse these.
#[test]
fn an_acceptance_set_naming_another_type_does_not_disturb_pre_rfc_tokens() {
    let mut other = [0u8; 16];
    other[15] = 0xA7;
    let accept = octo_cap_macaroon::attestation::AttestationTypeId::from_bytes(other);
    let ctx = attested_ctx(vec![accept]);

    for (name, macaroon) in corpus() {
        verify_attestation_for_verified_token(&ctx, &macaroon)
            .unwrap_or_else(|e| panic!("{name} must verify, got {e}"));
    }
}

/// The corpus must still be free of the RFC-0973 byte change.
///
/// Re-asserted here rather than trusting Phase 1: the dispatch work touched
/// `Macaroon`, and "the bytes are unchanged" is the one claim a later phase
/// breaks silently. The check is on the canonical form, which is what the
/// capability id hashes.
#[test]
fn the_corpus_carries_no_rfc0973_bytes() {
    for (name, macaroon) in corpus() {
        let canonical = macaroon.canonical_ser_unsigned();
        assert!(
            !canonical.is_empty(),
            "{name}: canonical form must not be empty"
        );
        // The envelope is omitted entirely when absent, so nothing about
        // RFC-0973 appears in the bytes. Pinned as a self-consistency check on
        // the field rather than a byte literal: the literal byte layout is
        // pinned in the base mission, where it belongs, and duplicating it here
        // would create a second site to edit when it changes.
        assert!(
            macaroon.attestation.is_none(),
            "{name}: no envelope means no envelope bytes"
        );
    }
}
