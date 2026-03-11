# RFC-0113 (Retrieval): Retrieval Gateway & Query Routing

## Status

Draft

> **Note:** This RFC was originally numbered RFC-0113 under the legacy numbering system. It remains at 0113 as it belongs to the Retrieval category.

## Summary

This RFC defines the Retrieval Gateway and Query Routing system for the CipherOcto network.

The gateway acts as the **control plane for data retrieval**, responsible for:

- query routing
- node selection
- tier discovery
- verification policy enforcement
- economic optimization

The Retrieval Gateway enables unified access to:

- storage nodes
- vector search engines
- AI agent memory
- distributed datasets
- archival storage

## Design Goals

| Goal                   | Target                | Metric           |
| ---------------------- | --------------------- | ---------------- |
| **G1: Routing**        | Route to optimal node | Latency savings  |
| **G2: Node Selection** | Select best provider  | Quality score    |
| **G3: Tier Discovery** | Find appropriate tier | Match accuracy   |
| **G4: Verification**   | Enforce proof policy  | 100% enforcement |

## Performance Targets

| Metric             | Target   | Notes       |
| ------------------ | -------- | ----------- |
| Routing decision   | <10ms    | Per query   |
| Node selection     | <5ms     | With scores |
| Gateway throughput | >10k QPS | Per gateway |

## Motivation

### Problem Statement

Current systems lack:

- Unified query interfaces for heterogeneous data
- Tier-aware routing to storage layers
- Verification policy enforcement at the gateway level
- Cost-aware optimization for queries

### Desired State

The network needs a gateway that:

1. Provides unified query interface
2. Routes to appropriate storage tiers
3. Enforces data flag constraints
4. Optimizes for cost and latency
5. Handles failures gracefully

## Specification

### Role in System Architecture

```
Applications / AI Agents
         ↓
Retrieval Gateway (control plane)
         ↓
   Retrieval Nodes
         ↓
  Storage Providers
```

**Responsibilities by layer**:

| Layer             | Responsibility                 |
| ----------------- | ------------------------------ |
| Retrieval Gateway | Orchestrates retrieval process |
| Retrieval Nodes   | Execute queries                |
| Storage Providers | Persist data                   |

### Core Responsibilities

#### Query Parsing

Incoming requests may include:

- SQL queries
- vector search queries
- memory recall requests
- file retrieval

Example:

```sql
SELECT *
FROM embeddings
ORDER BY distance(vec, :query)
LIMIT 10
```

The gateway classifies the request type:

| Query Type | Engine                   |
| ---------- | ------------------------ |
| SQL        | distributed query engine |
| Vector     | ANN search               |
| File       | storage retrieval        |
| Memory     | agent memory store       |

#### Data Flag Enforcement

Queries inherit execution constraints from data classification flags.

| Data Flag    | Execution Policy       |
| ------------ | ---------------------- |
| PRIVATE      | local only             |
| CONFIDENTIAL | TEE execution          |
| SHARED       | verifiable computation |
| PUBLIC       | open execution         |

The gateway ensures that routing respects these constraints.

> ⚠️ **Example**:
>
> - `PRIVATE` → must execute locally
> - `CONFIDENTIAL` → route to enclave node
> - `SHARED` → requires verification proofs
> - `PUBLIC` → any node

#### Storage Tier Discovery

The gateway resolves which storage tier contains the requested data.

| Tier    | Token    | Latency |
| ------- | -------- | ------- |
| Hot     | OCTO-S-H | <10ms   |
| Cold    | OCTO-S-C | minutes |
| Archive | OCTO-H   | hours   |

> **Example**:
>
> - vector index → hot tier
> - historical dataset → cold tier
> - archive proof → archive tier

#### Node Selection

The gateway selects nodes based on several metrics.

| Metric       | Description           |
| ------------ | --------------------- |
| Latency      | Response time         |
| Cost         | Query cost in OCTO    |
| Verification | Supported proof types |
| Reputation   | PoR score             |
| Capacity     | Available bandwidth   |

#### Multi-Node Query Execution

Some queries require multiple stages.

Example AI retrieval pipeline:

```
vector search
     ↓
dataset fetch
     ↓
context assembly
```

Gateway orchestrates each stage.

## Routing Architecture

### Deterministic Routing

Used when verification is required.

Routing is deterministic to allow proof generation.

```
hash(query) → node set
```

**Benefits**:

- Verifiable execution
- Reproducible queries

### Adaptive Routing

Used for public workloads.

Gateway optimizes for:

- latency
- cost
- load balancing

## Retrieval Pipelines

Complex queries may use pipelines.

```
Query
    ↓
Vector search
    ↓
Metadata filtering
    ↓
Dataset retrieval
    ↓
Context assembly
```

Each stage may execute on different nodes.

## Verification Integration

The gateway coordinates verification processes.

| Level    | Mechanism                   |
| -------- | --------------------------- |
| Basic    | Merkle proof                |
| Verified | transcript + coverage proof |
| Trusted  | enclave attestation         |
| ZK       | zero-knowledge proof        |

The required level depends on the **data flag**.

## Query Cost Estimation

Before execution, the gateway estimates query cost.

| Component    | Description      |
| ------------ | ---------------- |
| Bandwidth    | data transfer    |
| Compute      | query execution  |
| Index lookup | vector search    |
| Verification | proof generation |

