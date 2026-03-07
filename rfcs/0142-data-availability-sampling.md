# RFC-0142: Data Availability & Sampling Protocol

## Status

Draft

## Summary

This RFC defines the **Data Availability & Sampling (DAS) Protocol** — a mechanism for efficiently verifying that shard data is available across the network without requiring every node to download all data. Using erasure coding and random sampling, nodes can verify data availability with high probability while maintaining minimal bandwidth requirements.

## Design Goals

| Goal                   | Target                 | Metric          |
| ---------------------- | ---------------------- | --------------- |
| **G1: Sampling**       | 99% detection          | Random sampling |
| **G2: Efficiency**     | O(1) bandwidth         | Per sample      |
| **G3: Erasure Coding** | 50% redundancy         | Reed-Solomon    |
| **G4: Challenges**     | On-demand verification | Random          |

## Motivation

Large model shards and datasets require efficient availability verification:

- Cannot require all nodes to store all data
- Must detect withholding attacks
- Need economic penalties for unavailability

## Specification

### Erasure Coding

```rust
/// Data erasure coding
struct ErasureCoder {
    /// Data shards
    data_shards: u32,

    /// Parity shards
    parity_shards: u32,
}

impl ErasureCoder {
    /// Encode data
    fn encode(&self, data: &[u8]) -> Vec<Vec<u8>> {
        // Reed-Solomon encoding
    }

    /// Decode
    fn decode(&self, fragments: &[Vec<u8>]) -> Vec<u8> {
        // Reconstruct from any subset
    }
}
```

### Sampling Protocol

```rust
/// DAS request
struct DASRequest {
    /// Data root
    data_root: Digest,

    /// Sample index
    index: u32,

    /// Challenge
    challenge: Digest,
}

/// DAS response
struct DASResponse {
    /// Fragment
    fragment: Vec<u8>,

    /// Merkle proof
    proof: Vec<Digest>,
}

/// DAS verifier
struct DASVerifier {
    /// Sample count
    sample_count: u32,

    /// Failure threshold
    failure_threshold: u32,
}

impl DASVerifier {
    /// Verify availability
    async fn verify(&self, root: Digest, nodes: &[PeerId]) -> bool {
        let mut failures = 0;

        for _ in 0..self.sample_count {
            let node = self.random_node(nodes);
            let response = self.request_sample(root, node).await?;

            if !self.verify_response(&response, &root) {
                failures += 1;
            }
        }

        failures < self.failure_threshold
    }
}
```

### Availability Claims

```rust
/// Data availability claim
struct AvailabilityClaim {
    /// Data root
    data_root: Digest,

    /// Erasure root
    erasure_root: Digest,

    /// Samples available
    samples: u32,

    /// Timestamp
    timestamp: u64,

    /// Signer
    signer: PublicKey,
}
```

## Performance Targets

| Metric            | Target |
| ----------------- | ------ |
| Sample size       | <1KB   |
| Samples needed    | 10     |
| Verification time | <5s    |
| Bandwidth/sample  | O(1)   |

## Related RFCs

- RFC-0130: Proof-of-Inference Consensus
- RFC-0140: Sharded Consensus Protocol
- RFC-0143: OCTO-Network Protocol
