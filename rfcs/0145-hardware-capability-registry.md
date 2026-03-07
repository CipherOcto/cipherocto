# RFC-0145: Hardware Capability Registry

## Status

Draft

## Summary

This RFC defines the **Hardware Capability Registry** — a protocol for nodes to advertise their compute capabilities (GPU memory, tensor throughput, available model shards) to enable intelligent task routing in the Proof-of-Inference network.

## Design Goals

| Goal | Target | Metric |
| ---- | ------ | ------ |
| G1: Capability Advertisement | All nodes advertise hardware | 100% coverage |
| G2: Accurate Metrics | Real-time hardware reporting | <30s refresh |
| G3: Task Matching | Intelligent worker selection | <5s assignment |
| G4: Privacy | Minimal hardware fingerprinting | No unique identification |

## Motivation

### CAN WE? — Feasibility Research

The fundamental question: **Can we create a hardware capability system that enables intelligent task routing without compromising node privacy?**

Research confirms feasibility through:

- Capability bitmap advertising (no raw specs)
- Aggregate reputation scoring
- Differential privacy techniques
- Staked self-reporting with slashing

### WHY? — Why This Matters

Without hardware capability registry:

| Problem | Consequence |
|---------|-------------|
| Blind task routing | Workers receive unsuitable tasks |
| GPU starvation | Memory-intensive tasks sent to low-RAM nodes |
| Performance degradation | Timeouts, failed proofs, wasted work |
| Economic inefficiency | Misallocated compute resources |

The registry enables:

- **Intelligent routing** — Tasks to capable workers
- **Resource optimization** — Match task requirements to hardware
- **Reputation tracking** — Historical performance data
- **Capacity planning** — Network-wide compute visibility

### WHAT? — What This Specifies

The registry defines:

1. **Capability advertisement** — What hardware info nodes share
2. **Metric reporting** — How capabilities are measured
3. **Task matching** — How workers are selected
4. **Reputation system** — How performance is tracked

### HOW? — Implementation

Integration with existing stack:

```
RFC-0144 (Task Market)
       ↓
RFC-0145 (Hardware Capability Registry) ← NEW
       ↓
RFC-0143 (OCTO-Network)
       ↓
RFC-0130 (Proof-of-Inference)
```

## Specification

### Capability Advertisement

```rust
/// Hardware capability advertisement
struct HardwareCapability {
    /// Node identity
    node_id: PublicKey,

    /// Compute capabilities
    compute: ComputeCapabilities,

    /// Memory capabilities
    memory: MemoryCapabilities,

    /// Network capabilities
    network: NetworkCapabilities,

    /// Available model shards
    available_shards: Vec<ShardId>,

    /// Timestamp
    timestamp: u64,

    /// Self-stake for honesty
    stake: TokenAmount,
}

/// Compute capabilities
struct ComputeCapabilities {
    /// Device type
    device_type: DeviceType,

    /// Tensor throughput (GFLOPS)
    tensor_throughput: u64,

    /// CUDA cores / compute units
    compute_units: u32,

    /// Supported precisions
    precisions: Vec<Precision>,

    /// Specialized accelerators
    accelerators: Vec<Accelerator>,
}

enum DeviceType {
    CPU,
    NVIDIA_GPU,
    AMD_GPU,
    TPU,
    Custom(String),
}

enum Precision {
    Fp32,
    Fp16,
    Bf16,
    Int8,
    Int4,
}

/// Memory capabilities
struct MemoryCapabilities {
    /// Total VRAM (bytes)
    vram_total: u64,

    /// Available VRAM (bytes)
    vram_available: u64,

    /// System RAM (bytes)
    system_ram: u64,

    /// Memory bandwidth (GB/s)
    memory_bandwidth: u64,
}

/// Network capabilities
struct NetworkCapabilities {
    /// Bandwidth (Mbps)
    bandwidth: u64,

    /// Latency to peers (ms)
    avg_latency: u32,

    /// Geographic region
    region: String,
}
```

### Capability Verification

```rust
/// Capability verification protocol
struct CapabilityVerifier {
    /// Challenge generation
    fn generate_challenge(&self, node_id: PublicKey) -> Challenge;

    /// Verify claimed capabilities
    fn verify(&self, capability: &HardwareCapability, proof: &CapabilityProof) -> bool;
}

/// Proof of capability
struct CapabilityProof {
    /// Benchmark results
    benchmark: BenchmarkResult,

    /// Signature from node
    signature: Signature,

    /// Timestamp
    timestamp: u64,
}

/// Benchmark result
struct BenchmarkResult {
    /// Measured tensor throughput
    measured_throughput: u64,

    /// Measured memory bandwidth
    measured_bandwidth: u64,

    /// Verification data
    verification_data: Vec<u8>,
}
```

### Task Matching Algorithm

```rust
/// Task capability requirements
struct TaskRequirements {
    /// Minimum memory required
    min_memory: u64,

    /// Required precision support
    required_precision: Precision,

    /// Required model shards
    required_shards: Vec<ShardId>,

    /// Preferred region
    preferred_region: Option<String>,

    /// Minimum reputation
    min_reputation: u64,
}

/// Worker selector with capability matching
struct CapabilityMatcher {
    /// Match task to best workers
    fn match_workers(
        &self,
        task: &TaskRequirements,
        candidates: &[HardwareCapability],
    ) -> Vec<WorkerMatch> {
        candidates
            .iter()
            .filter(|c| self.meets_requirements(task, c))
            .map(|c| self.calculate_match_score(task, c))
            .sort_by(|a, b| b.score.cmp(&a.score))
            .take(3)
            .collect()
    }

    fn meets_requirements(&self, task: &TaskRequirements, cap: &HardwareCapability) -> bool {
        cap.memory.vram_available >= task.min_memory
            && cap.compute.precisions.contains(&task.required_precision)
            && cap.available_shards.contains_all(&task.required_shards)
            && cap.network.avg_latency < 100 // ms
    }
}

/// Worker match with score
struct WorkerMatch {
    worker: PublicKey,
    score: f64,
    reputation: u64,
    available_at: Timestamp,
}
```

