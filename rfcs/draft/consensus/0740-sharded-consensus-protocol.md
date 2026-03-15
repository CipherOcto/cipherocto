# RFC-0740 (Consensus): Sharded Consensus Protocol

## Status

Draft

> **Note:** This RFC was renumbered from RFC-0140 to RFC-0740 as part of the category-based numbering system.

## Summary

This RFC defines the **Sharded Consensus Protocol** — a mechanism for organizing the Proof-of-Inference network into parallel shards that process distinct subsets of inference tasks. Each shard maintains its own state and processes consensus independently, enabling horizontal scaling of throughput while maintaining security through cross-shard verification and state commitment.

## Design Goals

| Goal                             | Target                      | Metric            |
| -------------------------------- | --------------------------- | ----------------- |
| **G1: Horizontal Scaling**       | Linear with shards          | O(shards)         |
| **G2: Shard Independence**       | No cross-shard coordination | Isolated          |
| **G3: Cross-Shard Verification** | Fraud proofs                | Challenge period  |
| **G4: State Commitment**         | Merkle roots per shard      | O(1) verification |

## Motivation

### CAN WE? — Feasibility Research

The fundamental question: **Can we shard a PoI consensus network without centralization?**

Challenges:

- Cross-shard state consistency
- Fraud proof generation
- Validator assignment
- Economic security per shard

Feasibility established through:

- Ethereum 2.0 shard chain model
- Optimistic rollups fraud proofs
- Beacon chain coordination
- Token-curated registries

### WHY? — Why This Matters

Without sharding:

- Network throughput limited by single chain
- All nodes validate all transactions
- Scale瓶颈 at consensus layer

With sharding:

- Parallel processing
- Partial node participation
- Global throughput scales

### WHAT? — What This Specifies

1. Shard definition and boundaries
2. Validator assignment
3. Cross-shard communication
4. State commitments
5. Fraud proof mechanism

## Specification

### Shard Architecture

```rust
/// Shard definition
struct Shard {
    /// Shard ID
    shard_id: ShardId,

    /// Model IDs processed by this shard
    model_ids: Vec<Digest>,

    /// Validator set
    validators: Vec<Validator>,

    /// Current state root
    state_root: Digest,

    /// Block height
    height: u64,
}

/// Shard configuration
struct ShardConfig {
    /// Number of shards
    shard_count: u32,

    /// Models per shard
    models_per_shard: u32,

    /// Validator rotation period
    rotation_period: u64,

    /// Challenge period (blocks)
    challenge_period: u32,
}
```

### Validator Assignment

```rust
/// Validator assignment
struct ValidatorAssignment {
    /// Validator
    validator: PublicKey,

    /// Assigned shards
    shards: Vec<ShardId>,

    /// Assignment start epoch
    start_epoch: u64,

    /// Assignment end epoch
    end_epoch: u64,
}

/// Beacon chain (shard coordinator)
struct BeaconChain {
    /// Current epoch
    epoch: u64,

    /// Validator registry
    validators: HashMap<PublicKey, ValidatorInfo>,

    /// Shard assignments
    assignments: HashMap<ShardId, Vec<PublicKey>>,
}
```

### Cross-Shard Communication

```rust
/// Cross-shard message
struct CrossShardMessage {
    /// Source shard
    source_shard: ShardId,

    /// Destination shard
    dest_shard: ShardId,

    /// Message type
    msg_type: CrossShardMsgType,

    /// Payload
    payload: Vec<u8>,

    /// Nonce (ordering)
    nonce: u64,
}

enum CrossShardMsgType {
    /// State sync
    StateSync,
    /// Fraud proof
    FraudProof,
    /// Token transfer
    Transfer,
}
```

### State Commitment

```rust
/// Shard state
struct ShardState {
    /// State Merkle root
    state_root: Digest,

    /// Block receipts
    receipts: Vec<Receipt>,

    /// Pending cross-shard messages
    pending_messages: Vec<CrossShardMessage>,
}

/// State commitment
struct StateCommitment {
    /// Shard ID
    shard_id: ShardId,

    /// State root
    state_root: Digest,

    /// Block number
    block_number: u64,

    /// Signature
    signature: Signature,
}
```

### Fraud Proofs

```rust
/// Fraud proof
struct FraudProof {
    /// Shard ID
    shard_id: ShardId,

    /// Invalid transaction index
    tx_index: u32,

    /// Pre-state root
    pre_state_root: Digest,

    /// Post-state root
    post_state_root: Digest,

    /// Proof data
    proof_data: Vec<u8>,

    /// Challenger
    challenger: PublicKey,
}

/// Fraud proof verification
impl FraudProof {
    fn verify(&self) -> bool {
        // Verify state transition was invalid
    }
}
```

## Performance Targets

| Metric               | Target |
| -------------------- | ------ |
| Shards               | 16-256 |
| Validators per shard | 100+   |
| Cross-shard latency  | <5s    |
| Fraud proof window   | 7 days |

## Related RFCs

- RFC-0630 (Proof Systems): Proof-of-Inference Consensus
- RFC-0143 (Networking): OCTO-Network Protocol
- RFC-0141 (Consensus): Parallel Block DAG
- RFC-0142 (Consensus): Data Availability & Sampling
