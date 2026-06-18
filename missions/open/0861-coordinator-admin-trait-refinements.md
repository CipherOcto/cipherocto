# Mission: Implement RFC-0861 (CoordinatorAdmin Adapter Contract Refinements)

## Status

Open

## RFC

RFC-0861 (Networking): CoordinatorAdmin Adapter Contract Refinements
(`rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md`)

## Summary

Implement the spec from RFC-0861, closing 17 R1 findings deferred from
the R20/R21 `CoordinatorAdmin` implementation:

- **HIGH (3):** H1 (WhatsApp `can_join_by_invite` bit lie),
  H2 (WhatsApp `create_group` trait/inherent footgun),
  H6 (WhatsApp `add_member` partial-success surfaces as full error)
- **MEDIUM (14):** M1 (set_ephemeral u32 truncation),
  M2 (try_new constructors), M3 (IRC health_check use_tls),
  M4 (initial_admins_promoted field), M5 (swallowed errors),
  M7 (IRC add_member CHANOPRIVSNEEDED), M8 (IRC health_check is_authenticated),
  M10 (IRC can_join_by_id flip), M11 (WhatsApp HashSet opt),
  M12 (GroupModeFlags doc), M13 (list_own_groups_with_invites),
  M14 (is_admin doc), M15 (IrcConfig::validate channel rules),
  M16 (WhatsAppConfig JID validation)

Full finding-to-section mapping is in RFC-0861 Appendix A.

## Acceptance Criteria

### Phase 1: Trait surface (no behavior change)

- [ ] `GroupId::try_new`, `PeerId::try_new`, `InviteRef::try_new` exist and reject empty strings (M2)
- [ ] Existing `new` methods have `debug_assert!(!s.is_empty())` (M2)
- [ ] `AddMemberOutput { added: bool, promoted: Result<(), PlatformAdapterError> }` defined and the trait `add_member` returns it (H6)
- [ ] `GroupHandle.initial_admins_promoted: bool` field added with `#[serde(default)]` (M4)
- [ ] `list_own_groups_with_invites(&self) -> Result<Vec<GroupHandle>, _>` method added (M13)
- [ ] Doc-comments updated for `GroupModeFlags::set_ephemeral` (M12) and `GroupHandle::is_admin` (M14)
- [ ] All Phase 1 changes pass `cargo check` and `cargo test -p octo-network --lib` (the trait crate; `--lib` excludes integration tests that are out of scope for Phase 1)

### Phase 2: WhatsApp-side behavior changes

- [ ] `join_by_invite` impl calls `client.groups().join_with_invite_code(...)` and returns a proper `GroupHandle` (H1)
- [ ] `capabilities().can_join_by_invite` remains `true` (matches the new impl) (H1)
- [ ] Inherent `create_group` renamed to `create_group_str`; trait impl calls the renamed inherent; `leave_group` precedent at `adapter.rs:1767-1796` mirrored (H2)
- [ ] `set_ephemeral` returns `ApiError { code: 400, ... }` when `as_secs() > u32::MAX as u64` (M1)
- [ ] `get_group_metadata` and `get_invite_link` errors in `create_group` log at `tracing::debug!` and continue (M5)
- [ ] `list_own_groups` builds a `HashSet<String>` of bot's phone forms once before the iter (M11)
- [ ] `WhatsAppConfig::validate()` rejects `groups` entries with `@` that don't end with `@g.us`, and entries with `:` (M16)
- [ ] `group_to_jid` refuses non-numeric inputs without `@g.us` suffix (M16)
- [ ] All Phase 2 changes pass `cargo check` and `cargo test -p octo-adapter-whatsapp --lib` (existing 63 tests still pass; new tests for each finding)

### Phase 3: IRC-side behavior changes

- [ ] `IrcConfig::validate()` rejects channel names that don't start with `#`, `&`, `+`, or `!`, or that contain CR/LF/NUL/space/comma/colon (M15)
- [ ] `IrcAdapter` has `is_authenticated: AtomicBool` field, set on the first RPL_ENDOFMOTD (376) or ERR_NOMOTD (422), cleared on disconnect (M8) — using 376/422 (not 001/RPL_WELCOME) because the listener has no 001 parsing; 376/422 is the canonical "post-handshake" signal
- [ ] `health_check` returns `ApiError { code: 503, ... }` when `is_authenticated` is false (M8)
- [ ] `add_member` (INVITE) returns `ApiError { code: 403, ... }` when listener sees `ERR_CHANOPRIVSNEEDED` for the command (M7)
- [ ] `capabilities().can_join_by_id` flipped to `true` and `join_by_id` method added that wraps `join_by_invite` (M10)
- [ ] `health_check` attempts TLS handshake (via `tokio_rustls::TlsConnector::connect`) when `use_tls = true`; returns `ApiError { code: 525, ... }` on TLS failure (M3) — **not blocked**: R23d C1 is already fixed (commit `4b0f5e0`; `connect_tls` at `lib.rs:713-723` uses real `tokio-rustls`)
- [ ] `docs/research/coordinator-admin-actions.md` updated to reflect IRC's join-by-id support (M10)
- [ ] All Phase 3 changes pass `cargo check` and `cargo test -p octo-adapter-irc --lib` (existing 50 tests still pass; new tests for each finding)

