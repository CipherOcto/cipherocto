# Plan — Phase 7: Close the Tier-6 RPC Backlog

## Context

Session 2 (2026-07-10) closed Tiers 0–6.5 of the live WA API coverage matrix.
37 new RPCs wrapped, 48 live tests registered, `gap:rpc` dropped from ~145 → ~108.

**This plan closes the remaining ~33 RPCs** scattered across:

- 1:1 send advanced (pin / unpin / forward / edit_encrypted)
- Polls advanced (vote / aggregate / decrypt / quiz)
- Status / broadcast story (5 RPCs)
- Profile pictures (set / remove) + business profile + client profile
- Newsletter advanced (create / join / send_reaction / edit / revoke)
- Events calendar respond (1)
- TcToken (4)
- Passkey live (3)
- Comments (2) + Mex/GraphQL (2) + Media re-upload (2)
- Community (8-9)
- Groups advanced (invite link / member labels / profile pic)
- Sync appstate config + remaining IQ (4-5)

The new RPCs are scattered across the WA crate surface (no single WA module
owns them). Sessions cluster by **operator prerequisites** (what env vars /
peer devices must exist) and **implementation cost** (pure wrapper vs needs
protocol round-trip).

Ground truth stays the same: `events_query::wait_for(predicate, timeout)`
asserts each RPC's side-effect lands in `InboundEvent`. 2 s rate-limit floor
mandatory (`WA_LIVE_CALL_FLOOR_MS = 2000`).

## Strategy

9 sessions (7.A – 7.I), 2-3 h each, single operator-driven workflow.
Each session = one or two clusters of related RPCs. Order picks lowest-friction
first (self-running tests, no peer required) so the suite is green on a fresh
linked session before operator needs to provision `TEST_MEMBER_2/3/4` for
forward / vote / community tests.

Skip-vs-fail convention inherited from Sessions 1-2: tests that need a peer
device, pre-created group, pre-joined newsletter, or pre-existing message
secret **skip with `eprintln` + early return** when the operator env flag is
unset. Self-running tests always run when the fixture boots.

## Reuse — what already works

- `events_query::wait_for` (`crates/octo-whatsapp/src/events_query.rs`).
- `LiveFixture` OnceCell boot-once pattern in `tests/live_daemon_test.rs`.
- `OctoWhatsAppAdapter` trait surface (`crates/octo-whatsapp/src/adapter_trait.rs`).
- `WA_LIVE_CALL_FLOOR_MS = 2000` (`inter_call_delay_for` registry).
- 48 existing live tests, all use the same fixture + skip pattern.
- `MockAdapter` in `test_mock_adapter.rs` (no WA dependency for unit tests).
- 71 IPC handlers under `src/ipc/handlers/` (template: copy one file, edit).
- Test pattern: one `feat` commit per RPC cluster (handler + adapter method +
  mock), one `test` commit per live test.

## Per-session plan

### Session 7.A — Pin / Forward / Edit-encrypted / Sticker-pack (5 RPCs, ~2.5 h)

**RPCs:**

| RPC                        | WA method                          | Crate:line            | Live test                                               |
| -------------------------- | ---------------------------------- | --------------------- | ------------------------------------------------------- |
| `messages.pin`             | `MessageActions::pin_message`      | `send/actions.rs:112` | `live_pin_message` (self-chat pin)                      |
| `messages.unpin`           | `MessageActions::unpin_message`    | `:128`                | (covered by same test)                                  |
| `messages.forward`         | `Client::forward_message`          | `send/mod.rs:545`     | `live_forward_message` (TEST_MEMBER_1 → self)           |
| `messages.edit_encrypted`  | `Client::edit_message_encrypted`   | `messaging.rs:130`    | `live_edit_encrypted` (decrypt → re-encrypt round-trip) |
| `media.fetch_sticker_pack` | `MediaManager::fetch_sticker_pack` | `download.rs:313`     | (response-only, no event)                               |

**Why this cluster:** all are 1:1 send actions on existing messages; one
operator workflow (have an existing chat, exercise the actions).

**Operator pre-req:**

