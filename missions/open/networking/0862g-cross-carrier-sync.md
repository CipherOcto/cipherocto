# Mission: 0862g — Cross-Carrier Sync

## Status

Draft (awaiting adversarial review)

## RFC

RFC-0862 (Networking): Stoolap Data Sync Protocol — §Implementation Phases Phase 4, §System Architecture (transport adapters)

## Summary

Extend the Sync protocol to ride on multiple DOT platform adapters simultaneously (NativeP2P + Webhook + one social adapter). The same sync stream is replicated across carriers, providing automatic failover when one carrier is blocked or unreachable.

This is **Phase 4** of RFC-0862. It builds on Phase 3 (multi-peer) by adding carrier diversity.

## Design

### New module: `crates/octo-sync/src/carrier.rs`

```rust
use std::collections::HashMap;
use std::time::Instant;

use futures::future;
use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::{BroadcastDomainId, DeterministicEnvelope};

use crate::error::{Result, SyncError};

pub struct MultiCarrierSync {
    primary: Box<dyn PlatformAdapter>,
    secondary: Vec<Box<dyn PlatformAdapter>>,
    health: parking_lot::Mutex<HashMap<String, CarrierHealth>>,
    mission_id: [u8; 32],
}

struct CarrierHealth {
    last_heartbeat: Instant,
    last_successful_send: Instant,
    success_rate: f64,  // over the last 100 attempts
    avg_latency_ms: f64,
}

impl MultiCarrierSync {
    /// Send a Sync envelope via all healthy carriers.
    /// The envelope is wrapped in a `DeterministicEnvelope` and sent via the
    /// `PlatformAdapter::send_envelope` trait method (the actual method takes a
    /// broadcast domain ID, which is derived from the mission_id in v1).
    pub async fn broadcast(&self, envelope: DeterministicEnvelope) -> Result<()> {
        let domain = BroadcastDomainId::from_mission_id(self.mission_id);
        let mut tasks = Vec::new();
        let mut health = self.health.lock();
        for (carrier_name, carrier_health) in health.iter() {
            if carrier_health.success_rate < 0.5 {
                continue;  // skip unhealthy carriers
            }
            let carrier = self.carrier_by_name(carrier_name)?;
            tasks.push(carrier.send_envelope(&domain, &envelope));
        }
        // Wait for at least one to succeed; tolerate failures
        let results = futures::future::join_all(tasks).await;
        let any_success = results.iter().any(|r| r.is_ok());
        if !any_success {
            return Err(SyncError::AllCarriersFailed);
        }
        Ok(())
    }

    /// Periodic tick: rebalance carriers based on health.
    /// Takes &self (not &mut self) because the underlying state is in Mutex<>.
    pub async fn tick(&self) -> Result<()> {
        // 1. Measure carrier health
        let health_snapshot: Vec<(String, f64)> = {
            let health = self.health.lock();
            health.iter().map(|(name, h)| (name.clone(), h.success_rate)).collect()
        };
        // 2. Demote unhealthy carriers to secondary (separate from the mutation)
        // Implementation: the actual demotion happens in a separate `demote_carrier` method
        // that takes &mut self, called from a higher-level coordinator.
        Ok(())
    }

    fn carrier_by_name(&self, name: &str) -> Result<&Box<dyn PlatformAdapter>> {
        if self.primary.name() == name {
            Ok(&self.primary)
        } else {
            self.secondary.iter()
                .find(|c| c.name() == name)
                .ok_or(SyncError::UnknownCarrier(name.to_string()))
        }
    }
}
```

### Carrier selection

Default carriers (operator-configurable):
1. **Primary:** NativeP2P (libp2p gossipsub, RFC-0850 §3.1 0x000A)
2. **Secondary:** Webhook (HTTP, RFC-0850 §3.1 0x0009 — note: Webhook is 0x0009, not 0x000B; 0x000B is Bluetooth)
3. **Tertiary:** One social adapter (Telegram, Discord, Matrix, etc.) per operator config

### Health-based failover

Per-carrier health is tracked:
- **Heartbeat:** 30s
- **Success rate:** over the last 100 attempts
- **Average latency:** over the last 100 attempts

A carrier is demoted to secondary when:
- 3 consecutive failed sends, OR
- Success rate < 80% over 100 attempts, OR
- Average latency > 5s

A carrier is promoted to primary when:
- 10 consecutive successful sends, AND
- Success rate > 95% over 100 attempts, AND
- Average latency < 500ms

## Acceptance Criteria

