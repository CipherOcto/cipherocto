//! Sync configuration (per RFC-0862 §SyncConfig).
//!
//! The configuration is the operator's input to the cipherocto sync engine.
//! It is parsed from a DSN string or a config file (TBD per mission 0862-base
//! Phase 1). The struct is intentionally minimal for v1; future versions can
//! add multi-carrier, multi-peer, etc.

/// The role this node plays in the sync session (per RFC-0862 §4.1, G8).
///
/// The mission layer (RFC-0855) requires this role to be one of the
/// mission-defined roles. For the cipherocto sync engine, only `Replicator`
/// (writer) and `Observer` (reader) are accepted. Any other role produces
/// `E_SYNC_ROLE_NOT_SYNC_CAPABLE` at open time.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SyncRole {
    /// Writer. May issue WAL entries; may also receive (i.e., a writer can
    /// also be a reader for catch-up after restart).
    Replicator,
    /// Reader. May only receive WAL entries; cannot issue them.
    Observer,
}

impl SyncRole {
    /// Try to parse a role from a string.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "replicator" | "Replicator" => Ok(SyncRole::Replicator),
            "observer" | "Observer" => Ok(SyncRole::Observer),
            other => Err(format!("unknown sync role: {}", other)),
        }
    }

    /// Return the canonical string representation.
    pub fn as_str(self) -> &'static str {
        match self {
            SyncRole::Replicator => "Replicator",
            SyncRole::Observer => "Observer",
        }
    }
}

/// The cipherocto sync engine configuration.
#[derive(Clone, Debug)]
pub struct SyncConfig {
    /// The mission ID (32 bytes).
    pub mission_id: [u8; 32],
    /// The local node's role in the mission.
    pub role: SyncRole,
    /// The local node's public key (32 bytes; ed25519).
    pub public_key: Vec<u8>,
    /// For readers: the writer's `SyncNodeId`. The reader rejects WAL chunks
    /// from any other peer (per RFC-0862 §Roles and Authorities).
    pub writer_node_id: Option<[u8; 32]>,
    /// The transport carrier (e.g., "nativep2p", "webhook"). Single-carrier in
    /// v1; multi-carrier is in mission 0862g.
    pub transport: String,
    /// Heartbeat interval (seconds). Default: 5.
    pub heartbeat_interval_secs: u64,
    /// Suspect threshold (`heartbeat_interval_secs × suspect_multiplier`).
    /// Default: 2 (i.e., 10s for the default 5s interval).
    pub suspect_multiplier: u64,
    /// Reconnect attempts before `Terminated`. Default: 5 (~5 min).
    pub reconnect_attempts: u32,
    /// Per-peer rate limit (envelopes/s sustained). Default: 100.
    pub rate_limit_per_sec: u32,
    /// Per-peer rate limit burst. Default: 500.
    pub rate_limit_burst: u32,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            mission_id: [0u8; 32],
            role: SyncRole::Observer,
            public_key: Vec::new(),
            writer_node_id: None,
            transport: "nativep2p".to_string(),
            heartbeat_interval_secs: 5,
            suspect_multiplier: 2,
            reconnect_attempts: 5,
            rate_limit_per_sec: 100,
            rate_limit_burst: 500,
        }
    }
}

impl SyncConfig {
    /// Create a new `SyncConfig` with the given mission_id, role, and public_key.
    pub fn new(mission_id: [u8; 32], role: SyncRole, public_key: Vec<u8>) -> Self {
        Self {
            mission_id,
            role,
            public_key,
            ..Default::default()
        }
    }

    /// Set the writer's `SyncNodeId` (for readers).
    pub fn with_writer_node_id(mut self, writer_node_id: [u8; 32]) -> Self {
        self.writer_node_id = Some(writer_node_id);
        self
    }

    /// Set the transport carrier.
    pub fn with_transport(mut self, transport: impl Into<String>) -> Self {
        self.transport = transport.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sane_values() {
        let c = SyncConfig::default();
        assert_eq!(c.heartbeat_interval_secs, 5);
        assert_eq!(c.suspect_multiplier, 2);
        assert_eq!(c.reconnect_attempts, 5);
        assert_eq!(c.rate_limit_per_sec, 100);
        assert_eq!(c.rate_limit_burst, 500);
        assert_eq!(c.transport, "nativep2p");
    }

    #[test]
    fn role_parse_round_trip() {
        assert_eq!(SyncRole::parse("replicator").unwrap(), SyncRole::Replicator);
        assert_eq!(SyncRole::parse("Replicator").unwrap(), SyncRole::Replicator);
        assert_eq!(SyncRole::parse("observer").unwrap(), SyncRole::Observer);
        assert_eq!(SyncRole::parse("Observer").unwrap(), SyncRole::Observer);
        assert!(SyncRole::parse("validator").is_err());
    }

    #[test]
    fn builder_pattern() {
        let c = SyncConfig::new([1u8; 32], SyncRole::Observer, vec![2u8; 32])
            .with_writer_node_id([3u8; 32])
            .with_transport("webhook");
        assert_eq!(c.mission_id, [1u8; 32]);
        assert_eq!(c.role, SyncRole::Observer);
        assert_eq!(c.writer_node_id, Some([3u8; 32]));
        assert_eq!(c.transport, "webhook");
    }
}
