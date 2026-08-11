//! Drain coordinator (per RFC-0862 v1.3 §DrainCoordinator).
//!
//! `DrainCoordinator` mediates `submit_drain` calls across instances,
//! checking writer availability and holder balance before granting
//! the drain. The `submit_drain_local_fallback` default is fail-closed
//! (returns `WriterUnavailable`) per RFC-0862 v1.3 R12 — the LWW
//! fallback is gated behind a future RFC-0862 v1.4 amendment
//! (per `drain-coordinator-approach-2026-08-10` Option C).

use async_trait::async_trait;

use super::records::ActualDrained;
use super::records::DrainCoordinatorError;

/// Per-RFC-0862 v1.3 §DrainCoordinator trait.
///
/// `#[async_trait]` for dyn-compatibility (per R12 M18). Consumed
/// via `Arc<dyn DrainCoordinator>` at the layer-B drain mediator.
#[async_trait]
pub trait DrainCoordinator: Send + Sync {
    /// Submit a drain request. Routes through the writer-election
    /// substrate; returns `ActualDrained` with the receipt LSN on
    /// success.
    async fn submit_drain(
        &self,
        holder_did: &str,
        macaroon_id: &[u8],
        requested_cost: u128,
    ) -> Result<ActualDrained, DrainCoordinatorError>;

    /// `#[deprecated]` local fallback. Per RFC-0862 v1.3 R12 the
    /// default impl is fail-closed; the LWW fallback is gated behind
    /// a future RFC-0862 v1.4 F12 + F13 amendment.
    #[deprecated(since = "1.3.0", note = "LWW substrate pending F12 amendment")]
    async fn submit_drain_local_fallback(
        &self,
        holder_did: &str,
        macaroon_id: &[u8],
        requested_cost: u128,
    ) -> Result<(), DrainCoordinatorError> {
        let _ = (holder_did, macaroon_id, requested_cost);
        Err(DrainCoordinatorError::WriterUnavailable)
    }
}
