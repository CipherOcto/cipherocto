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

### Phase 1: Trait surface (additive; no breakage for existing callers)

- [ ] `GroupId::try_new`, `PeerId::try_new`, `InviteRef::try_new` exist and reject empty strings (M2)
- [ ] Existing `new` methods have `debug_assert!(!s.is_empty())` (M2)
- [ ] `AddMemberOutput { added: bool, promoted: Option<Result<(), PlatformAdapterError>> }` defined and the trait `add_member` returns it (H6); unit test in `crates/octo-network/src/dot/adapters/coordinator_admin.rs` test module covers all three `promoted` variants — `None` (no promote attempted), `Some(Ok(()))` (promote succeeded), `Some(Err(_))` (add succeeded, promote failed)
- [ ] `GroupHandle.initial_admins_promoted: bool` field added with `#[serde(default)]` (M4)
- [ ] `list_own_groups_with_invites(&self) -> Result<Vec<GroupHandle>, _>` method added (M13)
- [ ] Doc-comments updated for `GroupModeFlags::set_ephemeral` (M12) and `GroupHandle::is_admin` (M14)
- [ ] All Phase 1 changes pass `cargo check` and `cargo test -p octo-network --lib` (the trait crate; `--lib` excludes integration tests that are out of scope for Phase 1)

### Phase 2: WhatsApp-side behavior changes

- [ ] Inherent `create_group` renamed to `create_group_str`; trait impl calls the renamed inherent; `leave_group_str` precedent at `adapter.rs:1769` (inherent; comment block 1763-1767 — rationale: re-bind the public `String`-returning method to a distinct local name so the trait impl can call it; trait impl 1467-1479) mirrored (H2) — **do this FIRST**, since the M5 `.ok()` → `tracing::debug!` edit lands on the renamed function. M1 (`set_ephemeral`), M11 (`list_own_groups`), M16 (`WhatsAppConfig::validate`), and H1 (`join_by_invite`) are separate methods and don't depend on the H2 rename.
- [ ] `join_by_invite` impl calls `client.groups().join_with_invite_code(...)` and returns a proper `GroupHandle`; `capabilities().can_join_by_invite` remains `true` (matches the new impl) (H1)
- [ ] `set_ephemeral` returns `ApiError { code: 400, ... }` when `as_secs() > u32::MAX as u64` (M1)
- [ ] `get_group_metadata` and `get_invite_link` errors in `create_group_str` (post-H2 rename; was `create_group` pre-H2) log at `tracing::debug!` and continue (M5)
- [ ] `list_own_groups` builds a `HashSet<String>` of bot's phone forms once before the iter (M11)
- [ ] `WhatsAppConfig::validate()` rejects `groups` entries with `@` that don't end with `@g.us`, and entries with `:` (M16)
- [ ] `group_to_jid` refuses non-numeric inputs without `@g.us` suffix (M16)
- [ ] All Phase 2 changes pass `cargo check` and `cargo test -p octo-adapter-whatsapp --lib` (existing 63 tests still pass; new tests for each finding)

### Phase 3: IRC-side behavior changes

