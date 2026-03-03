# Use Case: Provable Quality of Service (QoS)

## Problem

Current service quality relies on trust:
- Latency claims unverified
- SLA violations difficult to prove
- Dispute resolution based on reputation
- No cryptographic proof of service delivery

## Motivation

### Why This Matters for CipherOcto

1. **Dispute resolution** - Cryptographic proof vs trust
2. **SLA enforcement** - Automatic compensation
3. **Provider differentiation** - Quality verifiable on-chain
4. **Enterprise confidence** - Guaranteed service levels

### The Opportunity

- Enterprise users pay premium for guarantees
- DeFi requires verifiable execution
- Compliance needs audit trails

## Quality Metrics

### Verifiable Metrics

| Metric | Proof Method | On-chain Settleable |
|--------|--------------|---------------------|
| **Latency** | Timestamp + hash | ✅ Auto-refund |
| **Uptime** | Block inclusion | ✅ SLA penalties |
| **Output validity** | Shape/content proof | ✅ Dispute resolution |
| **Routing correctness** | Merkle path | ✅ Payment release |
| **Model execution** | zkML proof | ✅ Quality bonus |

### Latency Proof

```rust
struct LatencyProof {
    // Timestamps (block-based)
    request_received: u64,    // Block timestamp
    response_sent: u64,       // Block timestamp

    // What was processed (hash, not content)
    request_hash: FieldElement,
    response_hash: FieldElement,

    // Quality indicators
    provider: Address,
    model: String,

    // Verification
    block_hashes: Vec<FieldElement>,  // Merkle path
    signature: Signature,
}

impl LatencyProof {
    fn calculate_latency(&self) -> u64 {
        self.response_sent - self.request_received
    }

    fn verify(&self) -> bool {
        // Verify block timestamps
        // Verify Merkle inclusion
        // Verify signature
        true
    }
}
```

### Uptime Proof

```mermaid
sequenceDiagram
    Network->>Provider: Ping (every block)
    Provider->>Network: Pong + signature
    Network->>Network: Record uptime

    Note over Network: Continuous = 100%
    Note over Network: <99.9% = SLA violation

    alt SLA Violation
        Network->>Escrow: Slash X%
        Escrow->>User: Auto-refund
    end
```

### Output Validity Proof

```rust
struct OutputValidityProof {
    // What was requested
    request_hash: FieldElement,

    // What was returned
    output_hash: FieldElement,

    // Validity checks
    checks: Vec<ValidityCheck>,
}

enum ValidityCheck {
    ValidJson,
    ValidSchema(Schema),
    MaxSize(u64),
    ContainsField(String),
    ValidTokenCount(u64),
}

impl OutputValidityProof {
    fn verify(&self, output: &[u8]) -> bool {
        self.checks.iter().all(|check| check.validates(output))
    }
}
```

## SLA Structure

### Service Levels

| Tier | Latency | Uptime | Output Validity | Price |
|------|---------|--------|------------------|-------|
| **Basic** | < 10s | 99% | Best effort | 1x |
| **Standard** | < 5s | 99.9% | Guaranteed | 1.5x |
| **Premium** | < 1s | 99.99% | Verified | 2x |
| **Enterprise** | < 500ms | 99.999% | Fully proven | 4x |

### SLA Penalties

```mermaid
flowchart TD
    VIOLATION[SLA Violation Detected] --> CHECK[Check Severity]

    CHECK -->|Latency| LATENCY[Latency Penalty]
    CHECK -->|Uptime| UPTIME[Uptime Penalty]
    CHECK -->|Output| OUTPUT[Output Penalty]

    LATENCY -->|5-10%| P5[5% refund]
    LATENCY -->|10-25%| P10[10% refund]
    LATENCY -->|>25%| P25[25% refund]

    UPTIME -->|99-99.9%| U5[5% refund]
    UPTIME -->|95-99%| U10[10% refund]
    UPTIME -->|<95%| U25[Full refund]

    OUTPUT -->|Invalid| OF[Full refund + penalty]
    OUTPUT -->|Missing| OM[Partial refund]
```

## On-chain Settlement

### Escrow Mechanism

```mermaid
flowchart LR
    subgraph STAKE["Provider Stake"]
        PROVIDER[Provider] -->|deposit| ESCROW[Escrow Contract]
    end

    subgraph EXECUTE["Execution"]
        USER[User] -->|request| ROUTER[Router]
        ROUTER -->|route| PROVIDER
        PROVIDER -->|execute| RESULT[Result + Proof]
    end

    subgraph VERIFY["Verification"]
        RESULT -->|submit| ESCROW
        ESCROW -->|verify| VERIFIER[Verifier]
    end

    subgraph SETTLE["Settlement"]
        VERIFIER -->|valid| PAY[Pay Provider]
        VERIFIER -->|invalid| REFUND[Refund User]
    end
```

### Smart Contract Logic

