# Mission: 0851p-a — Bootstrap node slashing

## Status

Open (2026-06-16) — post-launch

## RFC

RFC-0851p-a (Networking): Network Bootstrap — §"Future Work"

## Summary

Extend slash reason codes (defined in RFC-0855p-b §B "Slash Offense Codes") with `0x000D` = `bootstrap_node_misbehavior`. Bootstrap nodes that misbehave (e.g., withhold peers, serve stale data, censor, lie about their reachability) are slashed and removed from the seed list.

## Design

1. **New slash reason code:**
   - `0x000D` = `bootstrap_node_misbehavior`
   - Range allocation: `0x000A-0x000B` is transport-level (RFC-0850p-c §6), `0x000C-0xFFFF` is reserved. We claim `0x000D` for bootstrap-specific misbehavior. The reservation policy is in RFC-0855p-b §B.
2. **Misbehavior types (defined per code):**
   - `0x000D` = `bootstrap_node_misbehavior` (general; details in `slash_reason_data` field)
   - Sub-codes in `slash_reason_data`:
     - `0x000D.01` = `withholds_peers` (claims 0 reachable peers when it has > 0)
     - `0x000D.02` = `stale_data` (serves seed list older than `MAX_SEED_AGE_EPOCHS`)
     - `0x000D.03` = `censors_legit_peer` (refuses to include a specific peer that other seeds have)
     - `0x000D.04` = `false_reachability_claim` (claims a peer is reachable when it is not)
3. **Slash mechanism:**
   - Witnesses (2/3 majority per RFC-0855p-b) vote to slash the bootstrap node.
   - Slashed node is removed from the seed list (rejected at load time).
   - Slashed node's `peer_id` is added to a local blacklist.
4. **Recovery:** Slashed bootstrap nodes can appeal via a governance vote (RFC-0855 §11).

## Acceptance Criteria

- [ ] `0x000D` slash reason code in RFC-0855p-b §B (extends the table)
- [ ] `slash_reason_data: u32` field in `SlashEnvelope`
- [ ] `crates/octo-bootstrap/src/seed_list.rs::load_and_validate` — reject slashed seeds
- [ ] Witness voting flow in `crates/octo-network/src/mon/slash.rs`
- [ ] Unit tests: each misbehavior sub-code, witness vote aggregation, slash finalization
- [ ] Documentation: how bootstrap nodes can avoid being slashed (best practices)
- [ ] Documentation: operator guide for reviewing slash evidence

## Mitigates

D-NB-3 (malicious seed list operator); D-NB-6 (Sybil via compromised bootstrap nodes).

## Deadline

Post-launch