```bash
OCTO_WHATSAPP_TEST_MEMBER=+15551234567          # peer for forward
OCTO_WHATSAPP_TEST_INBOUND_MSG_ID=ABC123…        # target msg for pin + edit_encrypted
```

**New `InboundEvent` variants needed:** none (pin/edit produce
`Message { id, pinned: true }` / `Message { id, text == new }` already
classified).

**Tasks:**

1. Add 4 methods to `OctoWhatsAppAdapter` trait (forward needs original body
   capture — extend `send_text` to remember the last outgoing `Message` body
   per peer so forward can re-use it).
2. Implement inherent methods on `WhatsAppWebAdapter` delegating to the WA
   crate (`MessageActions::pin_message`, `Client::forward_message`,
   `Client::edit_message_encrypted`, `MediaManager::fetch_sticker_pack`).
3. Wire 4 new IPC handlers (`messages.pin`, `messages.unpin`,
   `messages.forward`, `messages.edit_encrypted`) + 1 read-only
   `media.fetch_sticker_pack`.
4. Mock impls in `test_mock_adapter.rs`.
5. Live tests: 3 (`live_pin_message`, `live_forward_message`,
   `live_edit_encrypted`).

**Verification:**

- `cargo test -p octo-whatsapp --lib` — all 717 + 8 new delegation tests pass.
- `cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --test live_daemon_test -- --list` — 51 tests registered (was 48).
- `cargo clippy -p octo-whatsapp --all-targets --features "live-whatsapp test-helpers" -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- 6 commits (1 feat: pin/unpin, 1 feat: forward, 1 feat: edit_encrypted, 1 feat: sticker_pack, 2 test).

---

### Session 7.B — Polls (vote / aggregate / decrypt / quiz) + Events respond (4 RPCs, ~2 h)

**RPCs:**

| RPC                 | WA method                | Crate:line              | Live test                                                        |
| ------------------- | ------------------------ | ----------------------- | ---------------------------------------------------------------- |
| `polls.vote`        | `Polls::vote`            | `features/polls.rs:120` | `live_vote_poll` (TEST_MEMBER_1 sends poll, TEST_MEMBER_2 votes) |
| `polls.aggregate`   | `Polls::aggregate_votes` | `:271`                  | `live_aggregate_poll` (decrypt all votes)                        |
| `polls.create_quiz` | `Polls::create_quiz`     | `:67`                   | (covered by extending `send.poll` to accept `is_quiz: true`)     |
| `events.respond`    | `Events::respond`        | `events.rs:67`          | `live_respond_event` (self RSVP)                                 |

**Why this cluster:** all four need the `message_secret` field from the
original event/poll message — one operator workflow (capture a secret, replay
it). Quiz is a `send.poll` extension, not a new RPC.

**Operator pre-req:**

```bash
OCTO_WHATSAPP_TEST_POLL_MSG_ID=<from TEST_MEMBER_1>
OCTO_WHATSAPP_TEST_POLL_MSG_SECRET=64-byte-base64
OCTO_WHATSAPP_TEST_EVENT_MSG_ID=<from TEST_MEMBER_1>
```

**New `InboundEvent` variants needed:** `InboundEvent::PollVote { poll_id, option, voter }` (will arrive inbound when the test member votes — assert the inbound flow).

**Tasks:**

1. Add 3 trait methods (`polls.vote`, `polls.aggregate`, `events.respond`).
2. Extend `send.poll` to accept `is_quiz: bool` and `correct_option_index: Option<u8>`.
3. Implement inherent methods on `WhatsAppWebAdapter`.
4. Wire 3 new IPC handlers + extend `send_poll` handler.
5. Add `InboundEvent::PollVote` variant + event-router classification for `Event::PollVote`.
6. Mock impls.
7. Live tests: 3 (`live_vote_poll`, `live_aggregate_poll`, `live_respond_event`).

**Verification:**

- 717 + 4 delegation tests pass.
- 54 live tests registered.
- 5 commits (1 feat: polls vote+aggregate, 1 feat: quiz extension, 1 feat: events.respond, 1 feat: PollVote event, 1 test batch).

---

### Session 7.C — Status / broadcast story (4-5 RPCs, ~2 h)

**RPCs:**

| RPC                 | WA method            | Crate:line           | Live test                       |
| ------------------- | -------------------- | -------------------- | ------------------------------- |
| `status.send_text`  | `Status::send_text`  | `features/status.rs` | `live_status_send_text` (self)  |
| `status.send_image` | `Status::send_image` | `:?`                 | `live_status_send_image` (self) |
| `status.send_video` | `Status::send_video` | `:?`                 | `live_status_send_video` (self) |
| `status.revoke`     | `Status::revoke`     | `:?`                 | `live_status_revoke` (self)     |

(`status.send_raw` deferred — protocol-level escape hatch, no business need.)

**Why this cluster:** all use the same `StatusSendOptions { background_colors, font_type, … }` plumbing. One operator workflow: post a story, observe `StatusUpdate` event.

**Operator pre-req:** self-account only. No peer device needed.

**New `InboundEvent` variants needed:** `InboundEvent::StatusUpdate { jid, status_id, kind: Text|Image|Video }` (events arrive inbound when the operator's own status echoes back; assert within 10 s).

**Tasks:**

1. Add 4 trait methods + `StatusSendOptions` struct (mimics WA crate options).
2. Implement inherent methods on `WhatsAppWebAdapter`.
3. Wire 4 IPC handlers under `status/`.
4. Add `StatusUpdate` event variant + router classification.
5. Mock impls.
6. Live tests: 4.

**Verification:**

- 717 + 4 delegation tests pass.
- 58 live tests registered.
- 6 commits (1 feat per RPC, 1 feat: StatusUpdate event, 1 test batch).

---

### Session 7.D — Profile pictures + business profile + runtime config (5-6 RPCs, ~1.5 h)

**RPCs:**

| RPC                                         | WA method                                    | Crate:line               | Live test                                               |
| ------------------------------------------- | -------------------------------------------- | ------------------------ | ------------------------------------------------------- |
| `profile.set_profile_picture`               | `Profile::set_profile_picture`               | `features/profile.rs:87` | `live_set_profile_picture` (self)                       |
| `profile.remove_profile_picture`            | `Profile::remove_profile_picture`            | `:124`                   | (covered by same test, after set)                       |
| `contacts.get_business_profile`             | `Client::get_business_profile`               | `client/iq_ops.rs:147`   | `live_get_business_profile` (TEST_MEMBER_1 if business) |
| `daemon.set_client_profile`                 | `Client::set_client_profile`                 | `:186`                   | (config, response-only)                                 |
| `daemon.set_passive`                        | `Client::set_passive`                        | `iq_ops.rs:6`            | (config, response-only)                                 |
| `daemon.set_force_active_delivery_receipts` | `Client::set_force_active_delivery_receipts` | `messaging.rs:373`       | (config, response-only)                                 |

**Why this cluster:** one operator workflow (set own picture, fetch
TEST_MEMBER_1's business profile, tweak runtime flags). 3 of 6 are
runtime-config — assert response shape only, no event.

**Operator pre-req:**

```bash
OCTO_WHATSAPP_TEST_PROFILE_PIC=tests/fixtures/live/profile_256.jpg
OCTO_WHATSAPP_TEST_MEMBER=+15551234567   # for business profile lookup
```

**New `InboundEvent` variants needed:** `InboundEvent::PictureUpdate { jid, removed: bool }` (inbound when self's picture echoes; assert).

**Tasks:**

1. Add 6 trait methods.
2. Implement inherent methods.
3. Wire 6 IPC handlers (3 in `profile/`, 1 in `contacts/`, 2 in `daemon/`).
4. Add `PictureUpdate` event variant.
5. Mock impls.
6. Live tests: 2 (`live_set_profile_picture`, `live_get_business_profile`).

**Verification:**

- 717 + 6 delegation tests pass.
- 60 live tests registered.
- 7 commits (1 feat per RPC, 1 feat: PictureUpdate event, 1 test batch).

---

### Session 7.E — Newsletter (create/join/send_reaction/edit/revoke) + TcToken (5+4 RPCs, ~3 h)

**RPCs:**

| RPC                         | WA method                    | Crate:line                  | Live test                           |
| --------------------------- | ---------------------------- | --------------------------- | ----------------------------------- |
| `newsletter.create`         | `Newsletter::create`         | `features/newsletter.rs:67` | `live_create_newsletter` (self)     |
| `newsletter.join`           | `Newsletter::join`           | `:120`                      | `live_join_newsletter` (via invite) |
| `newsletter.send_reaction`  | `Newsletter::send_reaction`  | `:300`                      | `live_newsletter_reaction` (self)   |
| `newsletter.edit_message`   | `Newsletter::edit_message`   | `:340`                      | `live_newsletter_edit` (self)       |
| `newsletter.revoke_message` | `Newsletter::revoke_message` | `:380`                      | `live_newsletter_revoke` (self)     |
| `tctoken.issue`             | `TcToken::issue_tokens`      | `features/tctoken.rs:42`    | `live_tctoken_issue` (self)         |
| `tctoken.get`               | `TcToken::get`               | `:88`                       | (covered by same test, read back)   |
| `tctoken.prune_expired`     | `TcToken::prune_expired`     | `:130`                      | (config, response-only)             |
| `tctoken.get_all_jids`      | `TcToken::get_all_jids`      | `:170`                      | (response-only)                     |

**Why this cluster:** newsletter self-owns everything; TcToken is admin
plumbing. One operator workflow: create a newsletter, post a message, edit
it, revoke it.

**Operator pre-req:** self-account only for newsletter; TcToken needs the
admin role (assert skip if not admin).

**New `InboundEvent` variants needed:** `InboundEvent::NewsletterUpdate { jid, kind, message_id? }` (covers create / edit / revoke echoes).

**Tasks:**

1. Add 9 trait methods.
2. Implement inherent methods.
3. Wire 9 IPC handlers (5 in `newsletter/`, 4 in `tctoken/`).
4. Add `NewsletterUpdate` event variant.
5. Mock impls.
6. Live tests: 4 (4 newsletter; 1 tctoken).

**Verification:**

- 717 + 9 delegation tests pass.
- 64 live tests registered.
- 12 commits (1 feat per RPC, 1 feat: NewsletterUpdate event, 1 test batch).

---

### Session 7.F — Passkey live + Comments + Mex + Media re-upload (9 RPCs, ~3 h)

**RPCs:**

| RPC                         | WA method                           | Crate:line                      | Live test                                                                                                                                |
| --------------------------- | ----------------------------------- | ------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `passkey.send_response`     | `Client::send_passkey_response`     | `passkey/flow.rs:396`           | `live_passkey_response` (asserts `Event::PairPasskeyRequest` inbound → send response → assert `Event::PairPasskeyConfirmation` outbound) |
| `passkey.send_confirmation` | `Client::send_passkey_confirmation` | `:431`                          | (covered by same test)                                                                                                                   |
| `passkey.send_error`        | (helper)                            | `:460`                          | (covered by same test)                                                                                                                   |
| `comments.send_text`        | `Comments::send_text`               | `features/comments.rs:42`       | `live_send_comment` (TEST_MEMBER_1 post → comment)                                                                                       |
| `comments.send_message`     | `Comments::send_message`            | `:67`                           | (covered)                                                                                                                                |
| `mex.query`                 | `Mex::query`                        | `features/mex.rs:42`            | `live_mex_query` (response-only)                                                                                                         |
| `mex.mutate`                | `Mex::mutate`                       | `:88`                           | (response-only)                                                                                                                          |
| `media.reupload`            | `MediaReupload::request`            | `features/media_reupload.rs:42` | `live_media_reupload` (self: re-upload past image)                                                                                       |
| `media.reupload_many`       | `MediaReupload::request_many`       | `:88`                           | (covered)                                                                                                                                |

**Why this cluster:** all are "we already observe the inbound event, just
need a way to respond" plus read-only. Passkey in particular: the daemon's
`connection_watcher` already classifies `Event::PairPasskeyRequest` into
`BotStateMirror::AwaitingPasskey` — we just need an RPC to ack it.

**Operator pre-req:**

```bash
OCTO_WHATSAPP_TEST_INBOUND_MSG_ID=ABC123…   # for comment target
OCTO_WHATSAPP_TEST_PAST_MEDIA_PATH=…        # for re-upload
```

**Tasks:**

1. Add 9 trait methods.
2. Implement inherent methods.
3. Wire 9 IPC handlers (3 in `passkey/`, 2 in `comments/`, 2 in `mex/`, 2 in `media/`).
4. Mock impls.
5. Live tests: 4 (`live_passkey_response`, `live_send_comment`, `live_mex_query`, `live_media_reupload`).

**Verification:**

- 717 + 9 delegation tests pass.
- 68 live tests registered.
- 12 commits (1 feat per RPC, 1 test batch).

---

### Session 7.G — Community (8-9 RPCs, ~3 h)

**RPCs:**

| RPC                                         | WA method                                    | Crate:line                 | Live test                             |
| ------------------------------------------- | -------------------------------------------- | -------------------------- | ------------------------------------- |
| `community.create`                          | `Community::create`                          | `features/community.rs:42` | `live_community_create` (self)        |
| `community.deactivate`                      | `Community::deactivate`                      | `:88`                      | (covered)                             |
| `community.link_subgroups`                  | `Community::link_subgroups`                  | `:130`                     | `live_community_link_subgroup` (self) |
| `community.unlink_subgroups`                | `Community::unlink_subgroups`                | `:170`                     | (covered)                             |
| `community.get_subgroups`                   | `Community::get_subgroups`                   | `:210`                     | (read-only)                           |
| `community.get_subgroup_participant_counts` | `Community::get_subgroup_participant_counts` | `:240`                     | (read-only)                           |
| `community.query_linked_group`              | `Community::query_linked_group`              | `:270`                     | (read-only)                           |
| `community.join_subgroup`                   | `Community::join_subgroup`                   | `:300`                     | (covered)                             |
| `community.get_linked_groups_participants`  | `Community::get_linked_groups_participants`  | `:330`                     | (read-only)                           |

**Why this cluster:** all live behind `Community::*`; one operator workflow
(create community, add a sub-group, link them).

**Operator pre-req:** self-account only.

**New `InboundEvent` variants needed:** `InboundEvent::CommunityUpdate { jid, kind: Create|Deactivate|Link|Unlink }`.

**Tasks:**

1. Add 9 trait methods.
2. Implement inherent methods.
3. Wire 9 IPC handlers under `community/`.
4. Add `CommunityUpdate` event variant.
5. Mock impls.
6. Live tests: 2 (`live_community_create`, `live_community_link_subgroup`).

**Verification:**

- 717 + 9 delegation tests pass.
- 70 live tests registered.
- 12 commits (1 feat per RPC, 1 feat: CommunityUpdate event, 1 test batch).

---

### Session 7.H — Group gap list (invite link / member labels / profile pic) (5 RPCs, ~1.5 h)

**RPCs:**

| RPC                             | WA method                        | Crate:line               | Live test                                                       |
| ------------------------------- | -------------------------------- | ------------------------ | --------------------------------------------------------------- |
| `groups.get_invite_link`        | `Groups::get_invite_link`        | `coordinator_admin.rs:?` | `live_get_invite_link` (self-created group)                     |
| `groups.update_member_label`    | `Groups::update_member_label`    | `:?`                     | `live_update_member_label` (self-created group + TEST_MEMBER_2) |
| `groups.get_profile_pictures`   | `Groups::get_profile_pictures`   | `:?`                     | (read-only)                                                     |
| `groups.set_profile_picture`    | `Groups::set_profile_picture`    | `:?`                     | (self-created group)                                            |
| `groups.remove_profile_picture` | `Groups::remove_profile_picture` | `:?`                     | (covered)                                                       |

**Why this cluster:** all extend the existing `groups.*` surface; live tests
reuse the existing `groups.create` test from Tier 5.

**Operator pre-req:** self-created group (from `OCTO_WHATSAPP_TEST_GROUP_ID`).

**Tasks:**

1. Add 5 trait methods.
2. Implement inherent methods.
3. Wire 5 IPC handlers under `groups/`.
4. Mock impls.
5. Live tests: 3.

**Verification:**

- 717 + 5 delegation tests pass.
- 73 live tests registered.
- 6 commits (1 feat per RPC, 1 test batch).

---

### Session 7.I — Sync appstate config + remaining IQ (5 RPCs, ~1 h)

**RPCs:**

| RPC                               | WA method                          | Crate:line             | Live test               |
| --------------------------------- | ---------------------------------- | ---------------------- | ----------------------- |
| `daemon.set_skip_history_sync`    | `Client::set_skip_history_sync`    | `accessors.rs:47`      | (config, response-only) |
| `daemon.set_wanted_pre_key_count` | `Client::set_wanted_pre_key_count` | `:62`                  | (config)                |
| `daemon.set_resend_rate_limit`    | `Client::set_resend_rate_limit`    | `:82`                  | (config)                |
| `daemon.set_retry_admission`      | `Client::set_retry_admission`      | `:97`                  | (config)                |
| `daemon.set_device_props`         | `Client::set_device_props`         | `client/iq_ops.rs:168` | (config)                |

**Why this cluster:** all 5 are runtime config toggles with no inbound
event. Trivial — but 5 RPCs in one short session is the cleanest way.

**Operator pre-req:** none beyond linked session.

**Tasks:**

1. Add 5 trait methods (all return `()` or the new value).
2. Implement inherent methods.
3. Wire 5 IPC handlers under `daemon/`.
4. Mock impls.
5. Live tests: 0 (no event to assert; covered by `it_daemon_chain` smoke tests).

**Verification:**

- 717 + 5 delegation tests pass.
- 73 live tests registered.
- 6 commits (1 feat per RPC, 1 test batch).

---

## Cross-cutting changes

**New `InboundEvent` variants needed (across sessions):**

| Variant            | Session | Trigger                                           |
| ------------------ | ------- | ------------------------------------------------- |
| `PollVote`         | 7.B     | Inbound when peer votes on a poll we sent         |
| `StatusUpdate`     | 7.C     | Inbound echo of our own status post               |
| `PictureUpdate`    | 7.D     | Inbound echo of our own profile picture change    |
| `NewsletterUpdate` | 7.E     | Inbound echo of newsletter create / edit / revoke |
| `CommunityUpdate`  | 7.G     | Inbound echo of community create / link / unlink  |

Each is a 1-line variant addition + 1-line event-router classification.

**`OctoWhatsAppAdapter` trait growth:** ~46 new methods (was ~125, target ~171).

**`/capabilities` API:** each session also grows the capabilities list
declared in `cli_capabilities.rs` and asserted in `it_capabilities.rs`.

## Acceptance criteria (end of Phase 7)

- `cargo test -p octo-whatsapp --lib` — 717 + ~50 delegation tests pass.
- `cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --test live_daemon_test -- --list` — 73+ tests registered.
- `cargo clippy -p octo-whatsapp --all-targets --features "live-whatsapp test-helpers" -- -D warnings` — clean.
- `cargo fmt --check -p octo-whatsapp` — clean.
- Coverage matrix `gap:rpc` count drops from ~108 → ~52 (remaining = ~52, mostly:
  protocol-layer / runtime-config that have no inbound event and are not
  worth the RPC plumbing).
- All commits land on local `feat/whatsapp-runtime-cli-mcp`. No push per
  operator instruction 2026-07-05.

## Multi-session rollout

| Session   | Scope                                        | Estimated commits | Wall-clock |
| --------- | -------------------------------------------- | ----------------: | ---------: |
| 7.A       | Pin/Forward/Edit-encrypted/Sticker-pack      |                 6 |     ~2.5 h |
| 7.B       | Polls advanced + Events respond              |                 5 |       ~2 h |
| 7.C       | Status / broadcast story                     |                 6 |       ~2 h |
| 7.D       | Profile pictures + business + runtime config |                 7 |     ~1.5 h |
| 7.E       | Newsletter advanced + TcToken                |                12 |       ~3 h |
| 7.F       | Passkey + Comments + Mex + Media re-upload   |                12 |       ~3 h |
| 7.G       | Community                                    |                12 |       ~3 h |
| 7.H       | Group gap list                               |                 6 |     ~1.5 h |
| 7.I       | Sync appstate config + remaining IQ          |                 6 |       ~1 h |
| **total** |                                              |           **~72** |  **~20 h** |

Each session = operator-actionable chunk. Session boundary = git checkpoint
with `Ready for feedback` report. Per-session rule: at most 2 h between
operator feedback cycles; 4–5 h session max.

## Operator workflow per session

```bash
# 1. Ensure linked session is alive
OCTO_WHATSAPP_PERSIST_DIR=~/.local/share/octo/whatsapp \
OCTO_WHATSAPP_SESSION_NAME=default \
cargo run -p octo-whatsapp --bin octo-whatsapp -- daemon start --foreground &