- [ ] `crates/octo-sync/src/carrier.rs` exists with `MultiCarrierSync` struct
- [ ] `broadcast(envelope)` sends via all healthy carriers concurrently
- [ ] `broadcast` returns `Ok` if at least one carrier succeeds
- [ ] `broadcast` returns `SyncError::AllCarriersFailed` if all carriers fail
- [ ] `tick()` runs every 30s: measures health, demotes/promotes carriers
- [ ] Default carrier config: NativeP2P primary, Webhook secondary, one social tertiary
- [ ] Operator can override carrier config via `SyncConfig`
- [ ] Per-carrier health is tracked (heartbeat, success rate, latency)
- [ ] Health-based failover thresholds are operator-tunable
- [ ] Unit tests for: broadcast, health tracking, failover logic
- [ ] Integration test: 2 carriers (NativeP2P + Webhook); kill one; sync continues via the other

## Tests

- **Unit:**
  - `broadcast` sends to all healthy carriers
  - `broadcast` returns `Ok` when at least one succeeds
  - `broadcast` returns `AllCarriersFailed` when all fail
  - `tick()` measures health correctly
  - `tick()` demotes carrier with 3 consecutive failures
  - `tick()` demotes carrier with success rate < 80%
  - `tick()` demotes carrier with latency > 5s
  - `tick()` promotes secondary with 10 consecutive successes
  - `tick()` doesn't promote a secondary with success rate < 95%
  - `tick()` doesn't promote a secondary with latency > 500ms

- **Integration:**
  - 2 carriers (NativeP2P + Webhook); writer commits 1000 rows; reader applies; both carriers succeeded
  - 2 carriers; kill NativeP2P mid-sync; sync continues via Webhook
  - 2 carriers; restore NativeP2P after 1 min; carrier auto-promoted to primary
  - 1 carrier (Webhook only, no NativeP2P); sync still works (single carrier is the fallback)

## Dependencies

- **Requires:**
  - `0862-base` — Sync engine
  - `0862f` — multi-peer (for DGP integration with multiple carriers)
  - RFC-0850 §3.1 (platform types)
  - RFC-0850 §8.7 (QUIC profile, if NativeP2P uses QUIC)

- **Required by:** none (this is the last sync-related mission)

## Blockers / Dependencies

- **Blocked by:** `0862-base`, `0862f`
- **Blocks:** none

## Description

Phase 4 of RFC-0862 extends the Sync protocol to ride on multiple DOT platform adapters simultaneously. The same sync stream is replicated across carriers, providing automatic failover when one carrier is blocked or unreachable. This is the last sync-related mission; the remaining work (F1–F10) is future work beyond RFC-0862.

## Technical Details

### Performance

- **Bandwidth:** N × per-carrier bandwidth (linear in the number of carriers)
- **Latency:** min(carrier latencies); the first carrier to ACK counts
- **Cost:** N × per-carrier cost (operator-tunable to limit expensive carriers)

### Why multiple carriers?

A single carrier can be blocked (e.g., Telegram in some jurisdictions, WhatsApp during outages). Multi-carrier ensures the sync stream survives such blockages. Per RFC-0862 §Implementation Phases Phase 4, "automatic failover to alternate carriers" is a primary goal.

### Why health-based (not random) failover?

Random failover would thrash between carriers on transient failures. Health-based failover uses a moving average to make stable decisions.

### Pitfalls

- **Don't broadcast to all carriers always.** The operator can configure a cost cap (e.g., "max 2 active carriers"); respect it.
- **Don't use the same nonce for different carriers.** Each carrier has its own replay cache; the nonce space is per-carrier.
- **Don't trust carrier ACKs for ordering.** Different carriers have different latencies; the receiver must order envelopes by their LSN, not by arrival time.
- **Don't fail the broadcast if a single carrier is slow.** Wait for at least one ACK; let the slow ones fail their health check.

---

**Mission Type:** Implementation
**Priority:** Medium
**Phase:** 4 (Cross-carrier, N-node, mission-aware)
**RFC Section Coverage:** §Implementation Phases Phase 4, §System Architecture (transport adapters)

## Type Coverage

This mission implements the following RFC-0862 types:

| Type | Role in this mission |
|------|---------------------|
| `MultiCarrierSync` | The broadcaster that fans out Sync envelopes to all healthy carriers (NativeP2P, Webhook, one social adapter) |
| `CarrierHealth` (per-carrier) | Per-carrier health tracking: `last_heartbeat`, `last_successful_send`, `success_rate`, `avg_latency_ms` |

The mission does NOT implement the underlying `PlatformAdapter` (NativeP2P, Webhook, etc.) — those are part of the DOT framework (RFC-0850). This mission only coordinates them. See the Type Coverage table in 0862-base for the full mapping.