## Location

- `crates/octo-network/src/dot/adapters/coordinator_admin.rs` — trait surface (Phases 1, 6)
- `crates/octo-adapter-whatsapp/src/adapter.rs` — WhatsApp impl (Phase 2)
- `crates/octo-adapter-whatsapp/src/config.rs` (if separate) — `WhatsAppConfig::validate` (M16)
- `crates/octo-adapter-irc/src/lib.rs` — IRC impl (Phase 3) and listener (M7)
- `docs/research/coordinator-admin-actions.md` — M10 doc update

## Complexity

High — 17 findings across 2 adapter crates and 1 trait crate,
including a public API change (`add_member` return type) and
listener-side work (M7 `ERR_CHANOPRIVSNEEDED` parsing, M8 376/422
trigger). Estimated 3 PRs aligned with the 3 phases (Phase 4
deleted; M3 folded into Phase 3 in R24a).

## Prerequisites

- RFC-0861 must be Accepted (currently Draft) before this mission can
  be claimed per BLUEPRINT.md mission rules
- M3 (TLS health check) is unblocked: R23d C1 is already fixed (commit
  `4b0f5e0`); `connect_tls` at `crates/octo-adapter-irc/src/lib.rs:713-723`
  uses real `tokio-rustls`. M3 acceptance is in Phase 3.

## Implementation Notes

- Use the `leave_group_str` rename pattern (RFC-0850p-c precedent: inherent
  method at `octo-adapter-whatsapp/src/adapter.rs:1769`, comment block at
  lines 1763-1764, trait impl at lines 1467-1479) for H2.
- For M7's `ERR_CHANOPRIVSNEEDED` parsing: extend the IRC listener's
  existing numeric-parsing path. The `add_member` trait impl is at
  `crates/octo-adapter-irc/src/lib.rs:1261-1273`. Use the `out_tx` channel
  already in place (R23d H4 fix) to correlate the command with the response
  numeric. Consider adding a small `pending_replies: HashMap<u64,
  oneshot::Sender<NumericResult>>` keyed by a per-command nonce.
- For M8's "authenticated" signal: set `*self.is_authenticated.store(true)`
  inside the existing 376/422 branch in `irc_session` at
  `crates/octo-adapter-irc/src/lib.rs:838-849`. Do NOT add new 001/RPL_WELCOME
  parsing — the listener has none, and 376/422 is the canonical
  post-handshake signal. (R24a N22 fix.)
- For H1's `JoinGroupResult` mapping: see RFC-0861 §3 H1. Map both
  `Joined(Jid)` and `PendingApproval(Jid)` to
  `Ok(GroupHandle { is_admin: false, subject: None, ... })`; callers
  that need to distinguish can call `get_group_metadata` after a backoff.
- For M13's invite-URL parallelization: add `futures = "0.3"` to
  `crates/octo-adapter-whatsapp/Cargo.toml` (not currently a dep) and use
  `futures::future::join_all`, OR use `tokio::task::JoinSet` (already
  available). (R24a N26 fix.)
- For M3's TLS health check: the `connect_tls` function at
  `crates/octo-adapter-irc/src/lib.rs:713-723` already uses
  `tokio_rustls::TlsConnector::connect`. Reuse the same
  `tls_client_config()` helper. (R24a N23 fix: M3 is unblocked, not Phase 4.)

## Reference

- RFC-0861 (Networking): CoordinatorAdmin Adapter Contract Refinements
- `docs/reviews/coordinator-admin-impl-adversarial-review-r1.md` —
  source of all 17 findings
- `docs/reviews/coordinator-admin-impl-adversarial-review-r5.md` —
  closure summary; this mission is the actionable follow-up
- RFC-0850p-c precedent: the `leave_group_str` rename at
  `crates/octo-adapter-whatsapp/src/adapter.rs:1767-1796`

## Mission Status Log

- 2026-06-18: Mission created. RFC-0861 in Draft. Awaiting RFC accept
  before claim.
- 2026-06-18 (R24a): Round 1 review found 9 issues (1 CRITICAL, 1 HIGH, 3 MEDIUM, 4 LOW); all fixed in this revision. See `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r1.md`.
