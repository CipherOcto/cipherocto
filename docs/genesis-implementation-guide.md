# Genesis Implementation Guide

## Overview

This guide outlines the **minimum viable implementation path** for a CipherOcto testnet. The goal is not implementing the entire RFC stack, but building the smallest system that proves the architecture works.

Think of it as a **vertical slice** through the stack:

```
query → deterministic inference → proof generation → block creation → consensus verification → network propagation
```

If this works, the entire architecture becomes credible.

---

## Core Components (9 Required)

### Component 1: Deterministic Numeric Runtime

**Purpose:** Foundation for everything — same model + same input = identical output

**Required RFCs:**
- RFC-0104: Deterministic Floating-Point
- RFC-0105: Deterministic Quant Arithmetic
- RFC-0106: Deterministic Numeric Tower

**Implementation:**
```rust
// octo-math library
pub mod deterministic {
    /// Deterministic tensor arithmetic
    pub fn matmul(a: &Tensor, b: &Tensor) -> Tensor;

    /// Fixed rounding rules
    pub fn softmax(x: &Tensor) -> Tensor;

    /// Reproducible across CPU/GPU
    pub fn layer_norm(x: &Tensor) -> Tensor;
}
```

**Key Property:** Without this, proofs cannot work.

---

### Component 2: Deterministic AI-VM

**Purpose:** Execute canonical AI operations and produce execution traces

**Required RFCs:**
- RFC-0120: Deterministic AI-VM

**Implementation:**
```rust
// octo-vm
pub struct AI-VM {
    // Execute canonical operations
    pub fn execute(&self, program: &Program) -> (Tensor, Trace);
}

pub enum Operator {
    MatMul,
    Softmax,
    Attention,
    LayerNorm,
}
```

**Output:** Execution trace → becomes proof input

---

### Component 3: Deterministic Transformer Circuit

**Purpose:** Generate STARK proofs for inference

**Required RFCs:**
- RFC-0131: Deterministic Transformer Circuit

**Start Small:**
- 100M parameter transformer (manageable proof size, easier debugging)

**Implementation:**
```rust
// octo-transformer
pub struct TransformerCircuit {
    pub params: u32,  // 100M
    pub layers: u32,
    pub heads: u32,
}

impl Circuit for TransformerCircuit {
    // AIR constraints for transformer ops
}
```

---

### Component 4: Proof-of-Inference Engine

**Purpose:** Generate and verify STARK proofs

**Required RFCs:**
- RFC-0130: Proof-of-Inference Consensus

**Options (use existing framework):**
- RISC Zero
- SP1
- Winterfell
- StarkWare STWO

**Implementation:**
```rust
// octo-prover
pub struct Prover {
    circuit: TransformerCircuit,
}

pub fn prove(inference: &Inference) -> Proof {
    // Generate STARK proof
}

pub fn verify(proof: &Proof, result: &Digest) -> bool {
    // Verify in O(log n)
}
```

**Output:** (inference_result, proof)

---

### Component 5: OCTO-Network (libp2p)

**Purpose:** Peer-to-peer communication

**Required RFCs:**
- RFC-0143: OCTO-Network Protocol

**Implementation:**
```rust
// octo-network
pub struct Network {
    // libp2p modules
    kad: KademliaDHT,      // peer discovery
    gossip: Gossipsub,      // block propagation
    req_resp: RequestResponse, // proof fetching
}

pub const TOPICS: &str = [
    "octo.blocks",   // block propagation
    "octo.tasks",    // task distribution
    "octo.proofs",   // proof exchange
];
```

---

### Component 6: Minimal Consensus Layer

**Purpose:** Block creation and verification

**Required RFCs:**
- RFC-0130: Proof-of-Inference
- RFC-0141: Parallel Block DAG

**Simplify for Genesis:**
- Start with single chain (no shards)
- Add sharding later

**Block Structure:**
```rust
struct Block {
    previous_hash: Digest,
    inference_task: Task,
    inference_result: Result,
    proof_hash: Digest,
    miner_signature: Signature,
}

fn verify_block(block: &Block) -> bool {
    verify_proof(&block.proof_hash)
        && verify_result_hash(&block.result)
        && verify_signature(&block.miner)
}
```

---

### Component 7: Inference Task Generator

**Purpose:** Replace mining puzzles with useful work

**Implementation:**
```rust
// octo-task-engine
pub struct TaskGenerator {
    prompt_dataset: Vec<String>,
}

pub fn generate_task() -> Task {
    let prompt = random_prompt();
    Task {
        prompt,
        model_id,
        difficulty,
    }
}

pub fn adjust_difficulty(block_time: u64) {
    // Increase: make model larger or batch bigger
    // Decrease: make model smaller
}
```

**Task Example:** Run model inference on prompt corpus → deterministic result hash

---

