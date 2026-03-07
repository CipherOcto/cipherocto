# RFC-0109: Retrieval Architecture & Read Economics

## Status

Draft

## Summary

This RFC defines the retrieval architecture for the CipherOcto network.

While storage responsibilities are handled by OCTO-S providers, retrieval is treated as a **cross-layer capability** spanning:

- storage nodes
- vector indexes
- AI agent memory
- dataset access
- archival proofs

The system separates:

- **Storage** = data persistence
- **Retrieval** = query execution + data delivery + verification

This RFC formalizes:

- retrieval roles
- read economics
- verification guarantees
- execution policy integration
- storage tier routing

## Motivation

### Problem Statement

Current systems conflate storage and retrieval, leading to:

- No clear economics for read operations
- Limited verification guarantees
- Poor integration with AI agent workflows
- No tiered retrieval routing

### Desired State

The network should have:

1. Clear retrieval roles separate from storage
2. Economic incentives for retrieval nodes
3. Verification at multiple trust levels
4. Integration with data flag system
5. Tiered routing based on storage type

## Specification

### Storage vs Retrieval

Storage nodes are responsible for:

- data persistence
- replication
- proof-of-storage
- durability guarantees

Retrieval nodes are responsible for:

- query execution
- vector search
- data decoding
- bandwidth delivery
- retrieval verification

> **Note**: A single node MAY perform both roles.

## Retrieval Roles

### Storage Retrieval Node

**Primary role**: Serve raw stored data.

| Capability           | Description                |
| -------------------- | -------------------------- |
| file retrieval       | Direct file access         |
| dataset streaming    | Large dataset delivery     |
| shard reconstruction | Erasure coding recovery    |
| erasure decoding     | Reconstruct missing shards |

**Typical data**: datasets, model weights, archives, logs

**Verification**: Merkle inclusion proof + proof-of-storage linkage

### Vector Retrieval Node

**Primary role**: Similarity search over embeddings.

| Capability          | Description                  |
| ------------------- | ---------------------------- |
| HNSW search         | Approximate nearest neighbor |
| hybrid queries      | SQL + vector combined        |
| ANN retrieval       | Scalable vector search       |
| embedding filtering | Metadata pre-filtering       |

**Data types**: embeddings, semantic indexes, knowledge bases

**Verification**: vector commitment proofs, Merkle index verification, ZK retrieval proofs (optional)

### Agent Memory Retrieval Node

**Primary role**: AI memory recall.

| Capability          | Description              |
| ------------------- | ------------------------ |
| episodic memory     | Event sequence retrieval |
| conversation memory | Chat history recall      |
| knowledge recall    | Factual retrieval        |
| semantic ranking    | Relevance scoring        |

**Data structure**: memory graph, vector store, structured metadata

**Latency requirement**: < 50ms

**Verification**: Depends on data classification flag

### Archive Retrieval Node

**Primary role**: Historical data access.

| Capability                 | Description                |
| -------------------------- | -------------------------- |
| large-scale reconstruction | Full dataset rebuild       |
| cold storage access        | Tier-2 retrieval           |
| proof-of-existence         | Cryptographic verification |

**Latency**: minutes to hours

**Verification**: proof-of-spacetime, archival commitment proof

## Storage Tier Integration

### Storage Tier Model

| Tier    | Token    | Technology           | Latency | Typical Use                    |
| ------- | -------- | -------------------- | ------- | ------------------------------ |
| Hot     | OCTO-S-H | NVMe / memory / edge | <10ms   | active datasets, embeddings    |
| Cold    | OCTO-S-C | HDD arrays           | minutes | backups, historical data       |
| Archive | OCTO-H   | erasure coded        | hours   | compliance, proof-of-existence |

### Tier Routing

**Hot Tier**:

- agent memory
- embeddings
- active tables
- frequently accessed data

**Cold Tier**:

- historical datasets
- backups
- less-frequently accessed data

**Archive Tier**:

- compliance archives
- historical proofs
- long-term retention

## Retrieval Execution Policies

Execution policy derives from **Data Flags** (see whitepaper):

| Data Flag    | Execution Policy | Retrieval Verification      |
| ------------ | ---------------- | --------------------------- |
| PRIVATE      | LOCAL            | ZK proof of local execution |
| CONFIDENTIAL | TEE              | remote attestation          |
| SHARED       | VERIFIED         | Merkle + ZK coverage proof  |
| PUBLIC       | OPEN             | optional verification       |

> ⚠️ **Integration**: These policies integrate with RFC-0108 (Verifiable AI Retrieval) to provide verification guarantees.

## Retrieval Query Types

### File Retrieval

```http
GET /storage/{cid}
```

Returns:

- file stream
- Merkle inclusion proof

