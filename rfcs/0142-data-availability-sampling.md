# RFC-0142: Data Availability & Sampling Protocol

## Status

Draft

## Summary

This RFC defines the **Data Availability & Sampling (DAS) Protocol** — a mechanism for efficiently verifying that shard data (model weights, datasets, proofs) is available across the network without requiring every node to download all data. Using Reed-Solomon erasure coding and random sampling, nodes can verify data availability with 99%+ probability while maintaining O(1) bandwidth per sample. The protocol integrates with the sharded consensus (RFC-0140) and OCTO-Network (RFC-0143) to provide cryptographic guarantees of data persistence.

## Design Goals

| Goal                         | Target             | Metric          |
| ---------------------------- | ------------------ | --------------- |
| **G1: Sampling Detection**   | 99%+               | Random sampling |
| **G2: Bandwidth Efficiency** | O(1) per sample    | Constant size   |
| **G3: Erasure Coding**       | 50% redundancy     | Reed-Solomon    |
| **G4: Challenge Frequency**  | Per block          | Random          |
| **G5: Slash Integration**    | Economic penalties | Stake removal   |

## Motivation

### CAN WE? — Feasibility Research

The fundamental question: **Can we verify multi-terabyte model shard availability without downloading it?**

Challenges in decentralized AI networks:

| Challenge             | Impact                             |
| --------------------- | ---------------------------------- |
| Large shard sizes     | Cannot download all                |
| Bandwidth constraints | Full replication expensive         |
| Storage limitations   | Not all nodes can store everything |
| Withholding attacks   | Malicious nodes hide data          |

Research confirms feasibility through:

- **Erasure coding** — Data can be reconstructed from subset
- **Information theory** — Random sampling detects withholding
- **Merkle proofs** — Efficient verification
- **Economic incentives** — Staking prevents attacks

### WHY? — Why This Matters

Without DAS:

| Problem                   | Consequence                 |
| ------------------------- | --------------------------- |
| Full replication required | Bandwidth waste             |
| No availability guarantee | Shard retrieval fails       |
| No fraud detection        | Withholding goes undetected |
| Centralized storage       | Single point of failure     |

DAS enables:

- **Light nodes** — Verify without full storage
- **Bandwidth efficiency** — O(1) verification
- **Economic security** — Slash unavailable nodes
- **Decentralization** — No single storage provider

### WHAT? — What This Specifies

DAS defines:

1. **Erasure coding scheme** — Reed-Solomon parameters
2. **Sampling protocol** — Random fragment requests
3. **Merkle tree structure** — Efficient proofs
4. **Challenge mechanism** — Random sampling seeds
5. **Availability claims** — Stake-backed assertions
6. **Slashing conditions** — Economic penalties

### HOW? — Implementation

Integration with existing stack:

```
RFC-0142 (DAS) ← NEW
       ↓
RFC-0140 (Sharded Consensus)
       ↓
RFC-0143 (OCTO-Network)
```

## Specification

### Erasure Coding Scheme

```rust
/// Reed-Solomon erasure coder
struct ErasureCoder {
    /// Data fragments (k)
    data_shards: usize,

    /// Parity fragments (m)
    parity_shards: usize,

    /// Total fragments (n = k + m)
    total_shards: usize,

    /// Fragment size
    fragment_size: usize,
}

impl ErasureCoder {
    /// Create coder with parameters
    fn new(data_shards: usize, parity_shards: usize) -> Self {
        Self {
            data_shards,
            parity_shards,
            total_shards: data_shards + parity_shards,
            fragment_size: 0, // Set based on data
        }
    }

    /// Encode data into fragments
    fn encode(&self, data: &[u8]) -> Result<Vec<DataFragment>, Error> {
        // Use Reed-Solomon (liberator or similar)
        // Split data into k fragments
        // Generate m parity fragments
        // Any k of n fragments can reconstruct data
    }

    /// Decode from any k fragments
    fn decode(&self, fragments: &[DataFragment]) -> Result<Vec<u8>, Error> {
        if fragments.len() < self.data_shards {
            return Err(Error::InsufficientFragments);
        }
        // Reed-Solomon decode
    }

    /// Calculate redundancy ratio
    fn redundancy(&self) -> f64 {
        self.parity_shards as f64 / self.data_shards as f64
    }
}

/// Individual data fragment
struct DataFragment {
    /// Fragment index (0 to n-1)
    index: usize,

    /// Fragment data
    data: Vec<u8>,

    /// Merkle root for this fragment
    fragment_root: Digest,
}
```

