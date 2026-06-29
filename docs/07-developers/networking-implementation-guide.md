# Networking Implementation Guide

## Overview

This guide provides concrete implementation details for the CipherOcto Deterministic Overlay Transport (DOT) networking layer, defined in RFCs 0850-0860. It bridges the gap between protocol specification and working Rust code.

**Prerequisites:** Familiarity with RFCs 0850-0860, the CipherOcto crate structure, and Rust async programming.

**Existing code:** `crates/octo-network/src/network.rs` provides a minimal Phase 1 peer list. The DOT layer builds on top of this foundation.

---

## Module Tree

```
crates/octo-network/src/
├── lib.rs                          # Re-exports (existing)
├── network.rs                      # Phase 1 peer list (existing)
│
├── dot/                            # RFC-0850: Deterministic Overlay Transport
│   ├── mod.rs                      # DOT module root
│   ├── envelope.rs                 # DeterministicEnvelope
│   ├── domain.rs                   # BroadcastDomainId
│   ├── gateway.rs                  # GatewayIdentity, GatewayCapacity
│   ├── sequence.rs                 # OverlaySequence, logical timestamps
│   ├── replay.rs                   # ReplayCache
│   ├── fragment.rs                 # EnvelopeFragment, reassembly
│   ├── error.rs                    # DotError enum
│   └── adapters/
│       ├── mod.rs                  # PlatformAdapter trait
│       ├── native_p2p.rs           # libp2p gossipsub adapter
│       ├── telegram.rs             # Telegram Bot API adapter
│       ├── discord.rs              # Discord webhook adapter
│       └── matrix.rs               # Matrix room adapter
│
├── gdp/                            # RFC-0851: Gateway Discovery Protocol
│   ├── mod.rs                      # GDP module root
│   ├── advertisement.rs            # GatewayAdvertisement
│   ├── capabilities.rs             # GatewayCapability, Merkle commitment
│   ├── heartbeat.rs                # GatewayHeartbeat, failure detection
│   ├── cache.rs                    # GatewayCache with deterministic eviction
│   ├── error.rs                    # GdpError enum
│   └── discovery.rs                # Discovery lifecycle (bootstrap, expansion)
│
├── dgp/                            # RFC-0852: Deterministic Gossip Protocol
│   ├── mod.rs                      # DGP module root
│   ├── object.rs                   # GossipObject
│   ├── domain.rs                   # GossipDomainId
│   ├── dedup.rs                    # Deduplication (seen hashes)
│   ├── ordering.rs                 # Canonical processing order
│   ├── anti_entropy.rs             # Merkle reconciliation
│   ├── modes/
│   │   ├── mod.rs                  # GossipMode trait
│   │   ├── flood.rs                # Flood gossip
│   │   ├── incremental.rs          # Incremental gossip
│   │   └── directed.rs             # Directed gossip
│   └── error.rs                    # DgpError enum
│
├── ocrypt/                         # RFC-0853: Overlay Cryptography
│   ├── mod.rs                      # OCrypt module root
│   ├── suite.rs                    # CryptoSuiteId
│   ├── identity.rs                 # Sovereign identity (extends octo-core)
│   ├── session.rs                  # Session handshake (X25519 + HKDF)
│   ├── mission_keys.rs             # Mission key hierarchy
│   ├── envelope_crypto.rs          # Envelope encryption/decryption
│   └── error.rs                    # CryptoError enum
│
├── dps/                            # RFC-0854: Deterministic Proof Substrate
│   ├── mod.rs                      # DPS module root
│   ├── trait_def.rs                # DeterministicProofSystem trait
│   ├── suite.rs                    # ProofSuiteId, ProofExecutionClass
│   ├── backends/
│   │   ├── mod.rs                  # Backend registry
│   │   ├── stark.rs                # STARK (STWO) backend
│   │   ├── plonk.rs                # PLONK backend
│   │   └── risc0.rs                # RISC0 zkVM backend
│   ├── witness.rs                  # WitnessGenerator trait
│   └── error.rs                    # ProofError enum
│
├── mon/                            # RFC-0855: Mission Overlay Networks
│   ├── mod.rs                      # MON module root
│   ├── mission.rs                  # MissionId, MissionDescriptor
│   ├── lifecycle.rs                # MissionStateMachine (8 states)
│   ├── membership.rs               # MissionNode, roles
│   ├── topology.rs                 # Topology models (Mesh, Star, etc.)
│   ├── key_hierarchy.rs            # Mission key derivation
│   └── error.rs                    # MonError enum
│
├── drs/                            # RFC-0856: Deterministic Route Selection
│   ├── mod.rs                      # DRS module root
│   ├── route.rs                    # DeterministicRoute
│   ├── scoring.rs                  # Canonical scoring formula
│   ├── trust.rs                    # TrustScore computation
│   ├── cache.rs                    # RouteCache with deterministic eviction
│   └── error.rs                    # DrsError enum
│
├── dom/                            # RFC-0857: Deterministic Overlay Mempool
│   ├── mod.rs                      # DOM module root
│   ├── intent.rs                   # OverlayIntent, IntentType
│   ├── ordering.rs                 # Canonical ordering (class, weight, ts)
│   ├── admission.rs                # Canonical admission rules
│   ├── eviction.rs                 # Deterministic eviction
│   ├── mempool.rs                  # Mempool implementation
│   └── error.rs                    # DomError enum
│
├── orr/                            # RFC-0858: Onion Relay Routing
│   ├── mod.rs                      # ORR module root
│   ├── route.rs                    # OnionRoute, OnionHop
│   ├── construction.rs             # Layered encryption construction
│   ├── peeling.rs                  # Layer peeling (decryption)
│   ├── cover_traffic.rs            # Cover traffic generation
│   └── error.rs                    # OrrError enum
│
├── pce/                            # RFC-0859: Proof-Carrying Envelopes
│   ├── mod.rs                      # PCE module root
│   ├── envelope.rs                 # ProofCarryingEnvelope
│   ├── verification.rs             # Proof verification pipeline
│   ├── boundary.rs                 # Canonical proof boundary
│   └── error.rs                    # PceError enum
│
└── porelay/                        # RFC-0860: Proof-of-Relay
    ├── mod.rs                      # PoRelay module root
    ├── proofs.rs                   # RelayProof types
    ├── heartbeat.rs                # GatewayHeartbeat
    ├── trust.rs                    # TrustScore computation
    ├── slashing.rs                 # Slashing conditions
    └── error.rs                    # PoRelayError enum
```