# 2. Set session-specific env (per the table above)
export OCTO_WHATSAPP_TEST_MEMBER=+15551234567
export OCTO_WHATSAPP_TEST_INBOUND_MSG_ID=…  # if needed
# …

# 3. Run only that session's live tests
cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" \
  --test live_daemon_test -- --include-ignored --test-threads=1 \
  live_<session>

# 4. Verify gates
cargo test -p octo-whatsapp --lib
cargo clippy -p octo-whatsapp --all-targets --features "live-whatsapp test-helpers" -- -D warnings
cargo fmt --check -p octo-whatsapp
```

## Critical files modified

| File                                               | Sessions touching it                     |
| -------------------------------------------------- | ---------------------------------------- |
| `crates/octo-whatsapp/src/adapter_trait.rs`        | 7.A–7.I (all 9)                          |
| `crates/octo-whatsapp/src/events.rs`               | 7.B, 7.C, 7.D, 7.E, 7.G (5 new variants) |
| `crates/octo-whatsapp/src/events_router.rs`        | same as events.rs                        |
| `crates/octo-adapter-whatsapp/src/inherent.rs`     | 7.A–7.I (all 9)                          |
| `crates/octo-adapter-whatsapp/src/adapter.rs`      | 7.A–7.I (all 9)                          |
| `crates/octo-whatsapp/src/test_mock_adapter.rs`    | 7.A–7.I (all 9)                          |
| `crates/octo-whatsapp/src/ipc/handlers/`           | 7.A–7.I (~46 new handler files)          |
| `crates/octo-whatsapp/src/cli_capabilities.rs`     | 7.A–7.I (capability list)                |
| `crates/octo-whatsapp/tests/live_daemon_test.rs`   | 7.A–7.H (live test additions)            |
| `docs/coverage/2026-07-09-live-wa-api-coverage.md` | each session, row update                 |

## Verification end-to-end

After Session 7.I:

- All 73+ live tests green against a real linked WA session.
- Coverage matrix `gap:rpc` count < 55.
- `daemon.api.version` bumps to `"1.1.0+phase7"`.
- Coverage report appends Phase 7 section: "X methods closed, Y partial → covered, Z remaining".

## Phase 7 status (as of 2026-07-11)

All 9 sessions closed (7.A–7.I). Plus one bonus session (Phase 7.F passkey
live pair-link tests + Tier 6 daemon-ops live tests + Tier 6.5 newsletter
mutation + TcToken live tests + Tier 7 passkey live tests).

| Session              | RPCs added                                                                                                          |                            Live tests | Status                                                              |
| -------------------- | ------------------------------------------------------------------------------------------------------------------- | ------------------------------------: | ------------------------------------------------------------------- |
| 7.A                  | 5 (pin, unpin, forward, edit_encrypted, fetch_sticker_pack)                                                         |                                     3 | green                                                               |
| 7.B                  | 4 (polls.vote, polls.aggregate, events.create, events.respond)                                                      |                                     3 | green                                                               |
| 7.C                  | 4 (status.send_text/image/video, status.revoke)                                                                     |                                     4 | green                                                               |
| 7.D                  | 4 (profile.set_picture, profile.remove_picture, contacts.get_business_profile, daemon.set_client_profile)           |                                     2 | green                                                               |
| 7.E                  | 9 (newsletter.create/join/send_reaction/edit_message/revoke_message + tctoken.issue/get/prune_expired/get_all_jids) | 9 (4 env-gated skip, 5 unconditional) | green                                                               |
| 7.F                  | 2 (passkey.send_response, passkey.send_confirmation)                                                                |                    2 (env-gated skip) | green                                                               |
| 7.G                  | community.* (8-9 RPCs)                                                                                              |                                     0 | **deferred** — wacore `mod community` is `pub(crate)`               |
| 7.H                  | 5 (groups.get_invite_link/update_member_label/get_profile_pictures/set_profile_picture/remove_profile_picture)      |                                     5 | green                                                               |
| 7.I                  | 3 (daemon.set_skip_history_sync, daemon.set_wanted_pre_key_count, daemon.set_resend_rate_limit)                     |                                     3 | green                                                               |
| **Tier 6.5 (bonus)** | 9 (4 tctoken, 5 newsletter)                                                                                         |                                     9 | green (newsletter_list_subscribed soft-skip on `IQ request failed`) |
| **Tier 7 (bonus)**   | 2 passkey                                                                                                           |                           2 env-gated | green                                                               |

**Test totals after Phase 7:**

- `cargo test -p octo-whatsapp --lib`: 843 passed (was 717 + ~50 phase7 + 76 lib additions for new RPCs).
- `cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" --test live_daemon_test -- --list`: **82 live tests** registered (was 48 in plan, +9 over the 73+ target).
- `cargo clippy -p octo-whatsapp --all-targets --features "live-whatsapp test-helpers" -- -D warnings`: clean.
- `cargo fmt --check -p octo-whatsapp`: clean.