### Merkle Commitment Structure

```rust
/// DAS Merkle tree structure
struct DASMerkleTree {
    /// Fragment merkle roots
    fragment_roots: Vec<Digest>,

    /// Final data root
    data_root: Digest,

    /// Tree height
    height: usize,
}

impl DASMerkleTree {
    /// Build from fragments
    fn build(fragments: &[DataFragment]) -> Self {
        // Layer 0: Fragment hashes
        let mut layer: Vec<Digest> = fragments
            .iter()
            .map(|f| hash(&f.data))
            .collect();

        // Build tree up
        while layer.len() > 1 {
            let mut next_layer = Vec::new();
            for chunk in layer.chunks(2) {
                if chunk.len() == 2 {
                    next_layer.push(hash([chunk[0], chunk[1]]));
                } else {
                    next_layer.push(chunk[0]);
                }
            }
            layer = next_layer;
        }

        Self {
            fragment_roots: fragments.iter().map(|f| hash(&f.data)).collect(),
            data_root: layer[0],
            height: (fragments.len() as f64).log2() as usize,
        }
    }

    /// Generate proof for fragment
    fn prove(&self, index: usize) -> Vec<Digest> {
        // Generate Merkle path to root
    }

    /// Verify fragment proof
    fn verify(fragment: &DataFragment, proof: &[Digest], root: Digest) -> bool {
        // Verify path to root
    }
}
```

### Sampling Protocol

```rust
/// DAS sampling request
struct DASRequest {
    /// Data root being sampled
    data_root: Digest,

    /// Random fragment index
    fragment_index: usize,

    /// Challenge seed (for verification)
    challenge_seed: Digest,

    /// Requester ID
    requester: PeerId,

    /// Timestamp
    timestamp: u64,
}

/// DAS response
struct DASResponse {
    /// Fragment data
    fragment: Vec<u8>,

    /// Fragment index
    fragment_index: usize,

    /// Merkle proof
    proof: Vec<Digest>,

    /// Data root
    data_root: Digest,

    /// Responder signature
    signature: Signature,
}

/// DAS verifier (runs on light nodes)
struct DASVerifier {
    /// Sample count per check
    sample_count: usize,

    /// Failure threshold
    failure_threshold: usize,

    /// Minimum nodes to query
    min_nodes: usize,

    /// Timeout
    timeout_ms: u64,
}

impl DASVerifier {
    /// Create verifier
    fn new(sample_count: usize) -> Self {
        Self {
            sample_count,
            failure_threshold: sample_count / 10, // 10% tolerance
            min_nodes: sample_count,
            timeout_ms: 5000,
        }
    }

    /// Verify data availability
    async fn verify_availability(
        &self,
        data_root: Digest,
        storage_nodes: &[PeerId],
    ) -> Result<VerificationResult, Error> {
        let mut successful_samples = 0;
        let mut failed_samples = 0;

        // Generate random indices
        let indices = self.generate_random_indices(data_root);

        for index in indices.iter().take(self.sample_count) {
            // Pick random node
            let node = self.random_node(storage_nodes);

            // Request sample
            let request = DASRequest {
                data_root,
                fragment_index: *index,
                challenge_seed: hash([data_root, *index]),
                requester: self.local_peer_id(),
                timestamp: current_timestamp(),
            };

            match self.request_sample(node, request).await {
                Ok(response) => {
                    if self.verify_response(&response, &request)? {
                        successful_samples += 1;
                    } else {
                        failed_samples += 1;
                    }
                }
                Err(_) => {
                    failed_samples += 1;
                }
            }
        }

        // Decision
        if failed_samples <= self.failure_threshold {
            Ok(VerificationResult::Available)
        } else {
            Ok(VerificationResult::Unavailable)
        }
    }

    /// Generate random indices from seed
    fn generate_random_indices(&self, seed: Digest) -> Vec<usize> {
        let mut rng = ChaCha8::from_seed(seed);
        (0..self.sample_count)
            .map(|_| rng.gen_range(0..MAX_FRAGMENTS))
            .collect()
    }

    /// Verify response
    fn verify_response(&self, response: &DASResponse, request: &DASRequest) -> Result<bool, Error> {
        // 1. Verify signature
        // 2. Verify Merkle proof
        // 3. Verify fragment index matches
        // 4. Verify data root matches
    }
}

/// Verification result
enum VerificationResult {
    /// Data is available
    Available,

    /// Data is unavailable
    Unavailable,

    /// Cannot determine (network issues)
    Unknown,
}
```