---

## Error Types

### DOT Error (RFC-0850)

```rust
// crates/octo-network/src/dot/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DotError {
    #[error("Invalid signature on envelope {envelope_id:?}")]
    InvalidSignature { envelope_id: [u8; 32] },

    #[error("Envelope {envelope_id:?} already seen at epoch {first_seen}")]
    ReplayDetected { envelope_id: [u8; 32], first_seen: u64 },

    #[error("Payload hash mismatch: expected {expected:?}, got {actual:?}")]
    PayloadHashMismatch { expected: [u8; 32], actual: [u8; 32] },

    #[error("Envelope expired: ttl_hops={ttl}, current_hops={hops}")]
    TtlExpired { ttl: u16, hops: u16 },

    #[error("Platform adapter error: {0}")]
    PlatformAdapter(#[from] PlatformAdapterError),

    #[error("Fragment reassembly timeout for envelope {envelope_id:?}")]
    FragmentTimeout { envelope_id: [u8; 32] },

    #[error("Canonical serialization error: {0}")]
    Serialization(String),

    #[error("Consensus boundary violation: {operation}")]
    ConsensusBoundaryViolation { operation: String },
}

#[derive(Debug, Error)]
pub enum PlatformAdapterError {
    #[error("Platform {platform} unreachable: {reason}")]
    Unreachable { platform: String, reason: String },

    #[error("Payload too large for platform {platform}: {size} > {max}")]
    PayloadTooLarge { platform: String, size: usize, max: usize },

    #[error("Rate limited by platform {platform}, retry after {retry_after_ms}ms")]
    RateLimited { platform: String, retry_after_ms: u64 },

    #[error("Platform API error: {code} {message}")]
    ApiError { code: u16, message: String },
}
```