### Reputation Tracking

```rust
/// Worker performance record
struct WorkerPerformance {
    /// Node identity
    node_id: PublicKey,

    /// Tasks completed
    tasks_completed: u64,

    /// Tasks failed
    tasks_failed: u64,

    /// Average completion time
    avg_completion_time: u64,

    /// Proof success rate
    proof_success_rate: f64,

    /// Last updated
    last_updated: u64,
}

impl WorkerPerformance {
    /// Calculate reputation score
    fn reputation_score(&self) -> u64 {
        let success_rate = self.tasks_completed as f64
            / (self.tasks_completed + self.tasks_failed) as f64;

        let base_score = success_rate * 100.0;
        let time_bonus = (self.avg_completion_time < 30000) as u64 as f64 * 0.1;

        ((base_score + time_bonus) * 1000.0) as u64
    }
}
```

### Registry Operations

```rust
/// Hardware registry
struct HardwareRegistry {
    /// All registered capabilities
    capabilities: HashMap<PublicKey, HardwareCapability>,

    /// Performance records
    performance: HashMap<PublicKey, WorkerPerformance>,
}

impl HardwareRegistry {
    /// Register capabilities
    fn register(&mut self, capability: HardwareCapability) {
        // Verify stake requirement
        assert!(capability.stake >= MIN_STAKE);

        // Store capability
        self.capabilities.insert(capability.node_id, capability);

        // Initialize performance record
        self.performance.insert(capability.node_id, WorkerPerformance {
            node_id: capability.node_id,
            tasks_completed: 0,
            tasks_failed: 0,
            avg_completion_time: 0,
            proof_success_rate: 1.0,
            last_updated: current_timestamp(),
        });
    }

    /// Update capabilities
    fn update(&mut self, capability: HardwareCapability) {
        // Verify node ownership
        assert!(self.verify_owner(&capability));

        self.capabilities.insert(capability.node_id, capability);
    }

    /// Query suitable workers
    fn query_workers(&self, requirements: &TaskRequirements) -> Vec<PublicKey> {
        self.capabilities
            .iter()
            .filter(|(_, cap)| self.meets_requirements(requirements, cap))
            .map(|(id, _)| *id)
            .collect()
    }
}
```

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Advertisement refresh | <30s | Near real-time |
| Worker matching | <5s | Per task |
| Registry lookup | <100ms | Query response |
| Capability verification | <60s | Initial registration |

## Adversarial Review

| Threat | Impact | Mitigation |
|--------|--------|------------|
| **Capability falsification** | High | Staked self-report + random benchmarks |
| **Hardware fingerprinting** | Medium | Aggregate data, differential privacy |
| **Reputation manipulation** | High | Slashing for false reporting |
| **Sybil attacks** | High | Stake requirement + rate limiting |

## Alternatives Considered

| Approach | Pros | Cons |
|----------|------|------|
| **Centralized benchmarking** | Accurate | Single point of failure |
| **Peer verification** | Decentralized | Collusion risk |
| **Self-reporting only** | Simple | Easy to fake |
| **This approach** | Balanced | Stake requirement |

## Implementation Phases

### Phase 1: Core Registry

- [ ] Basic capability advertisement
- [ ] Stake requirement
- [ ] Registry storage

### Phase 2: Capability Matching

- [ ] Task requirements matching
- [ ] Worker selection algorithm
- [ ] Reputation tracking

### Phase 3: Verification

- [ ] Random benchmark challenges
- [ ] Slash for falsification
- [ ] Privacy-preserving aggregates

### Phase 4: Integration

- [ ] OCTO-Network integration
- [ ] Task Market integration
- [ ] Performance optimization

## Future Work

- F1: Specialized hardware profiles (FPGA, ASIC)
- F2: Dynamic capability scaling
- F3: Cross-region load balancing
- F4: Hardware reputation markets

## Rationale

### Why Self-Reporting with Staking?

Self-reporting with economic stake provides:

- **Speed** — No centralized benchmarking bottleneck
- **Economics** — Honest reporting incentivized by stake
- **Verification** — Random challenges catch falsification
- **Decentralization** — No single benchmarking authority

### Why Not Raw Hardware Fingerprinting?

Raw hardware specs enable fingerprinting, which:

- Compromises node privacy
- Enables discrimination
- Creates tracking vectors

This RFC uses capability bitmaps instead.

## Related RFCs

- RFC-0143: OCTO-Network Protocol
- RFC-0144: Inference Task Market
- RFC-0130: Proof-of-Inference Consensus

## Related Use Cases

- [Compute Provider Network (OCTO-A)](../../docs/use-cases/compute-provider-network.md)
- [Node Operations](../../docs/use-cases/node-operations.md)
- [Hybrid AI-Blockchain Runtime](../../docs/use-cases/hybrid-ai-blockchain-runtime.md)

---

**Version:** 1.0
**Submission Date:** 2026-03-07
**Last Updated:** 2026-03-07