### Challenge Mechanism

```rust
/// DAS challenge generator
struct DASChallenge {
    /// Current epoch
    epoch: u64,

    /// Random beacon
    random_beacon: Digest,
}

impl DASChallenge {
    /// Generate challenge for epoch
    fn generate(epoch: u64, previous_beacon: Digest) -> DASChallenge {
        // Use VDF or VRF for unpredictable beacon
        let beacon = vdf_prove(previous_beacon, epoch);

        Self {
            epoch,
            random_beacon: beacon,
        }
    }

    /// Get fragment index for sampling
    fn fragment_index(&self, shard_id: ShardId, node_index: usize) -> usize {
        let seed = hash([self.random_beacon, shard_id, node_index as u64]);
        let mut rng = ChaCha8::from_seed(seed);
        rng.gen_range(0..MAX_FRAGMENTS)
    }
}
```

### Availability Claims and Staking

```rust
/// Data availability claim (posted to chain)
struct AvailabilityClaim {
    /// Claim ID
    claim_id: Digest,

    /// Data root being claimed
    data_root: Digest,

    /// Erasure root
    erasure_root: Digest,

    /// Number of samples verified
    samples_verified: u32,

    /// Success rate
    success_rate: f64,

    /// Timestamp
    timestamp: u64,

    /// Claimer (storage node)
    claimer: PublicKey,

    /// Signature
    signature: Signature,
}

impl AvailabilityClaim {
    /// Create claim
    fn create(
        data_root: Digest,
        erasure_root: Digest,
        verification: &VerificationResult,
        claimer: PublicKey,
    ) -> Self {
        let (samples, rate) = match verification {
            VerificationResult::Available => (100, 1.0),
            _ => (0, 0.0),
        };

        let claim = Self {
            claim_id: hash([data_root, claimer]),
            data_root,
            erasure_root,
            samples_verified: samples,
            success_rate: rate,
            timestamp: current_timestamp(),
            claimer,
            signature: Signature::default(),
        };

        claim
    }

    /// Verify claim
    fn verify(&self) -> bool {
        // Verify signature
        // Verify success rate meets threshold
    }
}

/// Storage node stake requirement
struct StorageStake {
    /// Minimum stake
    min_stake: TokenAmount,

    /// Slashable amount
    slashable: TokenAmount,

    /// Lock period
    lock_period: u64,
}

impl StorageStake {
    fn default() -> Self {
        Self {
            min_stake: TokenAmount::from(10_000), // 10k OCTO
            slashable: TokenAmount::from(5_000), // 50% of stake
            lock_period: 30 * 24 * 3600, // 30 days
        }
    }
}

/// Slashing conditions
enum DASlashingCondition {
    /// Failed too many samples
    SampleFailure {
        claim_id: Digest,
        failure_rate: f64,
    },

    /// Data not retrievable
    DataUnavailable {
        data_root: Digest,
        requester: PeerId,
    },

    /// Responded with invalid proof
    InvalidProof {
        claim_id: Digest,
    },

    /// Didn't respond to challenge
    NoResponse {
        challenge: DASChallenge,
    },
}

impl DASlashingCondition {
    fn slash_amount(&self, stake: TokenAmount) -> TokenAmount {
        match self {
            Self::SampleFailure { failure_rate, .. } => {
                stake * (*failure_rate as f64)
            }
            Self::DataUnavailable { .. } => stake * 0.5,
            Self::InvalidProof { .. } => stake,
            Self::NoResponse { .. } => stake * 0.25,
        }
    }
}
```

### Integration with Consensus