### GDP Error (RFC-0851)

```rust
// crates/octo-network/src/gdp/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GdpError {
    #[error("Invalid advertisement: {reason}")]
    InvalidAdvertisement { reason: String },

    #[error("Gateway {gateway_id:?} not found in cache")]
    GatewayNotFound { gateway_id: [u8; 32] },

    #[error("Capability mismatch: required {required:?}, available {available:?}")]
    CapabilityMismatch { required: u64, available: u64 },

    #[error("Advertisement sequence too old: got {got}, minimum {minimum}")]
    StaleSequence { got: u64, minimum: u64 },

    #[error("Heartbeat timeout for gateway {gateway_id:?} after {missed} missed")]
    HeartbeatTimeout { gateway_id: [u8; 32], missed: u32 },

    #[error("Signature verification failed: {0}")]
    SignatureError(String),
}
```

### Crypto Error (RFC-0853)

```rust
// crates/octo-network/src/ocrypt/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("Invalid key length: expected {expected}, got {got}")]
    InvalidKeyLength { expected: usize, got: usize },

    #[error("Nonce reuse detected for session {session_id:?}")]
    NonceReuse { session_id: [u8; 32] },

    #[error("Decryption failed: {reason}")]
    DecryptionFailed { reason: String },

    #[error("Key derivation failed: {context}")]
    KeyDerivationFailed { context: String },

    #[error("Consensus boundary violation: {operation}")]
    ConsensusBoundaryViolation { operation: String },

    #[error("Unsupported crypto suite: {suite_id}")]
    UnsupportedSuite { suite_id: u16 },
}
```

### Proof Error (RFC-0854)

```rust
// crates/octo-network/src/dps/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProofError {
    #[error("Invalid witness: {reason}")]
    InvalidWitness { reason: &'static str },

    #[error("Trace mismatch: expected {expected:?}, computed {computed:?}")]
    TraceMismatch { expected: [u8; 32], computed: [u8; 32] },

    #[error("Proof generation failed in backend {backend}: {detail}")]
    ProofGenerationFailed { backend: &'static str, detail: &'static str },

    #[error("Verification failed")]
    VerificationFailed,

    #[error("Invalid verification key")]
    InvalidVerificationKey,

    #[error("Unsupported proof system: {suite_id:?}")]
    UnsupportedProofSystem { suite_id: crate::dps::suite::ProofSuiteId },

    #[error("Consensus boundary violation: {operation}")]
    ConsensusBoundaryViolation { operation: &'static str },
}
```

---

## Core Type Implementations

### BroadcastDomainId (RFC-0850)

```rust
// crates/octo-network/src/dot/domain.rs
use blake3::Hash;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct BroadcastDomainId {
    pub platform_type: u16,
    pub domain_hash: [u8; 32],
}

#[repr(u16)]
pub enum PlatformType {
    Telegram = 0x0001,
    Discord = 0x0002,
    Matrix = 0x0003,
    Nostr = 0x0004,
    Signal = 0x0005,
    IRC = 0x0006,
    Slack = 0x0007,
    NativeP2P = 0x000A,
}

impl BroadcastDomainId {
    /// Create a new domain ID from platform type and identifier.
    ///
    /// Determinism: domain_hash = BLAKE3-256(normalized_platform_id)
    /// Platform IDs MUST be lowercase, trimmed before hashing.
    pub fn new(platform_type: PlatformType, platform_id: &str) -> Self {
        let normalized = platform_id.trim().to_lowercase();
        let hash = blake3::hash(normalized.as_bytes());
        Self {
            platform_type: platform_type as u16,
            domain_hash: *hash.as_bytes(),
        }
    }

    /// Serialize to canonical bytes (RFC-0126 DCS).
    /// Order: platform_type (2 bytes, big-endian) || domain_hash (32 bytes)
    pub fn to_canonical_bytes(&self) -> [u8; 34] {
        let mut buf = [0u8; 34];
        buf[0..2].copy_from_slice(&self.platform_type.to_be_bytes());
        buf[2..34].copy_from_slice(&self.domain_hash);
        buf
    }

    /// Deserialize from canonical bytes.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, crate::dot::error::DotError> {
        if bytes.len() < 34 {
            return Err(crate::dot::error::DotError::Serialization(
                "BroadcastDomainId requires 34 bytes".into(),
            ));
        }
        let platform_type = u16::from_be_bytes([bytes[0], bytes[1]]);
        let mut domain_hash = [0u8; 32];
        domain_hash.copy_from_slice(&bytes[2..34]);
        Ok(Self { platform_type, domain_hash })
    }
}
```

