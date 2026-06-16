# Mission: 0851p-a — Seed list health check at load

## Status

Open (2026-06-16) — pre-public-launch

## RFC

RFC-0851p-a (Networking): Network Bootstrap — §"Future Work" (mitigates IA-NB-11)

## Summary

At `start_node`, verify that each seed in the seed list has been signed within the last `MAX_SEED_AGE_EPOCHS = 10` epochs (~1 hour at 1-minute epochs). Stale seeds (older than 10 epochs) are rejected. If > 20% of seeds are stale, log a high-severity alert and emit a metric `seed_stale_ratio` — this is a Sybil/eclipse attack signal (an attacker who controls the seed list could be feeding you stale data).

## Design

1. On `start_node`, after loading the seed list:
   - For each seed, check `seed.signed_at_epoch >= current_epoch - MAX_SEED_AGE_EPOCHS`.
   - If stale, drop the seed and add to a `stale_seeds: Vec<StaleSeed>` list.
2. Compute `stale_ratio = stale_seeds.len() / total_seeds`.
3. If `stale_ratio > 0.2`:
   - Log at WARN level with seed IDs and timestamps
   - Emit metric `seed_stale_ratio` (Prometheus)
   - Optionally: refuse to start (configurable; default: log + start)
4. If the entire seed list is stale (100% stale), refuse to start with `SEED_LIST_FULLY_STALE` — this is a strong signal that the seed list service is down or the clock is wrong.
5. Add `MAX_SEED_AGE_EPOCHS = 10` to `crates/octo-bootstrap/src/config.rs`.

## Acceptance Criteria

- [ ] `MAX_SEED_AGE_EPOCHS = 10` constant
- [ ] Stale seed detection in `crates/octo-bootstrap/src/seed_list.rs::load_and_validate`
- [ ] `seed_stale_ratio` metric (Prometheus)
- [ ] `SEED_LIST_FULLY_STALE` error variant
- [ ] Unit tests: 0% stale (pass), 20% stale (log only), 50% stale (log + alert), 100% stale (refuse to start)
- [ ] Documentation: operator guide for investigating stale seed alerts

## Mitigates

IA-NB-11 (Sybil/eclipse via stale seeds)

## Deadline

Pre-public-launch