- [ ] `IrcConfig::validate()` rejects channel names that don't start with `#`, `&`, `+`, or `!`, or that contain CR/LF/NUL/space/comma/colon (M15)
- [ ] `IrcAdapter` has `is_authenticated: AtomicBool` field, set on the first RPL_ENDOFMOTD (376) or ERR_NOMOTD (422), cleared in BOTH `mark_disconnected` (transient drop, at `lib.rs:377`) AND `shutdown` (full teardown, at `lib.rs:1086`) per RFC §4 M8 (R24g N65) — using 376/422 (not 001/RPL_WELCOME) because the listener has no 001 parsing; 376/422 is the canonical "post-handshake" signal
- [ ] `health_check` returns `ApiError { code: 503, ... }` when `is_authenticated` is false (M8)
- [ ] `add_member` (INVITE) returns `ApiError { code: 403, ... }` when listener sees `ERR_CHANOPRIVSNEEDED` for the command (M7); unit test in `crates/octo-adapter-irc/src/lib.rs` test module covers the new `pending_replies: Mutex<HashMap<CommandId, oneshot::Sender<NumericResult>>>` field — register a fake INVITE nonce, simulate `ERR_CHANOPRIVSNEEDED` arriving in the listener, assert the oneshot resolves with the correct error and that the HashMap entry is removed; plus an independence test verifying that two `IrcAdapter` instances don't share the HashMap
- [ ] `capabilities().can_join_by_id` flipped to `true` and `join_by_id` method added that wraps `join_by_invite` (M10)
- [ ] `health_check` attempts TLS handshake (via `tokio_rustls::TlsConnector::connect`) when `use_tls = true`; returns `ApiError { code: 525, ... }` on TLS failure (M3) — **not blocked**: R23d C1 is already fixed (commit `4b0f5e0`; `connect_tls` at `lib.rs:713-723` uses real `tokio-rustls`)
- [ ] `docs/research/coordinator-admin-actions.md` updated to reflect IRC's join-by-id support (M10)
- [ ] All Phase 3 changes pass `cargo check` and `cargo test -p octo-adapter-irc --lib` (existing 50 tests still pass; new tests for each finding)

## Location

