# Mission: 0850h-e Matrix CoordinatorAdmin Live Test Coverage

## Status

Open (2026-06-28)

## RFC

RFC-0861 (CoordinatorAdmin Adapter Contract Refinements, Accepted
2026-06-19) — the trait surface this mission's tests exercise.

## Summary

Complete the live test coverage for the `CoordinatorAdmin` trait on
the matrix adapter. Mission 0850h-d implemented the trait and landed
6 live tests (mx09–mx14) covering 13 of 24 methods. This follow-on
adds the remaining 6 tests (mx15–mx20) to bring coverage to 20 of
24 methods. The 4 methods not covered by live tests are `leave_group`,
`destroy_group`, `list_own_groups_with_invites`, and
`transfer_ownership` — their error paths are covered by unit tests in
0850h-d Phase 1; live tests for these require destroying real rooms
or transferring ownership, which is destructive and not suitable for
a shared test homeserver.

## Acceptance Criteria

- [ ] `tests/live_matrix_test.rs` gains six new tests:
      `mx15_add_member`, `mx16_approve_join_request`,
      `mx17_list_own_groups_with_invites`, `mx18_resolve_invite`,
      `mx19_join_by_invite_and_id`, `mx20_transfer_ownership`
- [ ] Each test uses the same pre-scan guard + room-create +
      `octo-test-mx-mx{nn}-{ts}` naming convention as mx09–mx14
- [ ] `cargo test -p octo-adapter-matrix-sdk --features live-matrix
      --test live_matrix_test -- --ignored --nocapture` — all 19
      tests pass (mx00–mx08 + mx09–mx14 + mx15–mx20) when run with
      `--test-threads=1`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
      passes (zero warnings)
- [ ] `cargo fmt --check` is clean

## Per-test scope

| Test | Trait methods exercised | Section |
|------|------------------------|---------|
| `mx15_add_member` | `add_member` (with `is_admin = true` and `is_admin = false`) | B. Membership |
| `mx16_approve_join_request` | `approve_join_request` (set room to `JoinRule::Knock`, simulate join request, approve) | B. Membership |
| `mx17_list_own_groups_with_invites` | `list_own_groups_with_invites` (create room with canonical alias, verify invite_url populated) | D. Discovery |
| `mx18_resolve_invite` | `resolve_invite` (resolve canonical alias to room_id) | D. Discovery |
| `mx19_join_by_invite_and_id` | `join_by_invite`, `join_by_id` (join a room by alias, verify membership) | D. Discovery |
| `mx20_transfer_ownership` | `transfer_ownership` (multi-step dance: promote new owner, demote self, leave) | E. Handoff |

## Location

- `crates/octo-adapter-matrix-sdk/tests/live_matrix_test.rs` — six
      new tests (mx15–mx20)

## Complexity

Low — the trait impl already exists (0850h-d); this mission only
adds live tests that exercise it against matrix.org.

## Prerequisites

- Mission `0850h-d-matrix-coordinator-admin.md` (Open) — the trait
      impl must be landed first
- `octo-matrix-onboard login oidc --homeserver https://matrix.org` —
      live tests require an OIDC-authenticated session at
      `~/.config/octo/matrix.json`

## Implementation Notes

- Follow the same pre-scan guard + room-create + cleanup pattern
      as mx09–mx14 (mission 0850h-d §Phase 2)
- `mx16_approve_join_request` requires a second test user to send
      a knock request — use the `@ci2:localhost` user from the
      integration test setup (or skip if running against matrix.org
      without a second account)
- `mx20_transfer_ownership` leaves the test bot without admin in
      the room — create a fresh room for each run (the naming
      convention handles this)
- `mx19_join_by_invite_and_id` creates a room with the bot as
      sole member, then joins by alias — the bot must invite itself
      or use `JoinRule::Public` for the join-by-id path
