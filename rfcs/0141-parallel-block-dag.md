# RFC-0141: Parallel Block DAG Specification

## Status

Draft

## Summary

This RFC defines the **Parallel Block DAG** — a blockdag structure that replaces the traditional linear blockchain. Instead of a single chain, blocks reference multiple parent blocks, enabling parallel block production and confirmation. The DAG organizes blocks by shard and global ordering, supporting high throughput while maintaining eventual consistency.

## Design Goals

| Goal                        | Target            | Metric        |
| --------------------------- | ----------------- | ------------- |
| **G1: Parallel Production** | Concurrent blocks | No fork limit |
| **G2: Fast Confirmation**   | <10s finality     | DAG ordering  |
| **G3: Shard Support**       | Per-shard DAGs    | Isolation     |
| **G4: Ordering**            | Causal + total    | Consensus     |

## Motivation

Linear blockchains limit throughput. DAG enables:

- Parallel block production
- Higher transaction throughput
- Faster confirmation

## Specification

### DAG Structure

```rust
/// Block in DAG
struct DAGBlock {
    /// Block ID
    block_id: Digest,

    /// Parent block IDs
    parents: Vec<Digest>,

    /// Shard ID (if applicable)
    shard_id: Option<ShardId>,

    /// Payload
    payload: DAGPayload,

    /// Timestamp
    timestamp: u64,

    /// Producer
    producer: PublicKey,
}

/// DAG payload
enum DAGPayload {
    /// Inference block
    Inference(InferenceBatch),

    /// Checkpoint
    Checkpoint(Checkpoint),

    /// Cross-shard message
    CrossShard(CrossShardMessage),
}
```

### Block Ordering

```rust
/// DAG ordering
struct DAGOrdering {
    /// Topological order
    ordering: Vec<Digest>,

    /// Causal dependencies
    causal: HashMap<Digest, Vec<Digest>>,
}

impl DAGOrdering {
    /// Compute topological order
    fn order(blocks: &[DAGBlock]) -> Vec<Digest> {
        // Kahn's algorithm
    }

    /// Add block
    fn add_block(&mut self, block: DAGBlock) {
        // Update dependencies
    }
}
```

### Confirmation

```rust
/// Block confirmation
impl DAGBlock {
    /// Check if confirmed
    fn is_confirmed(&self, tips: &[DAGBlock], confirmations: u32) -> bool {
        // Count supermajority of descendants
    }
}
```

## Performance Targets

| Metric            | Target    |
| ----------------- | --------- |
| Blocks/second     | 1000+     |
| Confirmation time | <10s      |
| DAG depth         | Unlimited |

## Related RFCs

- RFC-0130: Proof-of-Inference Consensus
- RFC-0140: Sharded Consensus Protocol
- RFC-0143: OCTO-Network Protocol
