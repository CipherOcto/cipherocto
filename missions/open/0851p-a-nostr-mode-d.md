# Mission: 0851p-a — Mode D = NIP-05 / Nostr pubkey bootstrap

## Status

Open (2026-06-16) — future

## RFC

RFC-0851p-a (Networking): Network Bootstrap — §"Future Work"

## Summary

A new `bootstrap_mode = Nostr` configuration. The bootstrap adapter resolves a NIP-05 identifier (e.g., `user@example.com`) to a Nostr pubkey, fetches the user's contact list (kind 3 events), and treats each contact as a potential bootstrap peer. The peer is verified by checking that the contact has signed a `DOT capability claim` (a Nostr event of kind 30078 with a DOT-specific `d` tag).

## Design

1. Add `bootstrap_mode: Nostr` to `BootstrapConfig`.
2. New adapter `crates/octo-bootstrap/src/mode/nostr.rs`:
   - Takes a NIP-05 identifier `user@domain`
   - Resolves to Nostr pubkey via `https://domain/.well-known/nostr.json?name=user`
   - Fetches kind 3 contact list from public Nostr relays (configurable list)
   - For each contact pubkey, fetches their kind 30078 events with `d` tag = `dot-capability`
   - Verifies the DOT capability claim signature
   - Treats contacts with valid DOT capability as bootstrap peers
3. Verification: the contact's DOT capability claim must include a `peer_id` (libp2p peer ID) and `bootstrap_list_url` (optional: where to fetch the bootstrap list).
4. Trust is rooted in the operator's NIP-05 identifier; the operator decides who to trust by following them on Nostr.

## Acceptance Criteria

- [ ] `BootstrapConfig::bootstrap_mode = Nostr` support
- [ ] `crates/octo-bootstrap/src/mode/nostr.rs` — Nostr adapter
- [ ] NIP-05 resolution
- [ ] Kind 3 contact list fetch
- [ ] Kind 30078 DOT capability verification
- [ ] Unit tests: NIP-05 resolution (success, not found, network error), capability verification
- [ ] Integration test: full Nostr bootstrap flow with 5 contacts
- [ ] Documentation: how to publish a DOT capability claim (NIP-78 or similar)

## Mitigates

D-NB-9 (cold-start bootstrapping without IP-based seed list) — Nostr provides a trust-anchored bootstrap channel that doesn't depend on a centralized IP seed list.

## Deadline

Future