### DeterministicEnvelope (RFC-0850)

```rust
// crates/octo-network/src/dot/envelope.rs
use blake3;

#[derive(Clone, Debug)]
#[repr(C)]
pub struct DeterministicEnvelope {
    pub version: u16,
    pub network_id: u32,
    pub message_type: u16,
    pub envelope_id: [u8; 32],
    pub mission_id: [u8; 32],
    pub source_peer: [u8; 32],
    pub origin_gateway: [u8; 32],
    pub logical_timestamp: u64,
    pub ttl_hops: u16,
    pub payload_hash: [u8; 32],
    pub route_trace_root: [u8; 32],
    pub flags: u64,
    pub signature: [u8; 64],
}

impl DeterministicEnvelope {
    /// Derive envelope_id from canonical fields.
    ///
    /// envelope_id = BLAKE3-256(
    ///     version || network_id || message_type ||
    ///     source_peer || origin_gateway || logical_timestamp || payload_hash
    /// )
    pub fn derive_envelope_id(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.version.to_be_bytes());
        hasher.update(&self.network_id.to_be_bytes());
        hasher.update(&self.message_type.to_be_bytes());
        hasher.update(&self.source_peer);
        hasher.update(&self.origin_gateway);
        hasher.update(&self.logical_timestamp.to_be_bytes());
        hasher.update(&self.payload_hash);
        *hasher.finalize().as_bytes()
    }

    /// Serialize to canonical bytes for signing/verification.
    /// Excludes signature field.
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256);
        buf.extend_from_slice(&self.version.to_be_bytes());
        buf.extend_from_slice(&self.network_id.to_be_bytes());
        buf.extend_from_slice(&self.message_type.to_be_bytes());
        buf.extend_from_slice(&self.envelope_id);
        buf.extend_from_slice(&self.mission_id);
        buf.extend_from_slice(&self.source_peer);
        buf.extend_from_slice(&self.origin_gateway);
        buf.extend_from_slice(&self.logical_timestamp.to_be_bytes());
        buf.extend_from_slice(&self.ttl_hops.to_be_bytes());
        buf.extend_from_slice(&self.payload_hash);
        buf.extend_from_slice(&self.route_trace_root);
        buf.extend_from_slice(&self.flags.to_be_bytes());
        buf
    }

    /// Verify envelope integrity.
    pub fn verify(&self, public_key: &[u8; 32]) -> Result<(), crate::dot::error::DotError> {
        // 1. Verify envelope_id derivation
        let expected_id = self.derive_envelope_id();
        if self.envelope_id != expected_id {
            return Err(crate::dot::error::DotError::PayloadHashMismatch {
                expected: expected_id,
                actual: self.envelope_id,
            });
        }

        // 2. Verify signature (Ed25519)
        let signing_bytes = self.to_signing_bytes();
        let signature = ed25519_dalek::Signature::from_bytes(&self.signature);
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(public_key)
            .map_err(|e| crate::dot::error::DotError::Serialization(e.to_string()))?;
        verifying_key
            .verify(&signing_bytes, &signature)
            .map_err(|_| crate::dot::error::DotError::InvalidSignature {
                envelope_id: self.envelope_id,
            })
    }
}
```

### ReplayCache (RFC-0850)

