//! `CAPABILITY_REVOKE` handler (RFC-0871 §Roles and Authorities, mission
//! 0871d-capability-issuer-node).
//!
//! Receives: `<cap_root_hash: [u8; 32]>` — the registry primary key of the
//! capability to revoke.
//! Returns: `<cap_root_hash: [u8; 32], revoked: bool>`.
//!
//! ## Why the request carries a 32-byte key
//!
//! It used to carry `token_id: MacaroonId`, a 16-byte identifier. `HolderRegistry`
//! is keyed by `cap_root_hash: [u8; 32]` and its `revoke` takes that key, so a
//! 16-byte identifier **cannot address the registry at all** — there is no
//! lookup-by-channel-id anywhere in the workspace to bridge them. The stub
//! returned `acknowledged: true` for a `token_id` it had no way to resolve, so
//! the lie was invisible until the identifier was checked against the key it
//! had to name.
//!
//! A holder holding its own macaroon computes the key with
//! `compute_cap_root_hash_from_wire`, so the 32-byte form is derivable by the
//! party that legitimately wants to revoke.
//!
//! RFC-0871 §Roles and Authorities does not specify this payload kind's wire
//! form; the 16-byte identifier was an implementation choice from mission
//! 0871d. Nothing consumes it in production, so nothing breaks.
//!
//! ## Production semantics
//!
//! `HolderRegistry::revoke` is idempotent (revoked stays revoked; there is no
//! un-revoke). `revoked` in the response means **this registry holds a record
//! for this key and has marked it revoked** — not "a request was received".
//!
//! `revoked: false` means this registry has no record for that key. That is a
//! definite answer, not an unknown, so it is a response rather than an error:
//! the caller learns the capability was never registered here. Eventual-
//! consistency gossip for revocation propagation across nodes is a separate
//! concern (RFC-0957-A1 revocation sync — not yet spec'd).
//!
//! Authorization for the revoke itself — RFC-0965 `RevocationCaveat` issued by
//! the original issuer or a higher-authority governance capability — is **not**
//! enforced here. Any holder that can reach this payload kind can revoke any
//! capability this node knows. That is an open gap with no mission filed
//! against it yet; RFC-0965 owns the rule.

use borsh::{BorshDeserialize, BorshSerialize};
use octo_cap_macaroon::{Clock, HolderRegistry};
use octo_protocol::ProtocolError;

use super::HandlerOutput;

/// Request payload for `CAPABILITY_REVOKE`.
///
/// Wire form: borsh (`cap_root_hash`).
#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct RevokeRequest {
    /// 32-byte registry primary key (`cap_root_hash`). The holder computes it
    /// from its own macaroon via `compute_cap_root_hash_from_wire`.
    pub cap_root_hash: [u8; 32],
}

impl RevokeRequest {
    /// Decode from borsh wire form.
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` if borsh decode fails.
    pub fn from_borsh(bytes: &[u8]) -> Result<Self, ProtocolError> {
        borsh::from_slice(bytes).map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))
    }

    /// Encode to borsh wire form.
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` if borsh encode fails.
    pub fn to_borsh(&self) -> Result<Vec<u8>, ProtocolError> {
        borsh::to_vec(self).map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))
    }
}

/// Response payload for `CAPABILITY_REVOKE`.
///
/// Wire form: borsh (`cap_root_hash`, `revoked`).
#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct RevokeResponse {
    /// 32-byte registry primary key (echoed back).
    pub cap_root_hash: [u8; 32],
    /// True only if this registry holds a record for `cap_root_hash` and has
    /// marked it revoked. False when the registry has no such record.
    pub revoked: bool,
}

/// `CAPABILITY_REVOKE` handler.
///
/// Holds the node's registry and clock: RFC-0957-A1 §Roles and Authorities
/// gives the Token Issuer the authority to revoke, and §HolderRegistry takes
/// the clock by injection rather than reading one internally.
pub struct RevokeHandler<'a> {
    registry: &'a dyn HolderRegistry,
    clock: &'a dyn Clock,
}

impl<'a> RevokeHandler<'a> {
    /// Construct a `RevokeHandler` bound to a registry and a clock.
    #[must_use]
    pub const fn new(registry: &'a dyn HolderRegistry, clock: &'a dyn Clock) -> Self {
        Self { registry, clock }
    }

