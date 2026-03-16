# RFC-0741 (Consensus): Parallel Block DAG Specification

## Status

Draft

> **Note:** This RFC was renumbered from RFC-0141 to RFC-0741 as part of the category-based numbering system.

## Summary

This RFC defines the **Parallel Block DAG** — a blockdag structure that replaces the traditional linear blockchain. Instead of a single chain, blocks reference multiple parent blocks, enabling parallel block production and confirmation. The DAG organizes blocks by shard and global ordering, supporting high throughput while maintaining eventual consistency. The protocol uses a synchronous total order algorithm (Hashgraph-style) to achieve deterministic ordering without leader election.

## Design Goals

| Goal                           | Target            | Metric                   |
| ------------------------------ | ----------------- | ------------------------ |
| **G1: Parallel Production**    | Concurrent blocks | No fork limit            |
| **G2: Fast Confirmation**      | <10s finality     | DAG ordering             |
| **G3: Shard Support**          | Per-shard DAGs    | Isolation                |
| **G4: Deterministic Ordering** | No leader         | Byzantine fault tolerant |
| **G5: Throughput**             | 1000+ TPS         | Linear scaling           |

## Motivation

### CAN WE? — Feasibility Research

The fundamental question: **Can we achieve consensus without a single leader while maintaining deterministic ordering?**

Linear blockchain limitations:

| Limitation      | Impact              |
| --------------- | ------------------- |
| Single chain    | Serial processing   |
| Leader election | Centralization risk |
| Block time      | Throughput ceiling  |
| Fork resolution | Latency added       |

DAG-based solutions (Hashgraph, Avalanche, Phantom) demonstrate:

- Leaderless consensus achievable
- Parallel block production possible
- Sub-second finality feasible
- Byzantine fault tolerance maintained

### WHY? — Why This Matters

Current PoI consensus (RFC-0630) assumes linear block production. Problems:

| Problem           | Impact                       |
| ----------------- | ---------------------------- |
| Serial inference  | Limited throughput           |
| Block convergence | Network latency              |
| Single chain      | All nodes process all blocks |
| Leader bottleneck | Centralization pressure      |

DAG enables:

- **Parallel inference** — Multiple inference tasks simultaneously
- **No leader** — No single point of failure
- **Fast finality** — Blocks confirm in seconds
- **Horizontal scaling** — More nodes = more throughput

### WHAT? — What This Specifies

The Parallel Block DAG defines:

1. **DAG structure** — Block references and topology
2. **Hashgraph consensus** — Virtual voting for ordering
3. **Shard-level DAGs** — Per-model parallel processing
4. **Global checkpointing** — Cross-shard ordering
5. **Confirmation rules** — Deterministic finality
6. **Fork resolution** — Tie-breaking mechanisms

### HOW? — Implementation

Integration with existing stack:

```
RFC-0630 (Proof-of-Inference)
       ↓
RFC-0141 (Parallel Block DAG) ← NEW
       ↓
RFC-0143 (OCTO-Network)
```

## Specification

### DAG Structure

```rust
/// Block in DAG with full metadata
struct DAGBlock {
    /// Unique block identifier
    block_id: Digest,

    /// Parent block IDs (multiple for DAG)
    parents: Vec<Digest>,

    /// Self-parent (for virtual voting)
    self_parent: Digest,

    /// Creation timestamp
    timestamp: u64,

    /// Producer signature
    signature: Signature,

    /// Producer public key
    producer: PublicKey,

    /// Shard ID (if applicable)
    shard_id: Option<ShardId>,

    /// Block payload
    payload: DAGPayload,

    /// Received from network
    received_at: u64,
}

/// Extended block with consensus metadata
struct ExtendedBlock {
    /// Base block
    block: DAGBlock,

    /// Consensus round
    consensus_round: u64,

    /// Round received
    round_received: u64,

    /// topological index
    topological_index: u64,

    /// Witness status
    is_witness: bool,

    /// See-before relationships
    see_before: Vec<(Digest, Digest)>,
}

/// DAG payload types
enum DAGPayload {
    /// Inference batch for PoI
    Inference(InferenceBatch),

    /// Global checkpoint
    Checkpoint(DAGCheckpoint),

    /// Cross-shard message
    CrossShard(CrossShardMessage),

    /// Empty/filler block
    Empty,
}

/// Global checkpoint
struct DAGCheckpoint {
    /// Checkpoint ID
    checkpoint_id: Digest,

    /// Hash of all confirmed blocks
    block_merkle_root: Digest,

    /// State root after checkpoint
    state_root: Digest,

    /// Shard states
    shard_states: HashMap<ShardId, Digest>,

    /// Checkpoint number
    checkpoint_number: u64,
}
```

### Hashgraph Consensus Algorithm