```rust
// crates/octo-network/src/dot/replay.rs
use std::collections::BTreeMap;

pub struct ReplayCache {
    seen: BTreeMap<[u8; 32], u64>,
    window_duration: u64,
    max_entries: u32,
}

impl ReplayCache {
    pub fn new(window_duration: u64, max_entries: u32) -> Self {
        Self {
            seen: BTreeMap::new(),
            window_duration,
            max_entries,
        }
    }

    /// Check if envelope_id is a replay. If not, insert it.
    ///
    /// Determinism: BTreeMap provides sorted iteration order.
    /// Eviction removes the entry with smallest first_seen timestamp.
    /// If timestamps are equal, lexicographic envelope_id is the tiebreaker.
    pub fn check_and_insert(
        &mut self,
        envelope_id: [u8; 32],
        current_epoch: u64,
    ) -> Result<(), crate::dot::error::DotError> {
        if let Some(&first_seen) = self.seen.get(&envelope_id) {
            return Err(crate::dot::error::DotError::ReplayDetected {
                envelope_id,
                first_seen,
            });
        }

        // Evict if at capacity
        if self.seen.len() >= self.max_entries as usize {
            self.evict_oldest(current_epoch);
        }

        self.seen.insert(envelope_id, current_epoch);
        Ok(())
    }

    fn evict_oldest(&mut self, current_epoch: u64) {
        // Remove entries outside the replay window
        let cutoff = current_epoch.saturating_sub(self.window_duration);
        self.seen.retain(|_, &mut ts| ts > cutoff);

        // If still at capacity, remove the oldest entry (BTreeMap is sorted)
        if self.seen.len() >= self.max_entries as usize {
            if let Some(oldest_key) = self.seen.keys().next().copied() {
                self.seen.remove(&oldest_key);
            }
        }
    }
}
```

### OverlaySequence (RFC-0850)

```rust
// crates/octo-network/src/dot/sequence.rs

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(C)]
pub struct OverlaySequence {
    pub epoch: u64,
    pub gateway: [u8; 32],
    pub monotonic_counter: u64,
}

impl OverlaySequence {
    /// Create a new sequence for a gateway at the given epoch.
    pub fn new(epoch: u64, gateway: [u8; 32], counter: u64) -> Self {
        Self {
            epoch,
            gateway,
            monotonic_counter: counter,
        }
    }

    /// Compare two sequences deterministically.
    /// Order: (epoch, monotonic_counter, gateway_id)
    pub fn canonical_cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.epoch
            .cmp(&other.epoch)
            .then(self.monotonic_counter.cmp(&other.monotonic_counter))
            .then(self.gateway.cmp(&other.gateway))
    }
}
```

---

## Trait Definitions

### PlatformAdapter (RFC-0850)

```rust
// crates/octo-network/src/dot/adapters/mod.rs
use async_trait::async_trait;

#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    /// Send a deterministic envelope to the platform.
    async fn send_message(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError>;

    /// Receive raw messages from the platform.
    async fn receive_messages(
        &self,
        domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError>;

    /// Convert platform-specific message to canonical envelope.
    fn canonicalize(
        &self,
        raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError>;

    /// Report platform capabilities.
    fn capabilities(&self) -> CapabilityReport;

    /// Compute deterministic domain ID from platform-specific identifier.
    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId;
}

pub struct DeliveryReceipt {
    pub platform_message_id: String,
    pub delivered_at: u64,
}

pub struct RawPlatformMessage {
    pub platform_id: String,
    pub payload: Vec<u8>,
    pub metadata: std::collections::HashMap<String, String>,
}

pub struct CapabilityReport {
    pub max_payload_bytes: usize,
    pub supports_fragmentation: bool,
    pub supports_encryption: bool,
    pub rate_limit_per_second: u32,
}
```

### DeterministicProofSystem (RFC-0854)

```rust
// crates/octo-network/src/dps/trait_def.rs

pub trait DeterministicProofSystem {
    type Proof;
    type VerificationKey;
    type PublicInputs;
    type Witness;

    /// Generate a proof given witness data, trace commitment, and public inputs.
    fn prove(
        witness: &Self::Witness,
        trace_commitment: [u8; 32],
        public_inputs: Self::PublicInputs,
    ) -> Result<Self::Proof, crate::dps::error::ProofError>;

    /// Verify a proof — MUST be deterministic across all implementations.
    fn verify(
        vk: &Self::VerificationKey,
        public_inputs: &Self::PublicInputs,
        proof: &Self::Proof,
    ) -> Result<bool, crate::dps::error::ProofError>;

    /// Compute proof commitment (hash of proof for Merkle trees).
    fn proof_commitment(proof: &Self::Proof) -> [u8; 32];

    /// Return the execution model for this proof system.
    fn execution_model() -> ProofExecutionClass;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofExecutionClass {
    ClassA,
    ClassB,
    ClassC,
}
```

