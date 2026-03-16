# Mission: RFC-0902-b: Advanced Routing Strategies

## Status

Claimed

## RFC

RFC-0902 (Economics): Multi-Provider Routing and Load Balancing

## Dependencies

- Mission-0902-a: Routing Strategy Core

## Acceptance Criteria

- [x] LeastBusy strategy implementation
- [x] LatencyBased routing implementation
- [x] CostBased routing (requires RFC-0904) - placeholder
- [x] UsageBased routing (RPM/TPM tracking)
- [x] Request counting for LeastBusy
- [x] Latency tracking window
- [x] Unit tests for each strategy

## Description

Implement advanced routing strategies based on LiteLLM's implementations:

- **LeastBusy**: Track in-flight requests per deployment, pick lowest
- **LatencyBased**: Track rolling latency window, pick fastest
- **CostBased**: Route to cheapest (requires RFC-0904 pricing)
- **UsageBased**: Route based on current RPM/TPM usage

## Technical Details

### LeastBusy Algorithm (LiteLLM reference)

Tracks requests in flight:
- Increment on `log_pre_api_call` (request starts)
- Decrement on `log_success_event` / `log_failure_event` (request ends)
- Pick deployment with lowest count

### LatencyBased Algorithm (LiteLLM reference)

```python
class LowestLatencyLoggingHandler:
    def log_success_event(self, kwargs, response_obj, start_time, end_time):
        # Update latency cache per model_group + deployment_id
        latency = (end_time - start_time).total_seconds()
        # Maintain rolling window of latencies
        # Pick deployment with lowest average
```

### Configuration

```yaml
router_settings:
  routing_strategy: "least-busy"  # or "latency-based", "cost-based", "usage-based"
  latency_window: 10  # Track last N requests
```

## Implementation Notes

**Enhancements from Mission-0902-a:**
- Added request tracking (request_started, request_ended)
- Added latency window trimming for LatencyBased
- Added usage reset for sliding window RPM/TPM
- Added 3 new tests: latency_based_routing, usage_based_routing, request_tracking

**Advanced strategies implemented:**
- LeastBusy: Tracks active_requests, picks provider with fewest in-flight
- LatencyBased: Tracks rolling latency window, picks fastest avg
- UsageBased: Tracks current RPM, picks lowest current usage
- CostBased: Placeholder (falls back to SimpleShuffle until RFC-0904)

**Tests:** 7 new/enhanced tests passing (17 total)

---

**Claimant:** @claude-code

**Pull Request:** #

**Related RFCs:**
- RFC-0902: Multi-Provider Routing and Load Balancing
- RFC-0904: Real-Time Cost Tracking (for CostBased)