```rust
/// Hashgraph state
struct HashgraphState {
    /// All known blocks
    blocks: HashMap<Digest, ExtendedBlock>,

    /// Round number
    round: u64,

    /// Famous witnesses (for ordering)
    witnesses: Vec<Digest>,

    /// Processed events
    processed: HashSet<Digest>,
}

impl HashgraphState {
    /// Add new block to hashgraph
    fn add_block(&mut self, block: DAGBlock) -> ExtendedBlock {
        let extended = ExtendedBlock {
            block,
            consensus_round: 0,
            round_received: self.round,
            topological_index: 0,
            is_witness: false,
            see_before: Vec::new(),
        };

        self.blocks.insert(extended.block.block_id, extended.clone());
        extended
    }

    /// Calculate see-before relationships
    fn compute_ancestry(&mut self, block: &ExtendedBlock) {
        // Determine what events each event sees
        // A sees B if there's a path from B to A through parents
    }

    /// Determine if block is a witness
    fn is_witness(&self, block: &ExtendedBlock) -> bool {
        // Witness = first block received in a round
        // Compare with self-parent's round
        block.round_received > block.block.self_parent_round
    }

    /// Run virtual voting to find famous witnesses
    fn vote_witnesses(&self, witness: &Digest, round: u32) -> bool {
        // 2/3 supermajority of witnesses in previous round
        // vote based on seeing other witnesses
        let total_witnesses = self.witnesses.len();
        let mut votes = 0;

        for other_witness in &self.witnesses {
            if self.sees(*other_witness, *witness) {
                votes += 1;
            }
        }

        votes * 3 >= total_witnesses * 2
    }

    /// Check if A sees B
    fn sees(&self, a: Digest, b: Digest) -> bool {
        // DFS from a to find b in ancestors
        let mut visited = HashSet::new();
        self.dfs_sees(a, b, &mut visited)
    }

    fn dfs_sees(&self, current: Digest, target: Digest, visited: &mut HashSet<Digest>) -> bool {
        if current == target {
            return true;
        }

        if visited.contains(&current) {
            return false;
        }

        visited.insert(current);

        if let Some(block) = self.blocks.get(&current) {
            // Check self-parent
            if self.dfs_sees(block.block.self_parent, target, visited) {
                return true;
            }

            // Check other parents
            for parent in &block.block.parents {
                if self.dfs_sees(*parent, target, visited) {
                    return true;
                }
            }
        }

        false
    }
}
```

### Block Ordering and Consensus

```rust
/// Consensus ordering result
struct ConsensusOrder {
    /// Ordered block IDs
    ordered_blocks: Vec<Digest>,

    /// Checkpoint
    checkpoint: Option<DAGCheckpoint>,
}

/// Consensus computer
struct ConsensusComputer {
    /// Hashgraph state
    hashgraph: HashgraphState,

    /// Threshold for confirmation
    fame_threshold: f64,
}

impl ConsensusComputer {
    /// Run consensus round
    fn compute_round(&mut self, round: u64) -> ConsensusOrder {
        // 1. Find witnesses in this round
        let witnesses = self.find_witnesses(round);

        // 2. Vote on witness fame
        for witness in &witnesses {
            let is_famous = self.vote_witness(witness, round);
            if is_famous {
                self.hashgraph.witnesses.push(*witness);
            }
        }

        // 3. Order blocks based on famous witnesses
        self.order_blocks()
    }

    /// Find witnesses for a round
    fn find_witnesses(&self, round: u64) -> Vec<Digest> {
        self.hashgraph.blocks
            .values()
            .filter(|b| b.round_received == round && b.is_witness)
            .map(|b| b.block.block_id)
            .collect()
    }

    /// Order blocks using consensus
    fn order_blocks(&self) -> ConsensusOrder {
        let mut ordered: Vec<Digest> = Vec::new();

        // Sort famous witnesses by hashgraph order
        let mut witnesses: Vec<_> = self.hashgraph.witnesses.clone();
        witnesses.sort_by(|a, b| a.cmp(b));

        // All blocks seen by each famous witness are now ordered
        for witness in witnesses {
            let blocks = self.get_blocks_seen_by(witness);
            for block in blocks {
                if !ordered.contains(&block) {
                    ordered.push(block);
                }
            }
        }

        ConsensusOrder {
            ordered_blocks: ordered,
            checkpoint: None,
        }
    }
}
```

### Shard-Level DAGs