- `crates/octo-network/src/dot/adapters/coordinator_admin.rs` — trait surface (Phase 1)
- `crates/octo-adapter-whatsapp/src/adapter.rs` — WhatsApp impl (Phase 2) AND `WhatsAppConfig::validate` (M16; struct at line 30, impl at line 83, validate at line 97 — there is NO separate `config.rs` file)
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
- For M7's `ERR_CHANOPRIVSNEEDED` parsing: add a NEW `pending_replies:
  Mutex<HashMap<CommandId, oneshot::Sender<NumericResult>>>` on `IrcAdapter`
  (and the matching state inside the `irc_session` listener task). The
  `add_member` trait impl is at `crates/octo-adapter-irc/src/lib.rs:1261-1273`.
  Key the entry by a per-command nonce inserted before sending INVITE; the
  listener resolves the oneshot when the matching numeric arrives. **Do NOT**
  reuse `out_tx` (mpsc::Sender<String> for outbound lines,
  `lib.rs:222`) or `shutdown_tx` (watch::Sender<bool> for shutdown,
  `lib.rs:232`) — neither can carry reply codes. (R24b N32/N33 fix.)
- For M8's "authenticated" signal: set `*self.is_authenticated.store(true, std::sync::atomic::Ordering::SeqCst)`
  inside the existing 376/422 branch in `irc_session` at
  `crates/octo-adapter-irc/src/lib.rs:838-849`. Do NOT add new 001/RPL_WELCOME
  parsing — the listener has none, and 376/422 is the canonical
  post-handshake signal. (R24a N22 fix; R24c N42 added the required
  `Ordering` argument.)
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
  `crates/octo-adapter-whatsapp/src/adapter.rs:1769`
  (inherent method; comment block at lines 1763-1764, trait impl at
  lines 1467-1479)

## Mission Status Log

- 2026-06-18: Mission created. RFC-0861 in Draft. Awaiting RFC accept
  before claim.
- 2026-06-18 (R24a): Round 1 review found 9 issues (1 CRITICAL, 1 HIGH, 3 MEDIUM, 4 LOW); all fixed. See `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r1.md`.
- 2026-06-18 (R24b): Round 2 review found 8 issues (1 HIGH, 2 MEDIUM, 5 LOW); all fixed. See `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r2.md`.
- 2026-06-18 (R24c): Round 3 review found 8 LOW accuracy gaps (off-by-one line refs, missing `Ordering`, misleading config.rs reference, Phase 1 title); all fixed. See `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r3.md`.
- 2026-06-18 (R24d): Round 4 review found 5 issues (2 MEDIUM, 3 LOW). MEDIUMs: Version History 1.2 row was overwritten instead of appended (recovered); AddMemberOutput.promoted doc said 'None if X' but the type was Result, not Option<Result> (now Option<Result>). LOWs: M1 'sibling method' phrasing wrong; mission M5 still said 'in create_group'; Phase 2 plan overclaimed 'all other edits'. All fixed. See `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r4.md`.
- 2026-06-18 (R24e): Round 5 review found 4 issues (1 MEDIUM, 3 LOW). MEDIUM: downstream R5 closure summary still showed M3 as 'Phase 4 (blocked on C1)' — stale; corrected to '3 (unblocked since R23d C1)' with a footnote pointing to RFC-0861 Appendix A as the canonical mapping. LOWs: Version History 1.4 row was claimed in R24d commit message but never written (added); R5 footnote expanded to clarify the columns are R5-time snapshots; Mission Phase 1 H6 criterion extended to require a discriminator test for the three Option<Result<>> variants. See `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r5.md`.
- 2026-06-18 (R24f): Round 6 review found 3 issues (2 MEDIUM, 1 LOW). MEDIUMs: Version History 1.3 row recovered (sequence now 1.0/1.1/1.2/1.3/1.4/1.5/1.6 — complete); Mission Phase 2 plan reordered to put H2 BEFORE M5 (matches the RFC's instruction). LOW: Mission Phase 2 H1 bullets merged into one coupled acceptance criterion (the capability bit and the impl are not independent). See `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r6.md`.
- 2026-06-18 (R24g): Round 7 review found 3 issues (2 MEDIUM, 1 LOW). MEDIUMs: §3 H1 struct literal missing `initial_admins_promoted: false` (would fail to compile after Phase 1 M4 lands, since GroupHandle doesn't derive Default); §4 M8 "clear on disconnect" was ambiguous — clarified to clear in BOTH `mark_disconnected` (transient drop, lib.rs:377) AND `shutdown` (full teardown, lib.rs:1086) — without the former, a transient drop leaves is_authenticated=true until next 376/422. LOW: Mission Phase 3 M7 acceptance criterion extended to require a unit test for the new `pending_replies` HashMap. See `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r7.md`.
- 2026-06-18 (R24h): Round 8 review found 3 MEDIUMs — all downstream propagations of the R24g N65 fix that didn't reach the summary sites. RFC Phase 3 plan line still said 'clear on disconnect' (now 'clear in BOTH mark_disconnected and shutdown'); RFC Key Files row still said 'CLEAR it in disconnect next to the existing shutdown_tx clear' (now specifies both methods with line numbers); Mission Phase 3 M8 acceptance still said 'cleared on disconnect' (now specifies both methods). See `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r8.md`.
- 2026-06-18 (R24i): Round 9 review found 3 LOWs — line-number drift. RFC Key Files row cited `IrcAdapter` struct at '~line 225', actual is line 208 (struct decl at lib.rs:208); RFC §2 H2 cited leave_group_str 'comment block at lines 1763-1764', actual is 1763-1767 (5-line doc comment, the rationale sentence at 1765-1767 is the important part); Mission Phase 2 H2 cited 'comment block 1763-1764' — same fix, same rationale. See `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r9.md`.
- 2026-06-18 (R24j): Round 10 review found 2 issues (1 MEDIUM, 1 LOW). MEDIUM: RFC Phase 2 plan listed H1 BEFORE H2 — contradicted H2's 'do this FIRST' annotation AND the Mission (post-R24f N62 fix has H2 first). Reordered to put H2 first. LOW: RFC Phase 2 plan H1 cite said `per §1 (H1)` but the primary impl spec and `JoinGroupResult` variant mapping are in §3 H1 — changed to `per §1+§3 (H1; primary impl spec in §3 H1)`. See `docs/reviews/coordinator-admin-rfc-0861-adversarial-review-r10.md`.
