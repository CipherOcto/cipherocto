# R15 R6 Adversarial Review

## Scope
Re-review of `crates/octo-network/src/{mon,dc,gossip,common}/` after R5
fixes. Focus on: URL-injection vectors, voter-exclusivity in
governance, gossip-topic invariants, and integer-overflow in
2/3 quorum math.

## Findings & Fixes

### R6-1 (HIGH) — NIP-05 identifier allows URL-special characters
**Issue**: The original `Nip05Identifier::parse` (R4 fix) rejected
path separators and whitespace, but allowed URL-special characters
(`?`, `#`, `&`, `=`, `%`, `+`, `,`, `:`, etc.). Since
`resolution_url` does `format!("https://{}/.well-known/nostr.json?name={}", domain, user)`,
a user like `foo&bar@example.com` would produce a URL with a
second injected query parameter.
**Fix**: Strict whitelist: user must be `[a-zA-Z0-9._-]`, domain
must be `[a-zA-Z0-9.-]` and ≤ 253 chars (DNS max). 2 new tests
(`nip05_identifier_rejects_url_special_chars`,
`nip05_identifier_rejects_oversize_domain`).

### R6-2 (LOW) — Voter can count in both `votes_for` and `votes_against`
**Issue**: `GovernanceProposal::cast_vote` inserted a voter into
the appropriate map without removing them from the other. A voter
who cast both a `for` and an `against` vote would be counted in
both totals, inflating both sides.
**Fix**: Remove the voter from both maps before inserting. New test
`test_proposal_vote_change_replaces_prior_vote`.

### R6-3 (LOW) — `consensus_topic` accepts empty `domain_id`
**Issue**: `format!("/dot/dc-consensus/{domain_id}")` with empty
input produces the malformed topic `"/dot/dc-consensus/"`.
**Fix**: assert + test `consensus_topic_rejects_empty`.

### R6-4 (LOW) — `dc_slash_topic` accepts empty `dc_pubkey_hex`
**Issue**: Same as R6-3 but for `dc_slash_topic`. Produces
`"/dot/slash/dc/"`.
**Fix**: assert + test `topic_rejects_empty`.

### R6-5 (LOW) — `process_dc_slash` 2/3 quorum math can overflow
**Issue**: `(total_witnesses * 2).div_ceil(3)` can overflow with
adversarial `total_witnesses`.
**Fix**: Use `saturating_mul` for the multiplier.

## Test Results
- octo-network: 1081 passed (up from 1076 at R5; +5 in R6)

## Files Changed
- `crates/octo-network/src/mon/nostr_bootstrap.rs`
- `crates/octo-network/src/mon/governance.rs`
- `crates/octo-network/src/dc/consensus.rs`
- `crates/octo-network/src/dc/slash.rs`
