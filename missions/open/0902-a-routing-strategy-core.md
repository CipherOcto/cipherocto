# Mission: RFC-0902-a: Routing Strategy Core

## Status

Open

## RFC

RFC-0902 (Economics): Multi-Provider Routing and Load Balancing

## Dependencies

None - Core mission to start

## Acceptance Criteria

- [ ] SimpleShuffle strategy implementation
- [ ] Weighted random selection based on rpm/tpm
- [ ] RoundRobin strategy implementation
- [ ] Default strategy configuration
- [ ] Unit tests for routing strategies
- [ ] Integration tests with mock providers

## Description

Implement core routing strategies based on LiteLLM's simple_shuffle.py algorithm:

- Weighted random selection using `random.choices()` with weights
- Fallback to uniform random when no weights specified
- Round-robin via index rotation

## Technical Details

### SimpleShuffle Algorithm (LiteLLM reference)

```python
def simple_shuffle(healthy_deployments, model):
    # Check for weight/rpm/tpm
    for weight_by in ["weight", "rpm", "tpm"]:
        weight = healthy_deployments[0].get("litellm_params").get(weight_by)
        if weight is not None:
            weights = [m["litellm_params"].get(weight_by, 0) for m in healthy_deployments]
            total_weight = sum(weights)
            weights = [weight / total_weight for weight in weights]
            selected_index = random.choices(range(len(weights)), weights=weights)[0]
            return healthy_deployments[selected_index]

    # No weights - random pick
    return random.choice(healthy_deployments)
```

### Configuration

```yaml
router_settings:
  routing_strategy: "simple-shuffle"  # or "round-robin"
```

---

**Claimant:** Open

**Related RFCs:**
- RFC-0902: Multi-Provider Routing and Load Balancing
- RFC-0903: Virtual API Key System
- RFC-0904: Real-Time Cost Tracking
