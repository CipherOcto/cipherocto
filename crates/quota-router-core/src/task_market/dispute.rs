//! Dispute resolution — task market dispute creation + evidence.
//!
//! Placeholder; full implementation lands in Task 6.3.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisputeReason {
    ResultMismatch,
    ProviderTimeout,
    ProviderError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub hash: [u8; 32],
    pub description: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DisputeError {
    #[error("dispute already exists for escrow {0:?}")]
    AlreadyExists([u8; 32]),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dispute {
    pub escrow_id: [u8; 32],
    pub raised_by: String,
    pub reason: DisputeReason,
    pub evidence: Option<Evidence>,
}

impl Dispute {
    /// Construct a new dispute against `escrow_id` raised by `raised_by`.
    #[must_use]
    pub fn new(
        escrow_id: [u8; 32],
        raised_by: impl Into<String>,
        reason: DisputeReason,
        evidence: Option<Evidence>,
    ) -> Self {
        Self {
            escrow_id,
            raised_by: raised_by.into(),
            reason,
            evidence,
        }
    }

    /// True if the dispute carries a verifiable evidence payload.
    #[must_use]
    pub fn has_evidence(&self) -> bool {
        self.evidence.is_some()
    }
}

/// In-memory registry of disputes keyed by escrow id.
///
/// Production deployments would back this with the cipherocto-side
/// policy/catalog store; the in-memory variant is sufficient for the
/// Gap 6 surface and lets us assert at-most-one-dispute-per-escrow in
/// tests.
#[derive(Debug, Default, Clone)]
pub struct DisputeRegistry {
    disputes: std::collections::BTreeMap<[u8; 32], Dispute>,
}

impl DisputeRegistry {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a dispute. Errors if a dispute already exists for the escrow.
    /// # Errors
    /// Returns `DisputeError::AlreadyExists` if `escrow_id` is already
    /// disputed.
    pub fn open(&mut self, dispute: Dispute) -> Result<&Dispute, DisputeError> {
        if self.disputes.contains_key(&dispute.escrow_id) {
            return Err(DisputeError::AlreadyExists(dispute.escrow_id));
        }
        let id = dispute.escrow_id;
        self.disputes.insert(id, dispute);
        Ok(self.disputes.get(&id).expect("just inserted"))
    }

    /// Get the dispute for `escrow_id`, if any.
    #[must_use]
    pub fn get(&self, escrow_id: &[u8; 32]) -> Option<&Dispute> {
        self.disputes.get(escrow_id)
    }

    /// Resolve (and remove) the dispute for `escrow_id`. Returns the
    /// dispute if it existed.
    pub fn resolve(&mut self, escrow_id: &[u8; 32]) -> Option<Dispute> {
        self.disputes.remove(escrow_id)
    }

    /// Number of open disputes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.disputes.len()
    }

    /// True if no disputes are open.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.disputes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispute_new_sets_fields() {
        let d = Dispute::new(
            [0xab; 32],
            &octo_ident::test_helpers::sample_did(8),
            DisputeReason::ResultMismatch,
            None,
        );
        assert_eq!(d.escrow_id, [0xab; 32]);
        assert_eq!(d.raised_by, octo_ident::test_helpers::sample_did(8));
        assert!(!d.has_evidence());
    }

    #[test]
    fn dispute_with_evidence_has_evidence() {
        let d = Dispute::new(
            [0xab; 32],
            &octo_ident::test_helpers::sample_did(8),
            DisputeReason::ProviderError,
            Some(Evidence {
                hash: [0xcc; 32],
                description: "5xx response".into(),
            }),
        );
        assert!(d.has_evidence());
    }

    #[test]
    fn registry_open_records_dispute() {
        let mut reg = DisputeRegistry::new();
        let d = Dispute::new(
            [0x01; 32],
            &octo_ident::test_helpers::sample_did(8),
            DisputeReason::ResultMismatch,
            None,
        );
        let stored = reg.open(d).expect("open");
        assert_eq!(stored.escrow_id, [0x01; 32]);
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn registry_open_rejects_duplicate() {
        let mut reg = DisputeRegistry::new();
        let d1 = Dispute::new(
            [0x01; 32],
            &octo_ident::test_helpers::sample_did(8),
            DisputeReason::ResultMismatch,
            None,
        );
        let d2 = Dispute::new(
            [0x01; 32],
            &octo_ident::test_helpers::sample_did(8),
            DisputeReason::ProviderTimeout,
            None,
        );
        reg.open(d1).expect("first open");
        let err = reg.open(d2).unwrap_err();
        assert_eq!(err, DisputeError::AlreadyExists([0x01; 32]));
    }

    #[test]
    fn registry_resolve_removes_dispute() {
        let mut reg = DisputeRegistry::new();
        let d = Dispute::new(
            [0x01; 32],
            &octo_ident::test_helpers::sample_did(8),
            DisputeReason::ResultMismatch,
            None,
        );
        reg.open(d).expect("open");
        let removed = reg.resolve(&[0x01; 32]).expect("removed");
        assert_eq!(removed.escrow_id, [0x01; 32]);
        assert!(reg.is_empty());
    }

    #[test]
    fn registry_resolve_unknown_returns_none() {
        let mut reg = DisputeRegistry::new();
        assert!(reg.resolve(&[0x99; 32]).is_none());
    }
}