Example estimation:

```
vector search: 0.002 OCTO
dataset fetch: 0.004 OCTO
verification: 0.001 OCTO
─────────────────────
total: 0.007 OCTO
```

## Gateway Decentralization

The Retrieval Gateway is not a single node.

It is implemented as a **distributed routing layer**.

Possible implementations:

- Peer-to-peer routers
- Gateway clusters
- Client-side routing

Nodes may advertise gateway capability.

## Capability Advertisement

Nodes publish their capabilities.

```json
{
  "node_type": "retrieval-node",
  "roles": ["vector", "dataset"],
  "supported_indexes": ["HNSW"],
  "latency_ms": 8,
  "verification": ["Merkle", "ZK"],
  "max_qps": 4000
}
```

The gateway uses this metadata for routing.

## Failure Handling

The gateway must tolerate node failures.

| Strategy          | Description                  |
| ----------------- | ---------------------------- |
| Redundant routing | Multiple nodes for same data |
| Fallback nodes    | Secondary node selection     |
| Retry policies    | Automatic retry with backoff |
| Quorum retrieval  | Require N of M responses     |

> **Example**:
>
> ```
> primary node fails
>       ↓
> fallback node selected
> ```

## Security Considerations

### Threats

| Threat               | Description                          |
| -------------------- | ------------------------------------ |
| Malicious routing    | Gateway directs to compromised nodes |
| Incomplete retrieval | Partial data returned                |
| Manipulated rankings | Vector results altered               |
| Denial-of-service    | Gateway overwhelmed                  |

### Mitigations

- Deterministic routing
- Verification proofs
- Reputation systems
- Query sampling

## AI Agent Integration

AI agents rely heavily on retrieval.

```
User Query
        ↓
Agent reasoning
        ↓
Retrieval Gateway
        ↓
Vector + dataset retrieval
        ↓
Context assembly
        ↓
LLM inference
```

The gateway becomes the **data access layer for agent cognition**.

## Observability

Gateways must expose telemetry.

| Metric            | Description                    |
| ----------------- | ------------------------------ |
| Query latency     | End-to-end response time       |
| Success rate      | Queries completed successfully |
| Routing decisions | Node selection reasoning       |
| Cost distribution | Fees collected                 |

These metrics support:

- Network optimization
- Economic balancing

## Integration Points

### With RFC-0109 (Retrieval Architecture)

| Component         | Gateway Responsibility |
| ----------------- | ---------------------- |
| Storage retrieval | Route to storage nodes |
| Vector retrieval  | Route to ANN engines   |
| Agent memory      | Route to memory stores |
| Archive retrieval | Route to cold storage  |

### With Data Flags

The gateway enforces:

| Data Flag    | Routing Constraint         |
| ------------ | -------------------------- |
| PRIVATE      | Local-only execution       |
| CONFIDENTIAL | TEE-enabled nodes          |
| SHARED       | Verification-capable nodes |
| PUBLIC       | Any available node         |

## Adversarial Review

| Threat                   | Impact | Mitigation             |
| ------------------------ | ------ | ---------------------- |
| **Routing Manipulation** | High   | Multi-gateway fallback |
| **Node Collusion**       | Medium | Reputation + stake     |
| **Censorship**           | High   | Tier redundancy        |

## Alternatives Considered

| Approach                | Pros          | Cons                    |
| ----------------------- | ------------- | ----------------------- |
| **Centralized gateway** | Simple        | Single point of failure |
| **Gossip routing**      | Decentralized | Latency                 |
| **This approach**       | Hybrid        | Complexity              |

## Key Files to Modify

| File                     | Change         |
| ------------------------ | -------------- |
| src/gateway/router.rs    | Routing logic  |
| src/gateway/selector.rs  | Node selection |
| src/gateway/discovery.rs | Tier discovery |

## Future Extensions

Potential enhancements:

- Decentralized query marketplaces
- ZK-verifiable vector search
- Programmable routing policies
- AI-assisted routing optimization
- Adaptive data caching

## Summary

The Retrieval Gateway acts as the **orchestrator of data access in the CipherOcto network**.

It coordinates:

- Query execution
- Storage tier access
- Node selection
- Verification enforcement
- Cost optimization

This component enables the network to provide:

- Scalable AI data retrieval
- Verifiable computation
- Decentralized knowledge infrastructure

---

**Submission Date:** 2026-03-07
**Last Updated:** 2026-03-07

**Prerequisites**:

- RFC-0106 (Numeric/Math): Deterministic Numeric Tower
- RFC-0107 (Storage): Production Vector-SQL Storage v2
- RFC-0108 (Retrieval): Verifiable AI Retrieval
- RFC-0109 (Retrieval): Retrieval Architecture & Read Economics

**Related RFCs**:

- RFC-0103 (Numeric/Math): Unified Vector-SQL Storage
- RFC-0105 (Numeric/Math): Deterministic Quant Arithmetic

## Related Use Cases

- [Privacy-Preserving Query Routing](../../docs/use-cases/privacy-preserving-query-routing.md)
- [Data Marketplace](../../docs/use-cases/data-marketplace.md)

---

**Version:** 1.0
**Submission Date:** 2026-03-07
**Last Updated:** 2026-03-07