```rust
/// DAS in block production
struct DASConsensus {
    /// Verifier
    verifier: DASVerifier,

    /// Challenge generator
    challenge: DASChallenge,

    /// Stake manager
    stake_manager: StakeManager,
}

impl DASConsensus {
    /// Verify data availability before accepting block
    async fn verify_block_data(&self, block: &PoIBlock) -> Result<bool, Error> {
        // For each data root in block
        for data_root in &block.data_roots {
            // Generate challenge
            let challenge = self.challenge.generate(block.epoch, self.challenge.random_beacon);

            // Get storage nodes for this data
            let nodes = self.get_storage_nodes(data_root).await?;

            // Verify
            let result = self.verifier.verify_availability(*data_root, &nodes).await?;

            if !matches!(result, VerificationResult::Available) {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Slash nodes with low availability
    async fn process_slashing(&self, claims: &[AvailabilityClaim]) {
        for claim in claims {
            if claim.success_rate < MIN_SUCCESS_RATE {
                // Slash
                self.stake_manager.slash(&claim.claimer, SlashingAmount::Medium).await;
            }
        }
    }
}
```

## Performance Targets

| Metric            | Target | Notes              |
| ----------------- | ------ | ------------------ |
| Sample size       | <1KB   | Fragment           |
| Samples needed    | 10     | For 99% confidence |
| Verification time | <5s    | Per data root      |
| Bandwidth/sample  | O(1)   | Constant           |
| Slash detection   | 99%+   | Probability        |

## Adversarial Review

| Threat                 | Impact | Mitigation                       |
| ---------------------- | ------ | -------------------------------- |
| **Withholding attack** | High   | Random sampling + erasure coding |
| **Sampling evasion**   | High   | Random challenge seeds           |
| **Fake responses**     | Medium | Signature verification           |
| **Sybil attack**       | Medium | Stake requirement                |
| **Eclipse**            | Low    | Query multiple nodes             |

## Alternatives Considered

| Approach               | Pros               | Cons                      |
| ---------------------- | ------------------ | ------------------------- |
| **Full replication**   | Simple             | Bandwidth waste           |
| **This RFC**           | Efficient + secure | Implementation complexity |
| **Light clients only** | No storage         | Trust assumption          |
| **Checkpointing only** | Simple             | No real-time verification |

## Implementation Phases

### Phase 1: Core

- [ ] Reed-Solomon encoding
- [ ] Merkle tree structure
- [ ] Basic sampling

### Phase 2: Integration

- [ ] Challenge generation
- [ ] Availability claims
- [ ] Stake integration

### Phase 3: Production

- [ ] Performance optimization
- [ ] Slash automation
- [ ] Light client support

## Future Work

- **F1: Erasure Coding Diversity** — Multiple coding schemes
- **F2:ZK-Based DAS** — Zero-knowledge proofs
- **F3: Proxy Re-encryption** — Conditional sharing
- **F4: Differential Privacy** — Private sampling

## Rationale

### Why Reed-Solomon?

Reed-Solomon provides:

- Optimal redundancy — k-of-n minimum
- Simple implementation — Battle-tested
- Fast encoding/decoding — Optimized libraries

### Why 10 Samples?

Mathematical analysis:

- With 10 random samples and 50% withholding
- Probability of detection: 99.9%
- Bandwidth: 10KB for 1KB fragments

### Why Stake-Based?

Staking provides:

- Economic cost to attack
- Alignment with network health
- Automatic enforcement

## Related RFCs

- RFC-0130: Proof-of-Inference Consensus
- RFC-0140: Sharded Consensus Protocol
- RFC-0141: Parallel Block DAG Specification
- RFC-0143: OCTO-Network Protocol

## Related Use Cases

- [Hybrid AI-Blockchain Runtime](../../docs/use-cases/hybrid-ai-blockchain-runtime.md)
- [Node Operations](../../docs/use-cases/node-operations.md)

## Appendices

### A. Mathematical Analysis

**Detection Probability:**

For n samples with probability p of detecting a withholder:

- If 50% data withheld: p = 1 - (0.5)^n
- n=10: p = 99.9%

**Bandwidth:**

```
Samples: 10
Fragment size: 1KB
Total: 10KB verification
vs Full download: 2TB (model)
Savings: 99.9995%
```

### B. Slashing Schedule

| Offense           | Slash % |
| ----------------- | ------- |
| 90%+ failure rate | 100%    |
| 50%+ failure rate | 50%     |
| No response       | 25%     |
| Invalid proof     | 75%     |

---

**Version:** 1.1
**Submission Date:** 2026-03-07
**Last Updated:** 2026-03-07