```rust
/// Per-shard DAG manager
struct ShardDAG {
    /// Shard ID
    shard_id: ShardId,

    /// Local hashgraph
    hashgraph: HashgraphState,

    /// Consensus computer
    consensus: ConsensusComputer,

    /// Pending blocks from other shards
    cross_shard_blocks: Vec<DAGBlock>,
}

impl ShardDAG {
    /// Process new inference block
    fn process_inference(&mut self, batch: InferenceBatch) -> Result<DAGBlock, Error> {
        let block = DAGBlock {
            block_id: self.compute_block_id(&batch),
            parents: self.get_parent_ids(),
            self_parent: self.hashgraph.blocks.keys().last().copied().unwrap_or_default(),
            timestamp: current_timestamp(),
            signature: self.sign(&batch),
            producer: self.producer_key,
            shard_id: Some(self.shard_id),
            payload: DAGPayload::Inference(batch),
            received_at: current_timestamp(),
        };

        Ok(block)
    }

    /// Get parent block IDs
    fn get_parent_ids(&self) -> Vec<Digest> {
        // Use recent blocks as parents
        // Typically 2-4 parents for DAG structure
        let recent: Vec<_> = self.hashgraph.blocks
            .values()
            .rev()
            .take(3)
            .map(|b| b.block.block_id)
            .collect();

        recent
    }
}

/// Multi-shard DAG coordinator
struct DAGCoordinator {
    /// Per-shard DAGs
    shards: HashMap<ShardId, ShardDAG>,

    /// Global ordering
    global_order: ConsensusOrder,
}

impl DAGCoordinator {
    /// Create cross-shard checkpoint
    fn create_checkpoint(&mut self, checkpoint_num: u64) -> DAGCheckpoint {
        let mut shard_states = HashMap::new();

        for (shard_id, shard_dag) in &mut self.shards {
            let last_order = shard_dag.consensus.order_blocks();
            let state_root = self.compute_state_root(&last_order);
            shard_states.insert(*shard_id, state_root);
        }

        DAGCheckpoint {
            checkpoint_id: hash(&checkpoint_num),
            block_merkle_root: self.compute_merkle_root(),
            state_root: hash(&shard_states),
            shard_states,
            checkpoint_number: checkpoint_num,
        }
    }
}
```

### Confirmation Rules

```rust
/// Block confirmation status
enum ConfirmationStatus {
    /// Not yet confirmed
    Unconfirmed,
    /// Under voting
    Voting,
    /// Confirmed in ordering
    Confirmed,
    /// Checkpointed (final)
    Finalized,
}

impl DAGBlock {
    /// Check confirmation status
    fn get_confirmation(
        &self,
        hashgraph: &HashgraphState,
        consensus_order: &ConsensusOrder,
    ) -> ConfirmationStatus {
        // Check if in consensus order
        if consensus_order.ordered_blocks.contains(&self.block_id) {
            // Check if checkpointed
            return ConfirmationStatus::Confirmed;
        }

        // Check if ancestor of any ordered block
        for ordered in &consensus_order.ordered_blocks {
            if hashgraph.sees(ordered, self.block_id) {
                return ConfirmationStatus::Voting;
            }
        }

        ConfirmationStatus::Unconfirmed
    }

    /// Check if finalized (checkpointed)
    fn is_finalized(&self, checkpoints: &[DAGCheckpoint]) -> bool {
        checkpoints.iter().any(|cp| {
            cp.shard_states.values().any(|root| root == &self.block_id)
        })
    }
}
```

### Fork Resolution

```rust
/// Fork resolution rules
struct ForkResolver {
    /// Resolution strategy
    strategy: ForkStrategy,
}

enum ForkStrategy {
    /// Longest chain (by total work)
    LongestChain,

    /// Hashgraph timestamp
    Timestamp,

    /// Producer reputation
    Reputation,
}

impl ForkResolver {
    /// Resolve fork between two blocks
    fn resolve(&self, fork_a: &DAGBlock, fork_b: &DAGBlock) -> Digest {
        match self.strategy {
            ForkStrategy::LongestChain => self.resolve_longest(fork_a, fork_b),
            ForkStrategy::Timestamp => self.resolve_timestamp(fork_a, fork_b),
            ForkStrategy::Reputation => self.resolve_reputation(fork_a, fork_b),
        }
    }

    fn resolve_longest(&self, a: &DAGBlock, b: &DAGBlock) -> Digest {
        // Compare depth in DAG
        // Return block with deeper ancestry
        if a.parents.len() > b.parents.len() {
            a.block_id
        } else {
            b.block_id
        }
    }

    fn resolve_timestamp(&self, a: &DAGBlock, b: &DAGBlock) -> Digest {
        // Earlier timestamp wins
        if a.timestamp <= b.timestamp {
            a.block_id
        } else {
            b.block_id
        }
    }

    fn resolve_reputation(&self, a: &DAGBlock, b: &DAGBlock) -> Digest {
        // Higher reputation producer wins
        // Would query reputation system
        a.block_id // Simplified
    }
}
```

### Network Propagation

