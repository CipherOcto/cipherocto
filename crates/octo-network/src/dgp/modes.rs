//! Gossip modes (RFC-0852 §6)

/// Gossip propagation modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GossipMode {
    /// Broadcast aggressively — bootstrap, emergency, partition recovery
    Flood,
    /// Only unseen objects — normal operation
    Incremental,
    /// Merkle summary exchange — periodic reconciliation
    AntiEntropy,
    /// Targeted propagation — mission overlays, validator coordination
    Directed,
}
