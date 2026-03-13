# Mission: RFC-0902-b: Advanced Routing Strategies

## Status

Open

## RFC

RFC-0902 (Economics): Multi-Provider Routing and Load Balancing

## Dependencies

- Mission-0902-a: Routing Strategy Core

## Acceptance Criteria

- [ ] LeastBusy strategy implementation
- [ ] LatencyBased routing implementation
- [ ] CostBased routing (requires RFC-0904)
- [ ] UsageBased routing (RPM/TPM tracking)
- [ ] Request counting for LeastBusy
- [ ] Latency tracking window
- [ ] Unit tests for each strategy

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

---

**Claimant:** Open

**Related RFCs:**
- RFC-0902: Multi-Provider Routing and Load Balancing
- RFC-0904: Real-Time Cost Tracking (for CostBased)