```rust
/// DAG gossip topics
struct DAGTtopics {
    /// New block announcements
    new_block: Topic,

    /// Block requests
    block_request: Topic,

    /// Checkpoint announcements
    checkpoint: Topic,
}

/// Block propagator
struct BlockPropagator {
    /// Gossipsub
    gossipsub: Gossipsub,

    /// Topics
    topics: DAGTtopics,
}

impl BlockPropagator {
    /// Announce new block
    async fn announce(&self, block: &DAGBlock) {
        self.gossipsub
            .publish(&self.topics.new_block, block.serialize())
            .await;
    }

    /// Request missing block
    async fn request_block(&self, block_id: Digest, from: PeerId) {
        let request = BlockRequest { block_id };
        self.gossipsub
            .send_request(&from, &self.topics.block_request, request)
            .await;
    }
}
```

## Performance Targets

| Metric            | Target       | Notes                |
| ----------------- | ------------ | -------------------- |
| Blocks/second     | 1000+        | Per shard            |
| Confirmation time | <10s         | To finalized         |
| Fork rate         | <1%          | With honest majority |
| Finality          | Checkpointed | Irreversible         |
| DAG depth         | Unlimited    | No limit             |

## Adversarial Review

| Threat                     | Impact | Mitigation                   |
| -------------------------- | ------ | ---------------------------- |
| **Double-production**      | High   | See-before prevents          |
| **Selfish mining**         | Medium | Virtual voting detects       |
| **Eclipse**                | Medium | Multiple peer connections    |
| **Timestamp manipulation** | Low    | Hashgraph ordering dominates |
| **Partition attack**       | High   | Cross-shard checkpoints      |

## Alternatives Considered

| Approach                   | Pros             | Cons                   |
| -------------------------- | ---------------- | ---------------------- |
| **Linear chain (current)** | Simple           | Limited throughput     |
| **Hashgraph (this)**       | Fast, leaderless | Complex implementation |
| **Avalanche**              | High scalability | Probabilistic finality |
| **Tendermint/BFT**         | Proven           | Leader-based           |
| **Phantom/DAG**            | Good throughput  | Complex ordering       |

## Implementation Phases

### Phase 1: Core DAG

- [ ] DAG block structure
- [ ] Parent selection
- [ ] Basic gossip
- [ ] Local ordering

### Phase 2: Consensus

- [ ] Hashgraph implementation
- [ ] Witness detection
- [ ] Virtual voting
- [ ] Consensus ordering

### Phase 3: Sharding

- [ ] Per-shard DAGs
- [ ] Cross-shard ordering
- [ ] Global checkpoints

### Phase 4: Production

- [ ] Performance optimization
- [ ] Checkpointing
- [ ] Fork resolution

## Future Work

- **F1: Probabilistic Verification** — Random sampling for light clients
- **F2: Dynamic Sharding** — Adaptive shard creation
- **F3: Privacy** — Confidential transactions in DAG
- **F4: Storage Optimization** — Pruning old DAG history

## Rationale

### Why Hashgraph-Style?

Hashgraph provides:

- Leaderless consensus — No centralization
- Deterministic ordering — No randomness
- Fast confirmation — Seconds, not minutes
- Byzantine fault tolerance — 1/3 honest assumption

### Why Per-Shard DAGs?

Per-shard DAGs enable:

- Parallel processing — Each model shard independent
- Isolation — Failures don't cascade
- Scaling — Add shards = add throughput
- Simpler consensus — Smaller validator set

### Why Global Checkpoints?

Global checkpoints provide:

- Finality — Irreversible state
- Cross-shard ordering — Total order
- Storage efficiency — Prune old history
- Audit points — Clear final state

## Related RFCs

- RFC-0630 (Proof Systems): Proof-of-Inference Consensus
- RFC-0140 (Consensus): Sharded Consensus Protocol
- RFC-0142 (Consensus): Data Availability & Sampling Protocol
- RFC-0143 (Networking): OCTO-Network Protocol

## Related Use Cases

- [Hybrid AI-Blockchain Runtime](../../docs/use-cases/hybrid-ai-blockchain-runtime.md)
- [Node Operations](../../docs/use-cases/node-operations.md)

## Appendices

### A. Parent Selection Algorithm

```
When creating a new block:

1. Get all known blocks from last 10 seconds
2. Filter to unique ancestors
3. Select 3-5 blocks as parents:
   - At least 1 from different producer
   - At least 1 from same shard
   - Prefer recent (within 5 seconds)
4. Set self-parent to most recent
```

### B. Confirmation Flow

```
Block produced
    ↓
Gossip to network
    ↓
Received by nodes
    ↓
Added to local hashgraph
    ↓
Ancestry computed (see-before)
    ↓
Witness selection (round n)
    ↓
Virtual voting (round n+1)
    ↓
Famous witnesses determined
    ↓
Blocks ordered
    ↓
Checkpointed
    ↓
FINAL
```

---

**Version:** 1.1
**Submission Date:** 2026-03-07
**Last Updated:** 2026-03-07