---

## Canonical Scoring (RFC-0856)

```rust
// crates/octo-network/src/drs/scoring.rs

/// Network-level weight constants (set at genesis, modifiable via governance).
pub struct RouteWeights {
    pub trust: u64,
    pub bandwidth: u64,
    pub latency: u64,
    pub censorship_resistance: u64,
}

impl Default for RouteWeights {
    fn default() -> Self {
        Self {
            trust: 50,
            bandwidth: 30,
            latency: 20,
            censorship_resistance: 0,
        }
    }
}

/// Compute deterministic route score using u64 arithmetic.
///
/// score = (trust_weight * trust_score) +
///         (bandwidth_weight * bandwidth_class) +
///         (latency_weight * latency_class) +
///         (cr_weight * censorship_resistance_class) -
///         (route_cost * 100)
///
/// All inputs are u64. Overflow handled via saturating_mul.
pub fn compute_route_score(
    route: &crate::drs::route::DeterministicRoute,
    weights: &RouteWeights,
) -> u64 {
    let trust_component = weights
        .trust
        .saturating_mul(route.trust_score as u64);
    let bandwidth_component = weights
        .bandwidth
        .saturating_mul(route.bandwidth_class as u64);
    let latency_component = weights
        .latency
        .saturating_mul(route.latency_class as u64);
    let cr_component = weights
        .censorship_resistance
        .saturating_mul(route.censorship_resistance_class as u64);
    let cost_component = 100u64.saturating_mul(route.route_cost as u64);

    trust_component
        .saturating_add(bandwidth_component)
        .saturating_add(latency_component)
        .saturating_add(cr_component)
        .saturating_sub(cost_component)
}

/// Canonical route ordering: (score DESC, epoch ASC, route_id ASC)
pub fn canonical_route_cmp(
    a: &crate::drs::route::DeterministicRoute,
    b: &crate::drs::route::DeterministicRoute,
    weights: &RouteWeights,
) -> std::cmp::Ordering {
    let score_a = compute_route_score(a, weights);
    let score_b = compute_route_score(b, weights);
    score_b
        .cmp(&score_a) // Higher score first
        .then(a.route_epoch.cmp(&b.route_epoch))
        .then(a.route_id.cmp(&b.route_id))
}
```

---

## Integration with Existing Network

```rust
// crates/octo-network/src/network.rs (extended)
use crate::dot::{DeterministicEnvelope, BroadcastDomainId};
use crate::dot::adapters::PlatformAdapter;
use std::sync::Arc;

pub struct Network {
    peers: RwLock<Vec<String>>,
    // New: DOT gateway
    gateway: Option<Arc<DotGateway>>,
}

pub struct DotGateway {
    identity: crate::dot::gateway::GatewayIdentity,
    adapters: Vec<Arc<dyn PlatformAdapter>>,
    replay_cache: RwLock<crate::dot::replay::ReplayCache>,
}

impl DotGateway {
    pub async fn process_envelope(
        &self,
        envelope: &DeterministicEnvelope,
    ) -> Result<ProcessingResult, crate::dot::error::DotError> {
        // 1. Verify signature (Class A)
        envelope.verify(&self.identity.public_key)?;

        // 2. Check replay cache (Class A)
        let mut cache = self.replay_cache.write().await;
        cache.check_and_insert(envelope.envelope_id, envelope.logical_timestamp)?;

        // 3. Forward to all adapters
        for adapter in &self.adapters {
            for domain in self.connected_domains() {
                adapter.send_message(&domain, envelope).await?;
            }
        }

        Ok(ProcessingResult::Forwarded)
    }
}
```

---

## Config Schema