    /// Revoke the capability named by `req` in this node's registry.
    ///
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` if the registry read or
    /// write fails, or if the response cannot be serialized. A capability the
    /// registry does not know is **not** an error — the response says
    /// `revoked: false`.
    pub fn handle(&self, req: &RevokeRequest) -> Result<HandlerOutput, ProtocolError> {
        let known = self
            .registry
            .lookup(&req.cap_root_hash)
            .map_err(|e| ProtocolError::AuthorizationFailed(format!("registry lookup: {e}")))?
            .is_some();

        let mut revoked = false;
        if known {
            self.registry
                .revoke(&req.cap_root_hash, self.clock)
                .map_err(|e| ProtocolError::AuthorizationFailed(format!("registry revoke: {e}")))?;
            revoked = true;
        }

        let response = RevokeResponse {
            cap_root_hash: req.cap_root_hash,
            revoked,
        };
        let payload = borsh::to_vec(&response)
            .map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))?;
        let note = if revoked {
            format!("revoked {:02x?}", req.cap_root_hash)
        } else {
            format!(
                "not revoked: this registry holds no record for {:02x?}",
                req.cap_root_hash
            )
        };
        Ok(
            HandlerOutput::response(payload, octo_protocol::payload_kind::CAPABILITY_REVOKE)
                .with_note(note),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use octo_cap_macaroon::holder_record::{CapabilityClass, CapabilityTokenLike, HolderRecord};
    use octo_cap_macaroon::{FixedClock, HolderKind};

    fn sample_registry() -> Arc<dyn HolderRegistry> {
        Arc::new(octo_ident_storage::StoolapHolderRegistry::open_in_memory().unwrap())
    }

    const HOLDER_PUB: [u8; 32] = [0x77; 32];

    fn register(reg: &dyn HolderRegistry, cap_root_hash: [u8; 32]) -> HolderRecord {
        let record = HolderRecord::from_capability(
            &CapabilityTokenLike {
                cap_root_hash,
                class: CapabilityClass::V1,
            },
            &HOLDER_PUB,
            "did:octo:zRevokeHolder",
            None,
            0,
        );
        reg.insert(record.clone()).expect("insert");
        record
    }

    fn response_from(out: &HandlerOutput) -> RevokeResponse {
        let payload = out
            .response_payload
            .as_ref()
            .expect("response payload present");
        borsh::from_slice(payload).expect("decode RevokeResponse")
    }

    #[test]
    fn revoke_request_borsh_round_trip() {
        let req = RevokeRequest {
            cap_root_hash: [0xab; 32],
        };
        let bytes = req.to_borsh().unwrap();
        let back = RevokeRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    /// Revoking a registered capability marks it revoked **in the registry**,
    /// which is the whole point. The previous stub asserted `acknowledged` on a
    /// response and touched nothing.
    #[test]
    fn revoking_a_registered_capability_marks_the_registry_record_revoked() {
        let reg = sample_registry();
        let clock = FixedClock::new(1_700_000_500_000);
        let cap_root_hash = [0x42; 32];
        let record = register(&*reg, cap_root_hash);
        assert!(
            record.revoked_at_millis_unix.is_none(),
            "fresh record is live"
        );

        let out = RevokeHandler::new(&*reg, &clock)
            .handle(&RevokeRequest { cap_root_hash })
            .expect("revoke");

        assert!(
            response_from(&out).revoked,
            "a known capability must report revoked"
        );

        let after = reg
            .lookup(&cap_root_hash)
            .expect("lookup")
            .expect("still present");
        assert_eq!(
            after.revoked_at_millis_unix,
            Some(1_700_000_500_000),
            "the registry row must carry the revocation timestamp"
        );
    }

    /// A revoked capability is no longer active. This is the property that
    /// makes revocation a security control rather than a bookkeeping entry —
    /// `verify_with_resolve` consults `lookup_active`.
    #[test]
    fn a_revoked_capability_is_no_longer_active() {
        let reg = sample_registry();
        let clock = FixedClock::new(1_700_000_500_000);
        let cap_root_hash = [0x43; 32];
        register(&*reg, cap_root_hash);
        assert!(
            reg.lookup_active(&cap_root_hash, &clock)
                .expect("active")
                .is_some(),
            "a freshly registered capability is active"
        );

        RevokeHandler::new(&*reg, &clock)
            .handle(&RevokeRequest { cap_root_hash })
            .expect("revoke");

        assert!(
            reg.lookup_active(&cap_root_hash, &clock)
                .expect("active")
                .is_none(),
            "a revoked capability must not resolve as active"
        );
    }

    /// The anti-lie test. The stub answered `true` to every request, including
    /// ones naming a capability it had never heard of.
    #[test]
    fn revoking_an_unknown_capability_reports_not_revoked() {
        let reg = sample_registry();
        let clock = FixedClock::new(1_700_000_500_000);
        let out = RevokeHandler::new(&*reg, &clock)
            .handle(&RevokeRequest {
                cap_root_hash: [0xde; 32],
            })
            .expect("unknown revoke is an answer, not an error");
        assert!(
            !response_from(&out).revoked,
            "a capability this registry never registered cannot have been revoked"
        );
    }

    /// Idempotence is the registry's documented contract; the handler must not
    /// turn a repeat revoke into a failure or a different answer.
    #[test]
    fn revoking_twice_still_reports_revoked() {
        let reg = sample_registry();
        let clock = FixedClock::new(1_700_000_500_000);
        let cap_root_hash = [0x44; 32];
        register(&*reg, cap_root_hash);
        let handler = RevokeHandler::new(&*reg, &clock);
        let req = RevokeRequest { cap_root_hash };
        assert!(response_from(&handler.handle(&req).unwrap()).revoked);
        assert!(
            response_from(&handler.handle(&req).unwrap()).revoked,
            "revoke is idempotent, so the second call still reports revoked"
        );
        // The first timestamp stands — no un-revoke, no rewrite.
        let after = reg.lookup(&cap_root_hash).unwrap().unwrap();
        assert_eq!(after.revoked_at_millis_unix, Some(1_700_000_500_000));
    }

    /// Revocation is recorded under the holder's `HolderKind`, so a record of
    /// another kind sharing the key is not silently revoked.
    #[test]
    fn revocation_preserves_the_record_kind_and_holder() {
        let reg = sample_registry();
        let clock = FixedClock::new(1_700_000_500_000);
        let cap_root_hash = [0x45; 32];
        let record = register(&*reg, cap_root_hash);
        RevokeHandler::new(&*reg, &clock)
            .handle(&RevokeRequest { cap_root_hash })
            .expect("revoke");
        let after = reg.lookup(&cap_root_hash).unwrap().unwrap();
        assert_eq!(after.kind, HolderKind::V1);
        assert_eq!(after.holder_pub, record.holder_pub);
        assert_eq!(after.holder_did, record.holder_did);
    }

    #[test]
    fn revoke_response_borsh_round_trip() {
        let resp = RevokeResponse {
            cap_root_hash: [0xef; 32],
            revoked: true,
        };
        let bytes = borsh::to_vec(&resp).unwrap();
        let back: RevokeResponse = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, resp);
    }
}