### Dataset Query

```sql
SELECT * FROM dataset
WHERE timestamp > NOW() - 1h
```

Executed via: distributed SQL over storage shards

### Vector Search

```sql
SELECT id
FROM embeddings
ORDER BY distance(vec, :query)
LIMIT 10
```

Execution: HNSW search + filtered vector retrieval

### Agent Memory Recall

```sql
RECALL memory
WHERE topic = 'cryptography'
LIMIT 5
```

Execution: vector + metadata ranking

## Retrieval Verification

### Basic Verification

**Proofs**:

- Merkle inclusion
- shard integrity

**Used for**: PUBLIC data

### Verified Retrieval

**Proofs**:

- Merkle inclusion
- query transcript
- coverage proof

**Used for**: SHARED datasets

### Trusted Execution Retrieval

**Proofs**:

- enclave attestation
- encrypted computation

**Used for**: CONFIDENTIAL data

### ZK Retrieval

**Proofs**:

- zk-SNARK query proof
- vector search correctness

**Used for**: high-assurance AI pipelines

## Retrieval Economics

Storage rewards handle **write operations**.

Retrieval introduces **read bandwidth markets**.

### Retrieval Fees

Users pay for:

| Operation      | Cost Driver |
| -------------- | ----------- |
| File retrieval | bandwidth   |
| Vector search  | compute     |
| Dataset query  | CPU + IO    |

### Fee Distribution

| Recipient        | Share |
| ---------------- | ----- |
| Retrieval node   | 40%   |
| Storage provider | 40%   |
| Network treasury | 20%   |

## Retrieval Marketplace

Nodes may advertise capabilities.

```json
{
  "node_type": "vector-retrieval",
  "max_qps": 5000,
  "latency_ms": 8,
  "supported_indexes": ["HNSW", "IVF"],
  "verification": ["Merkle", "ZK"]
}
```

Query routers select optimal nodes based on:

- latency requirements
- cost
- verification level
- reputation score

## AI Integration

Retrieval is critical for AI agent workflows.

```
Agent
  ↓
Retriever
  ↓
Vector index
  ↓
Dataset fetch
  ↓
Context assembly
  ↓
Agent processing
```

### Agent Memory Types

| Memory Type | Retrieval Node   | Latency |
| ----------- | ---------------- | ------- |
| Episodic    | Agent Memory     | <50ms   |
| Semantic    | Vector Retrieval | <100ms  |
| Working     | Hot Storage      | <10ms   |

## Security Considerations

### Risks

| Risk                 | Description                          |
| -------------------- | ------------------------------------ |
| Data poisoning       | Malicious data inserted into storage |
| Incomplete retrieval | Partial data returned                |
| Malicious ranking    | Vector results manipulated           |
| Censorship           | Selective data withholding           |

### Mitigations

- Retrieval proofs (Merkle + ZK)
- Reputation scoring (PoR)
- Redundant retrieval from multiple nodes
- Verification sampling

## Integration Points

### With RFC-0106 (Numeric Tower)

| Numeric Tower | Retrieval Use                      |
| ------------- | ---------------------------------- |
| DFP           | Deterministic distance computation |
| DQA           | ZK-compatible arithmetic           |
| DVEC          | Vector commitment structure        |

### With RFC-0107 (Vector Storage)

| Vector Storage | Retrieval Use       |
| -------------- | ------------------- |
| HNSW index     | ANN search          |
| Segments       | Shard retrieval     |
| Merkle root    | Verification proofs |

### With RFC-0108 (Verifiable Retrieval)

| Verifiable Retrieval | Integration              |
| -------------------- | ------------------------ |
| Coverage proofs      | ANN verification         |
| ZK circuits          | High-assurance pipelines |
| Transcript hash      | Query integrity          |

## Future Work

Potential future extensions:

- Decentralized retrieval marketplaces
- Query bandwidth trading
- ZK-vector search
- Verifiable ranking proofs
- Programmable retrieval pipelines

## Summary

Retrieval in CipherOcto is a **network capability**, not a single service.

It integrates:

- storage nodes
- vector indexes
- AI memory systems
- verification layers

By separating **storage economics** from **retrieval economics**, the network enables:

- scalable AI data access
- verifiable machine learning pipelines
- decentralized knowledge infrastructure

---

**Submission Date:** 2026-03-07
**Last Updated:** 2026-03-07

**Prerequisites**:

- RFC-0106: Deterministic Numeric Tower
- RFC-0107: Production Vector-SQL Storage v2
- RFC-0108: Verifiable AI Retrieval

**Related RFCs**:

- RFC-0100: AI Quota Marketplace Protocol
- RFC-0103: Unified Vector-SQL Storage
- RFC-0105: Deterministic Quant Arithmetic