```cairo
#[starknet::contract]
mod QoSContract {
    struct Storage {
        provider_stake: u256,
        total_requests: u64,
        sla_violations: u64,
    }

    #[external]
    fn verify_and_settle(
        proof: QualityProof,
        user: address
    ) -> u256 {
        // 1. Verify proof
        assert(verify_proof(proof), 'Invalid proof');

        // 2. Calculate penalty if any
        let penalty = calculate_penalty(proof);

        // 3. Settle
        if penalty > 0 {
            slash_provider(penalty);
            refund_user(user, penalty);
        } else {
            pay_provider(proof.amount);
        }

        penalty
    }
}
```

## Dispute Resolution

### Challenge Flow

```mermaid
sequenceDiagram
    User->>Protocol: "Service was below SLA"
    Protocol->>User: "Submit proof or claim"
    User->>Protocol: "Here is my proof"

    alt User Has Proof
        Protocol->>Verifier: Verify
        alt Proof Valid
            Protocol->>Provider: Slash + Refund
        else Proof Invalid
            Protocol->>User: Claim rejected
        end

    else User No Proof
        Protocol->>Arbitration: Escalate
        Arbitration->>Provider: "Submit counter-proof"
        Arbitration->>Arbitration: Judge
    end
```

### Arbitration Levels

| Level | Description | Resolution Time |
|-------|-------------|----------------|
| **Automated** | On-chain verification | < 1 minute |
| **Evidence** | Both parties submit proof | < 24 hours |
| **Arbitration** | Third-party judge | < 7 days |
| **Appeals** | DAO vote on edge cases | < 30 days |

## Quality Scoring

### Provider Reputation Integration

```rust
struct QualityScore {
    // Raw metrics
    total_requests: u64,
    successful_requests: u64,
    avg_latency_ms: u64,
    uptime_percent: f64,

    // SLA performance
    sla_violations: u64,
    sla_fulfilled: u64,

    // Proof quality
    proofs_submitted: u64,
    proofs_valid: u64,

    // Calculated
    score: u8,
    tier: QualityTier,
}

enum QualityTier {
    Basic,      // < 50
    Standard,   // 50-75
    Premium,    // 75-90
    Elite,      // > 90
}

impl QualityScore {
    fn calculate(&mut self) {
        let sla_score = (self.sla_fulfilled as f64 / self.total_requests as f64) * 100.0;
        let proof_score = (self.proofs_valid as f64 / self.proofs_submitted as f64) * 100.0;
        let latency_score = if self.avg_latency_ms < 1000 { 100 } else { 50 };

        self.score = ((sla_score * 0.4) + (proof_score * 0.4) + (latency_score * 0.2)) as u8;
        self.tier = match self.score {
            0..=50 => QualityTier::Basic,
            51..=75 => QualityTier::Standard,
            76..=90 => QualityTier::Premium,
            _ => QualityTier::Elite,
        };
    }
}
```

### Quality Display

```mermaid
flowchart TD
    subgraph PROVIDER["Provider Listing"]
        NAME[Provider Name]
        SCORE[Quality Score: 85/100]
        METRICS[Uptime: 99.9%<br/>Latency: 450ms<br/>SLA: 98%]
        TIER[Tier: Premium]
    end

    style SCORE fill:#27ae60
    style TIER fill:#1f618d
```

## Integration with CipherOcto

### Modified Request Flow

```mermaid
sequenceDiagram
    User->>Router: Request (with SLA tier)
    Router->>Router: Check provider quality scores
    Router->>Provider: Route to qualified provider
    Provider->>Provider: Execute + generate proofs
    Provider->>Escrow: Submit proof + stake
    Escrow->>Router: Verification result
    Router->>User: Deliver result + proof

    alt SLA Met
        Router->>Provider: Release payment
    else SLA Violated
        Router->>Escrow: Trigger penalty
        Escrow->>User: Auto-refund
    end
```

### Token Economics

| Component | Token | Purpose |
|-----------|-------|---------|
| Provider stake | OCTO | Security deposit |
| Payment | OCTO-W | For execution |
| Bonuses | OCTO | For exceeding SLA |
| Penalties | OCTO | Slashed for violations |

## Implementation Path

### Phase 1: Basic QoS
- [ ] Timestamp-based latency proofs
- [ ] Block inclusion for uptime
- [ ] Basic SLA penalties
- [ ] Manual dispute submission

### Phase 2: Automated Verification
- [ ] On-chain proof verification
- [ ] Automatic refund triggers
- [ ] Quality score calculation
- [ ] Provider tiering

### Phase 3: Full SLA
- [ ] zkML output validation
- [ ] Real-time verification
- [ ] Complete arbitration system
- [ ] Enterprise SLA contracts

---

**Status:** Draft
**Priority:** High (improves trust)
**Token:** OCTO, OCTO-W
**Research:** [LuminAIR Analysis](../research/luminair-analysis.md)
