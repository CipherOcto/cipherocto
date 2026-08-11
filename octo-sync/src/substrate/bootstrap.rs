//! Bootstrap orchestrator (per RFC-0862 v1.3 §BootstrapOrchestrator
//! + §Supporting types).
//!
//! `BootstrapOrchestrator` discovers peers via the underlying
//! transport overlay (libp2p / custom). The seal pattern is preserved
//! (per [[cipherocto-design-principles]] §No parallel abstractions):
//! only the substrate crate implements this trait; concrete impls
//! live alongside the `WriterElection` impl in production.

use async_trait::async_trait;

use super::records::BootstrapError;
use super::records::PeerIdentity;

/// Per-RFC-0862 v1.3 §BootstrapOrchestrator trait.
///
/// `#[async_trait]` for dyn-compatibility (per R12 M18): consumed
/// via `Arc<dyn BootstrapOrchestrator>` at the election construction
/// boundary.
#[async_trait]
pub trait BootstrapOrchestrator: Send + Sync {
    /// Discover peers for the local mission. Returns at least one
    /// peer on success; `BootstrapError::NoPeers` if discovery fails.
    async fn acquire_peers(&self) -> Result<Vec<PeerIdentity>, BootstrapError>;
}