```yaml
# octo-network.yaml
dot:
  network_id: 1
  gateway:
    class: Edge
    creation_epoch: 0

  replay_cache:
    max_entries: 100000
    window_duration_secs: 3600

  platforms:
    telegram:
      enabled: true
      bot_token: "${TELEGRAM_BOT_TOKEN}"
      groups:
        - "-1001234567890"
    discord:
      enabled: true
      webhook_url: "${DISCORD_WEBHOOK_URL}"
    matrix:
      enabled: false
    native_p2p:
      enabled: true
      listen_addr: "/ip4/0.0.0.0/tcp/4001"

gdp:
  heartbeat_interval_secs: 30
  missed_heartbeats_threshold: 3
  cache_max_entries: 10000

dgp:
  max_gossip_objects: 50000
  anti_entropy_interval_secs: 60

drs:
  weights:
    trust: 50
    bandwidth: 30
    latency: 20
    censorship_resistance: 0

dom:
  max_pending_intents: 100000
  max_per_mission: 10000

orr:
  default_hop_count: 3
  cover_traffic_ratio: 0.20
```

---

## Testing Strategy

### Unit Tests (per module)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_broadcast_domain_id_deterministic() {
        let id1 = BroadcastDomainId::new(PlatformType::Telegram, "-1001234567890");
        let id2 = BroadcastDomainId::new(PlatformType::Telegram, "-1001234567890");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_broadcast_domain_id_case_insensitive() {
        let id1 = BroadcastDomainId::new(PlatformType::Telegram, "-1001234567890");
        let id2 = BroadcastDomainId::new(PlatformType::Telegram, "-1001234567890 ");
        assert_eq!(id1, id2); // trimmed
    }

    #[test]
    fn test_replay_cache_detects_replay() {
        let mut cache = ReplayCache::new(3600, 1000);
        let envelope_id = [0x01; 32];
        assert!(cache.check_and_insert(envelope_id, 100).is_ok());
        assert!(cache.check_and_insert(envelope_id, 101).is_err());
    }

    #[test]
    fn test_overlay_sequence_ordering() {
        let a = OverlaySequence::new(1, [0x01; 32], 100);
        let b = OverlaySequence::new(1, [0x02; 32], 100);
        assert!(a.canonical_cmp(&b) == std::cmp::Ordering::Less);
    }

    #[test]
    fn test_route_score_deterministic() {
        let route = DeterministicRoute { /* ... */ };
        let weights = RouteWeights::default();
        let score1 = compute_route_score(&route, &weights);
        let score2 = compute_route_score(&route, &weights);
        assert_eq!(score1, score2);
    }

    #[test]
    fn test_route_score_no_overflow() {
        let route = DeterministicRoute {
            trust_score: u16::MAX,
            bandwidth_class: u16::MAX,
            // ...
        };
        let weights = RouteWeights {
            trust: u64::MAX,
            // ...
        };
        // Should not panic due to saturating_mul
        let _score = compute_route_score(&route, &weights);
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_envelope_round_trip() {
    let gateway = DotGateway::new(test_config());
    let envelope = create_test_envelope();
    let result = gateway.process_envelope(&envelope).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_multi_platform_propagation() {
    let telegram = MockTelegramAdapter::new();
    let discord = MockDiscordAdapter::new();
    let gateway = DotGateway::with_adapters(vec![telegram, discord]);
    let envelope = create_test_envelope();
    gateway.process_envelope(&envelope).await.unwrap();
    assert_eq!(telegram.sent_count(), 1);
    assert_eq!(discord.sent_count(), 1);
}
```

---

## Cargo Dependencies

```toml
# crates/octo-network/Cargo.toml additions
[dependencies]
blake3 = "1.5"
ed25519-dalek = "2.1"
x25519-dalek = "2.0"
chacha20poly1305 = "0.10"
hkdf = "0.12"
sha2 = "0.10"
serde = { version = "1.0", features = ["derive"] }
thiserror = "2.0"
async-trait = "0.1"
tokio = { version = "1", features = ["full"] }
anyhow = "1.0"

# Platform adapters (feature-gated)
[features]
default = ["native-p2p"]
native-p2p = ["libp2p"]
telegram = ["reqwest"]
discord = ["reqwest"]
matrix = ["reqwest"]
```