### Component 8: Minimal Dataset Registry

**Purpose:** Deterministic prompts for inference tasks

**Required RFCs:**
- RFC-0133: Proof-of-Dataset Integrity

**Simplified Format:**
```rust
struct Dataset {
    root: Digest,
    merkle: MerkleTree,
    records: Vec<Record>,
}

fn get_prompt(i: u32) -> String {
    dataset.records[i % len].prompt
}
```

**Purpose:** Every node can reproduce the same task from the same index.

---

### Component 9: Simple Wallet & Node Identity

**Purpose:** Node identification and signing

**Required RFCs:**
- RFC-0102: Wallet Cryptography

**Implementation:**
```rust
struct NodeIdentity {
    node_id: PublicKey,
    stake_key: SecretKey,
    signing_key: SecretKey,
}

impl NodeIdentity {
    pub fn sign_block(&self, block: &Block) -> Signature;
    pub fn verify_task(&self, task: &Task) -> bool;
}
```

---

## Genesis Architecture

```
                    users
                      │
                      │
              ┌───────┴───────┐
              │  OCTO Network  │
              │    (libp2p)    │
              └───────┬───────┘
                      │
          ┌───────────┼───────────┐
          │           │           │
      node A      node B      node C
          │           │           │
    ┌─────┴─────┐   │     ┌─────┴─────┐
    │ AI-VM      │   │     │ AI-VM     │
    │ Execution   │   │     │ Execution  │
    └─────┬─────┘   │     └─────┬─────┘
          │           │           │
      prover      prover      prover
          │           │           │
          └─────┬─────┴─────┬─────┘
                │           │
            block creation
                │
             block DAG
```

---

## Minimal Genesis Node

**Single Binary:** `octo-node`

```rust
mod network;   // libp2p
mod consensus;  // PoI + DAG
mod prover;     // STARK proofs
mod vm;         // AI-VM
mod dataset;    // Prompt registry
mod task_engine; // Task generation
```

**Startup:**
```bash
octo-node --bootstrap --network testnet
```

---

## Network Size for Genesis

**Minimum:** 5-10 nodes

| Node | Role |
|------|------|
| node1 | bootstrap |
| node2 | compute |
| node3 | compute |
| node4 | verifier |
| node5 | router |

---

## First Demonstration Goal

Prove the complete pipeline works:

```
task generated
     ↓
node runs inference (AI-VM)
     ↓
proof generated (STARK)
     ↓
block proposed
     ↓
network verifies proof
     ↓
block accepted
```

**What This Proves:**
- Proof-of-Inference consensus works
- AI inference secures the network
- Deterministic execution is reproducible

---

## What Can Wait (Application Layer)

These RFCs are **not required for genesis**:

| RFC | Reason |
|-----|--------|
| RFC-0118 | Autonomous Agent Organizations — application layer |
| RFC-0111 | Knowledge Markets — application layer |
| RFC-0100 | AI Quota Marketplace — application layer |
| RFC-0125 | Model Liquidity — application layer |
| RFC-0119 | Alignment Mechanisms — application layer |

**Genesis focuses on the infrastructure layer.** Applications can grow on top.

---

## Realistic Build Order

| Step | Component | Duration |
|------|-----------|----------|
| 1 | Deterministic Math Library | 1-2 months |
| 2 | Deterministic AI-VM | 2 months |
| 3 | Transformer Execution | 2 months |
| 4 | Proof Generation | 2 months |
| 5 | libp2p Network | 1-2 months |
| 6 | Minimal Consensus | 1-2 months |
| 7 | Task Generator | 1 month |
| 8 | Dataset Registry | 1 month |
| 9 | Node Binary | 1 month |

**Total:** 12-16 months for a small team

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Proof size too large | Start with small model (100M params) |
| Network instability | Limited node count (5-10) |
| Consensus failure | Single chain initially |
| Performance issues | Profile and optimize iteratively |

---

## Success Criteria

| Metric | Target |
|--------|--------|
| Block time | 10-30s |
| Proof generation | <60s |
| Proof verification | <1s |
| Node count | 5-10 |
| Determinism | 100% (same input = same output) |

---

## Related RFCs

- RFC-0104: Deterministic Floating-Point
- RFC-0105: Deterministic Quant Arithmetic
- RFC-0106: Deterministic Numeric Tower
- RFC-0120: Deterministic AI-VM
- RFC-0130: Proof-of-Inference Consensus
- RFC-0131: Deterministic Transformer Circuit
- RFC-0143: OCTO-Network Protocol
- RFC-0147: Implementation Roadmap

---

## Summary

Genesis implementation proves:

> **The first network where AI inference secures consensus.**

Once this works, everything else (agents, markets, governance) can grow on top.

---

*This guide complements RFC-0147: Implementation Roadmap with a focused genesis strategy.*