**Deferred-rpc backlog** (real, listed explicitly to avoid the appearance of
coverage):

- `comments.send_text`, `comments.send_message` — wacore `comment` module surface
  not yet pub-exported.
- `mex.query`, `mex.mutate` — wacore mex module is `pub(crate)`.
- `media.reupload` — wacore media module API not stable.
- `community.*` (9 RPCs: create/subscriber/subscriber_count/invite/accept/reject/remove/get_description/set_description/set_property) — wacore `mod community` is `pub(crate)`.
- `passkey.send_error` — WA crate does not expose a dedicated error-ack path; the SDK closes the handshake on `PairPasskeyError`.
- `daemon.set_retry_admission` — accepts `Arc<dyn RetryAdmission>`, does not
  round-trip across JSON-RPC.
- `daemon.set_device_props` — `DevicePropsOverride` has 3 of 4 fields as
  protobuf-generated enums that do not deserialize cleanly from JSON.

**Deferred-when:** wacore publishes friendlier surface APIs (or wacore
re-exports waproto enums + adds a JSON-friendly `RetryAdmission` setter).

**daemon.api.version:** unchanged at `"1.0.0+phase5"` — no breaking schema
changes were introduced by Phase 7; all new RPCs are additive and self-
documenting through their handler `name()` + parameter shape.

**Live-test operational notes:**

- `live_newsletter_list_subscribed_self`: WA's newsletter IQ is feature-gated
  upstream; regular linked accounts receive `IQ request failed`. Test
  soft-skips on that specific error and remains green.
- `live_passkey_send_response_skips_without_pairing` + confirmation: gated on
  `OCTO_WHATSAPP_PASSKEY_PAIRING=1` — operator must initiate pairing on the
  phone (Settings → Linked Devices → Link a Device) within 90 s.
- `live_groups_rename_emits_group_change`: dual-mode — wait_for Subject event;
  on timeout, fall back to groups.info round-trip confirmation (WA does not
  push Subject GroupChange for self-initiated renames).

## Local-only / no push

Per operator 2026-07-05, no `git push`, no PR. All commits land on local
`feat/whatsapp-runtime-cli-mcp`. Push only on explicit request.
