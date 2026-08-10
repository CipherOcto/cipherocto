//! `CAPABILITY_LOOKUP` handler (RFC-0871 §Roles and Authorities,
//! mission 0957-phase2c).
//!
//! Receives: `<cap_root_hash: [u8; 32]>` — the 32-byte capability
//! root hash PK per RFC-0957-A1 §Data Structures (the same PK the
//! `HolderRegistry::lookup` consumes).
//! Returns: `<holder_record: Option<HolderRecord>>` — `Some` if a
//! record exists for the PK, `None` otherwise.
//!
//! The handler is read-only: no `HolderRegistry` mutation, no event
//! emission. The lookup response carries the full `HolderRecord` so
//! callers can inspect `revoked_at_millis_unix`, `kind`, `holder_did`,
//! `caveats_canonical`, etc. without a second round-trip.

use borsh::{BorshDeserialize, BorshSerialize};
use octo_protocol::ProtocolError;
use quota_router_storage::holder_record::HolderRecord;
use quota_router_storage::holder_registry::HolderRegistry;

use super::HandlerOutput;

/// Request payload for `CAPABILITY_LOOKUP`.
///
/// Wire form: borsh (`cap_root_hash`).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct CapabilityLookupRequest {
    /// 32-byte capability root hash PK (RFC-0957-A1 §Data
    /// Structures — `holder_registry` table PK).
    pub cap_root_hash: [u8; 32],
}

impl CapabilityLookupRequest {
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

/// Response payload for `CAPABILITY_LOOKUP`.
///
/// Wire form: borsh envelope wrapping a length-prefixed canonical-JSON
/// `HolderRecord` payload (RFC-0126). The borsh envelope carries a
/// `present: bool` discriminant + the 32-byte PK echo; the inner
/// canonical-JSON bytes are `HolderRecord::canonical_ser` output (or
/// empty for absent records). This avoids coupling `HolderRecord` to
/// the borsh wire format (the canonical_ser is the gossip-friendly
/// substrate form per RFC-0957-A1 §G5).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct CapabilityLookupResponse {
    /// Whether a `HolderRecord` is present.
    pub present: bool,
    /// Canonical-JSON `HolderRecord` bytes (`canonical_ser`); empty
    /// when `present == false`.
    pub holder_record_bytes: Vec<u8>,
    /// Echo of the request's `cap_root_hash`.
    pub cap_root_hash: [u8; 32],
}

impl CapabilityLookupResponse {
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

    /// Decode the inner canonical-JSON `HolderRecord` (if present).
    /// Returns `Ok(None)` for absent records; `Ok(Some(_))` for
    /// present records.
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` if canonical-JSON
    /// deserialization fails.
    pub fn holder_record(&self) -> Result<Option<HolderRecord>, ProtocolError> {
        if !self.present {
            return Ok(None);
        }
        let rec = HolderRecord::canonical_de(&self.holder_record_bytes)
            .map_err(|e| ProtocolError::AuthorizationFailed(format!("canonical_de: {e}")))?;
        Ok(Some(rec))
    }
}

/// `CAPABILITY_LOOKUP` handler.
///
/// Holds a reference to the `HolderRegistry` substrate (RFC-0957-A1
/// §Algorithms). Read-only — no mutation, no event emission.
pub struct CapabilityLookupHandler<'a> {
    registry: &'a dyn HolderRegistry,
}

impl<'a> CapabilityLookupHandler<'a> {
    /// Construct a new lookup handler bound to the given registry.
    #[must_use]
    pub const fn new(registry: &'a dyn HolderRegistry) -> Self {
        Self { registry }
    }

    /// Look up the `HolderRecord` for the request's `cap_root_hash`.
    ///
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` if the registry
    /// reports a storage error. `NotFound` from the registry is
    /// surfaced as `Ok(None)` in the response (the lookup is a
    /// query, not a hard error).
    pub fn handle(&self, req: &CapabilityLookupRequest) -> Result<HandlerOutput, ProtocolError> {
        let record = self
            .registry
            .lookup(&req.cap_root_hash)
            .map_err(|e| ProtocolError::AuthorizationFailed(format!("holder registry: {e}")))?;
        let (present, holder_record_bytes) = match record.as_ref() {
            Some(r) => (
                true,
                r.canonical_ser().map_err(|e| {
                    ProtocolError::AuthorizationFailed(format!("canonical_ser: {e}"))
                })?,
            ),
            None => (false, Vec::new()),
        };
        let response = CapabilityLookupResponse {
            present,
            holder_record_bytes,
            cap_root_hash: req.cap_root_hash,
        };
        let payload = borsh::to_vec(&response)
            .map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))?;
        Ok(
            HandlerOutput::response(payload, octo_protocol::payload_kind::CAPABILITY_LOOKUP)
                .with_note(format!(
                    "lookup for cap_root_hash {:02x?}",
                    req.cap_root_hash
                )),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quota_router_storage::stoolap_holder_registry::StoolapHolderRegistry;

    fn sample_registry() -> std::sync::Arc<dyn HolderRegistry> {
        std::sync::Arc::new(StoolapHolderRegistry::open_in_memory().unwrap())
    }

    #[test]
    fn lookup_request_borsh_round_trip() {
        let req = CapabilityLookupRequest {
            cap_root_hash: [0xab; 32],
        };
        let bytes = req.to_borsh().unwrap();
        let back = CapabilityLookupRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn lookup_response_borsh_round_trip_none() {
        let resp = CapabilityLookupResponse {
            present: false,
            holder_record_bytes: Vec::new(),
            cap_root_hash: [0xab; 32],
        };
        let bytes = resp.to_borsh().unwrap();
        let back = CapabilityLookupResponse::from_borsh(&bytes).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn lookup_absent_returns_none() {
        let registry = sample_registry();
        let handler = CapabilityLookupHandler::new(&*registry);
        let req = CapabilityLookupRequest {
            cap_root_hash: [0x01; 32],
        };
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.expect("response payload");
        let resp = CapabilityLookupResponse::from_borsh(&payload).unwrap();
        assert!(!resp.present);
        assert!(resp.holder_record().unwrap().is_none());
        assert_eq!(resp.cap_root_hash, req.cap_root_hash);
    }
}
