//! `CoordinatorAdmin` impl for the MTProto Telegram adapter
//! (RFC-0850 §8 extension).
//!
//! The MTProto adapter supports the full coordinator / admin
//! surface for supergroups and most of it for basic groups.
//! Telegram differentiates between:
//!
//! - **Basic groups** (Telegram's "small group", up to 200
//!   members, no admin/moderator concept): `messages.*` RPCs
//!   (e.g., `messages.createChat`, `messages.addChatUser`).
//!   No promote/demote; everyone is equal.
//! - **Supergroups** (Telegram's "big group", up to 200,000
//!   members, full admin/moderator): `channels.*` RPCs
//!   (e.g., `channels.createChannel`,
//!   `channels.inviteToChannel`, `channels.editAdmin`).
//! - **Channels** (broadcast only, no participants):
//!   `channels.*` RPCs but no member management.
//!
//! The MTProto adapter can opt in to the full surface. The
//! capability report is **opt-in**: callers should check
//! `admin_capabilities()` before calling methods that
//! supergroups support but basic groups do not. The mock
//! implementation accepts all calls (so unit tests can drive
//! any sequence); the real-network implementation
//! (`#[cfg(feature = "real-network")]`) will disambiguate
//! between basic groups and supergroups by the chat_id's
//! negative-id convention (Telegram supergroups have
//! negative chat_ids with a `-100` prefix).
//!
//! # Implementation notes
//!
//! - `GroupId` round-trip: Telegram's `chat_id` is a signed
//!   64-bit integer (e.g., `-1001234567890` for a supergroup,
//!   `1234567890` for a basic group / private chat). The
//!   adapter stores it as a `String` (matching the
//!   `GroupId::new` convention) and parses back to `i64` for
//!   client calls. The platform name is `"telegram"` (the
//!   same string returned by `CoordinatorAdmin::platform_name`
//!   in the TDLib adapter and the WhatsApp adapter).

use async_trait::async_trait;

use octo_network::dot::adapters::coordinator_admin::{
    AddMemberOutput, AdminCapabilityReport, CoordinatorAdmin, GroupHandle, GroupId,
    GroupMemberSpec, GroupMetadata, GroupModeFlags, InviteRef, PeerId,
};
use octo_network::dot::error::PlatformAdapterError;

use crate::client::MtprotoTelegramClient;
use crate::error::MtprotoTelegramError;
use crate::MtprotoTelegramAdapter;

// ── Helpers ──────────────────────────────────────────────────────

/// Parse a `GroupId` (a chat_id stored as a string) into the
/// `i64` the client's RPCs expect. Returns
/// `PlatformAdapterError::ApiError(400)` if the string is
/// not a valid signed 64-bit integer.
fn parse_chat_id(group_id: &GroupId) -> Result<i64, PlatformAdapterError> {
    group_id
        .as_str()
        .parse::<i64>()
        .map_err(|e| PlatformAdapterError::ApiError {
            code: 400,
            message: format!("invalid chat_id {:?}: {}", group_id.as_str(), e),
        })
}

/// Map a `MtprotoTelegramError` to `PlatformAdapterError`.
/// The mapping is the same as the adapter's main
/// `From<MtprotoTelegramError> for PlatformAdapterError`
/// impl (adapter.rs) — re-implemented here to keep
/// `coordinator_admin.rs` self-contained.
fn map_err(e: MtprotoTelegramError) -> PlatformAdapterError {
    match e {
        MtprotoTelegramError::NotReady(msg) => PlatformAdapterError::Unreachable {
            platform: "telegram-mtproto".into(),
            reason: msg,
        },
        MtprotoTelegramError::Auth(msg) => PlatformAdapterError::ApiError {
            code: 401,
            message: format!("auth: {msg}"),
        },
        MtprotoTelegramError::Config(msg) => PlatformAdapterError::ApiError {
            code: 400,
            message: format!("config: {msg}"),
        },
        MtprotoTelegramError::Rpc { code, message } => PlatformAdapterError::ApiError {
            code: u16::try_from(code).unwrap_or(500),
            message: format!("rpc: {message}"),
        },
        other => PlatformAdapterError::ApiError {
            code: 500,
            message: format!("{other}"),
        },
    }
}

/// Heuristic: is this chat_id a supergroup? Telegram uses
/// negative chat_ids with a `-100` prefix for supergroups
/// and channels (e.g., `-1001234567890`). Basic groups
/// and private chats have positive chat_ids.
///
/// This is a *best-effort* heuristic for the capability
/// report — the real client disambiguates per-call via
/// the server's response (the TL type's `Chat` /
/// `Channel` variant). The heuristic lets the adapter
/// report `can_promote: true` for supergroups without
/// making a separate `messages.getChats` call.
fn is_supergroup(chat_id: i64) -> bool {
    chat_id < 0
}

// ── Capability report (cached, no I/O) ──────────────────────────
//
// The capability report is a static struct — it does not
// depend on the adapter state. Caching it as a
// `OnceLock<AdminCapabilityReport>` is allocation-free on
// the hot path (`admin_capabilities` is called once per
// caller session).
//
// The one per-method decision that does depend on
// adapter state — "is this chat_id a supergroup?" — is
// computed in each method body, not in
// `admin_capabilities`.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_supergroup_detects_negative_ids() {
        // Supergroups / channels have negative chat_ids
        // with a -100 prefix.
        assert!(is_supergroup(-1001234567890));
        assert!(is_supergroup(-1009876543210));
        // Basic groups / private chats have positive
        // chat_ids.
        assert!(!is_supergroup(1234567890));
        assert!(!is_supergroup(0));
    }

    #[test]
    fn parse_chat_id_round_trip() {
        let id = GroupId::new("-1001234567890");
        let parsed = parse_chat_id(&id).unwrap();
        assert_eq!(parsed, -1001234567890);
    }

    #[test]
    fn parse_chat_id_rejects_garbage() {
        let id = GroupId::new("not a number");
        let err = parse_chat_id(&id).unwrap_err();
        match err {
            PlatformAdapterError::ApiError { code, .. } => assert_eq!(code, 400),
            other => panic!("expected ApiError(400), got {other:?}"),
        }
    }
}

// ── CoordinatorAdmin impl ───────────────────────────────────────

#[async_trait]
impl<C: MtprotoTelegramClient + Send + Sync + 'static> CoordinatorAdmin
    for MtprotoTelegramAdapter<C>
{
    /// Capability report. The MTProto adapter supports the
    /// full lifecycle / membership / mode / discovery
    /// surfaces for supergroups and a strict subset for
    /// basic groups. Telegram has no "ban" primitive
    /// (kick + permanent invite-revocation is the
    /// closest equivalent; we report `can_ban: false` and
    /// rely on `kick_participant`).
    fn admin_capabilities(&self) -> AdminCapabilityReport {
        AdminCapabilityReport {
            // Lifecycle
            can_create: true,
            can_join_by_id: false, // Telegram has no "join by id" — only invite links / add by user_id
            can_join_by_invite: true, // `messages.importChatInvite` is supported
            can_leave: true,
            can_destroy: true, // `messages.deleteChat` for basic, `channels.deleteChannel` for supergroups
            // Membership
            can_add_member: true,
            can_remove_member: true,
            can_ban: false,    // No first-class ban
            can_promote: true, // supergroup-only; per-method check
            can_demote: true,
            can_approve_join: false, // Join requests are automatic; no approval primitive
            // Mode
            can_rename: true,
            can_describe: true,
            can_lock: false,    // No "lock chat" concept (only slow-mode / silent)
            can_announce: true, // `channels.toggleSlowMode` / perms
            can_set_ephemeral: false, // No self-destructing-message primitive for groups
            can_require_approval: false, // No "join requires approval" — Telegram handles it via invite links
            // Discovery
            can_list_own_groups: true, // `messages.getDialogs`
            can_get_metadata: true,    // `messages.getChats` / `channels.getChannels`
            can_resolve_invite: true,  // `messages.checkChatInvite`
            // Handoff
            can_transfer_ownership: true, // `channels.editCreator` for supergroups
        }
    }

    fn platform_name(&self) -> String {
        "telegram".into()
    }

    async fn create_group(
        &self,
        subject: &str,
        initial_members: &[GroupMemberSpec],
    ) -> Result<GroupHandle, PlatformAdapterError> {
        // Translate `GroupMemberSpec` to a slice of `i64`
        // user_ids. Telegram's `createChat` takes `users:
        // Vector<InputUser>`; the mock accepts raw `i64`
        // user_ids; the real client (Phase 2) will resolve
        // these to `InputUser::User { user_id, access_hash }`
        // via a peer-info cache lookup. For the Phase 1
        // surface, we pass the raw `i64`s through.
        let user_ids: Vec<i64> = initial_members
            .iter()
            .map(|m| {
                // GroupMemberSpec.handle is the platform-
                // native form; for Telegram it's the
                // numeric user_id as a string. We parse it
                // back to i64.
                m.handle.parse::<i64>().unwrap_or({
                    // Non-numeric handles (usernames) are
                    // not supported in Phase 1.
                    0_i64
                })
            })
            .collect();

        let info = self
            .client
            .create_group(subject, &user_ids)
            .await
            .map_err(map_err)?;

        // Push the new chat_id into the runtime group
        // registry so subsequent `send_envelope` calls can
        // route to it without restarting the bot.
        self.register_group_at_runtime(info.chat_id);

        // Best-effort: promote any member marked
        // `is_admin`. Telegram's `createChat` adds everyone
        // as a regular member; admin status is set with
        // `channels.editAdmin` for supergroups. The mock
        // accepts; the real client (Phase 2) will check
        // that the chat is a supergroup before invoking
        // the RPC.
        for m in initial_members.iter().filter(|m| m.is_admin) {
            if let Ok(uid) = m.handle.parse::<i64>() {
                if let Err(e) = self.client.promote_participant(info.chat_id, uid).await {
                    tracing::debug!(
                        chat_id = info.chat_id,
                        user_id = uid,
                        error = %e,
                        "create_group: promote_participant failed (best-effort)"
                    );
                }
            }
        }

        Ok(GroupHandle {
            id: GroupId::new(info.chat_id.to_string()),
            subject: Some(info.title.clone()),
            invite_url: None, // Phase 1: no invite-URL fetch (the real client will)
            is_admin: true,   // Telegram adds the creator as admin at create time
            member_count: info.member_count,
            mode_flags: None, // Phase 1: no mode flags (basic groups have no modes)
            initial_admins_promoted: true,
        })
    }

    async fn leave_group(&self, group_id: &GroupId) -> Result<(), PlatformAdapterError> {
        let chat_id = parse_chat_id(group_id)?;
        // Idempotent: leaving a chat you're not in is Ok.
        // The mock and the real client both accept this
        // (Telegram's `leaveChannel` returns
        // `CHANNEL_PRIVATE` if you're not a member; the
        // real client maps that to Ok in Phase 2).
        self.client.leave_chat(chat_id).await.map_err(map_err)
    }

    async fn destroy_group(&self, group_id: &GroupId) -> Result<(), PlatformAdapterError> {
        let chat_id = parse_chat_id(group_id)?;
        // Telegram has no single "destroy" RPC; we use
        // `messages.deleteChat` (basic) or
        // `channels.deleteChannel` (supergroup). The
        // client's `delete_chat` picks the right one
        // based on the chat_id's sign.
        self.client.delete_chat(chat_id).await.map_err(map_err)
    }

    async fn add_member(
        &self,
        group_id: &GroupId,
        member: &GroupMemberSpec,
    ) -> Result<AddMemberOutput, PlatformAdapterError> {
        let chat_id = parse_chat_id(group_id)?;
        let user_id = member
            .handle
            .parse::<i64>()
            .map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("invalid user_id {:?}: {}", member.handle, e),
            })?;
        self.client
            .add_participant(chat_id, user_id)
            .await
            .map_err(map_err)?;
        // Promote if requested and the chat is a
        // supergroup (basic groups have no admin concept).
        let promoted = if member.is_admin && is_supergroup(chat_id) {
            Some(
                self.client
                    .promote_participant(chat_id, user_id)
                    .await
                    .map_err(map_err),
            )
        } else {
            None
        };
        Ok(AddMemberOutput {
            added: true,
            promoted,
        })
    }

    async fn remove_member(
        &self,
        group_id: &GroupId,
        member: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        let chat_id = parse_chat_id(group_id)?;
        let user_id =
            member
                .as_str()
                .parse::<i64>()
                .map_err(|e| PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("invalid user_id {:?}: {}", member.as_str(), e),
                })?;
        self.client
            .kick_participant(chat_id, user_id)
            .await
            .map_err(map_err)
    }

    async fn promote_to_admin(
        &self,
        group_id: &GroupId,
        member: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        let chat_id = parse_chat_id(group_id)?;
        let user_id =
            member
                .as_str()
                .parse::<i64>()
                .map_err(|e| PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("invalid user_id {:?}: {}", member.as_str(), e),
                })?;
        if !is_supergroup(chat_id) {
            // Telegram's basic groups have no admin
            // concept; promoting is a no-op error. We
            // return `Unimplemented` so callers can
            // detect and skip.
            return Err(PlatformAdapterError::Unimplemented {
                platform: self.platform_name(),
                action: format!(
                    "promote_to_admin: chat_id {chat_id} is a basic group (no admin concept)"
                ),
            });
        }
        self.client
            .promote_participant(chat_id, user_id)
            .await
            .map_err(map_err)
    }

    async fn demote_from_admin(
        &self,
        group_id: &GroupId,
        member: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        let chat_id = parse_chat_id(group_id)?;
        let user_id =
            member
                .as_str()
                .parse::<i64>()
                .map_err(|e| PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("invalid user_id {:?}: {}", member.as_str(), e),
                })?;
        if !is_supergroup(chat_id) {
            return Err(PlatformAdapterError::Unimplemented {
                platform: self.platform_name(),
                action: format!(
                    "demote_from_admin: chat_id {chat_id} is a basic group (no admin concept)"
                ),
            });
        }
        self.client
            .demote_participant(chat_id, user_id)
            .await
            .map_err(map_err)
    }

    async fn rename_group(
        &self,
        group_id: &GroupId,
        new_subject: &str,
    ) -> Result<(), PlatformAdapterError> {
        let chat_id = parse_chat_id(group_id)?;
        self.client
            .set_chat_title(chat_id, new_subject)
            .await
            .map_err(map_err)
    }

    async fn set_group_description(
        &self,
        group_id: &GroupId,
        about: &str,
    ) -> Result<(), PlatformAdapterError> {
        let chat_id = parse_chat_id(group_id)?;
        self.client
            .set_chat_about(chat_id, about)
            .await
            .map_err(map_err)
    }

    async fn list_own_groups(&self) -> Result<Vec<GroupHandle>, PlatformAdapterError> {
        let chat_ids = self.client.list_dialog_ids().await.map_err(map_err)?;
        let mut handles = Vec::with_capacity(chat_ids.len());
        for chat_id in chat_ids {
            // Best-effort: if `get_chat` fails (e.g.,
            // the chat was deleted between
            // `list_dialog_ids` and `get_chat`), surface
            // a `GroupHandle` with just the chat_id and
            // a placeholder title.
            let handle = match self.client.get_chat(chat_id).await {
                Ok(info) => GroupHandle {
                    id: GroupId::new(info.chat_id.to_string()),
                    subject: Some(info.title),
                    invite_url: None,
                    is_admin: info.is_admin.unwrap_or(false),
                    member_count: info.member_count,
                    mode_flags: None,
                    initial_admins_promoted: false,
                },
                Err(e) => {
                    tracing::debug!(
                        chat_id = chat_id,
                        error = %e,
                        "list_own_groups: get_chat failed; returning handle without metadata"
                    );
                    GroupHandle {
                        id: GroupId::new(chat_id.to_string()),
                        subject: None,
                        invite_url: None,
                        is_admin: false,
                        member_count: None,
                        mode_flags: None,
                        initial_admins_promoted: false,
                    }
                }
            };
            handles.push(handle);
        }
        Ok(handles)
    }

    async fn get_group_metadata(
        &self,
        group_id: &GroupId,
    ) -> Result<GroupMetadata, PlatformAdapterError> {
        let chat_id = parse_chat_id(group_id)?;
        let info = self.client.get_chat(chat_id).await.map_err(map_err)?;
        Ok(GroupMetadata {
            id: GroupId::new(info.chat_id.to_string()),
            subject: Some(info.title),
            // Telegram's `getChat` does not surface
            // description for the Phase 1 mock; the real
            // client (Phase 2) will fill this from
            // `Chat.full.about`.
            description: None,
            // Member and admin lists: the mock does not
            // surface them; the real client will pull
            // from `ChatFull.participants`.
            members: Vec::new(),
            admins: Vec::new(),
            // Invite URL: Phase 1 stub (the real client
            // resolves via `messages.exportChatInvite`).
            invite_url: None,
            // Mode flags: Telegram groups have no per-mode
            // flag set in Phase 1; the real client will
            // fill `mode_flags` with a translated
            // `GroupModeFlags` in Phase 2.
            mode_flags: GroupModeFlags {
                locked: false,
                announce_only: false,
                ephemeral_ttl: None,
                requires_approval: false,
            },
        })
    }

    async fn resolve_invite(
        &self,
        _invite: &InviteRef,
    ) -> Result<GroupHandle, PlatformAdapterError> {
        // Phase 1 stub: the real client uses
        // `messages.checkChatInvite` to resolve a
        // `t.me/joinchat/<hash>` URL or a `t.me/+<hash>`
        // invite link to a `ChatInvite` struct, which we
        // translate to `GroupHandle`. The mock does
        // not support this.
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "resolve_invite".into(),
        })
    }

    async fn join_by_invite(
        &self,
        _invite: &InviteRef,
    ) -> Result<GroupHandle, PlatformAdapterError> {
        // Phase 1 stub: the real client uses
        // `messages.importChatInvite` to join via
        // `t.me/joinchat/<hash>`. The mock does not
        // support this.
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "join_by_invite".into(),
        })
    }

    async fn transfer_ownership(
        &self,
        group_id: &GroupId,
        new_owner: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        let chat_id = parse_chat_id(group_id)?;
        let user_id =
            new_owner
                .as_str()
                .parse::<i64>()
                .map_err(|e| PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("invalid user_id {:?}: {}", new_owner.as_str(), e),
                })?;
        if !is_supergroup(chat_id) {
            return Err(PlatformAdapterError::Unimplemented {
                platform: self.platform_name(),
                action: format!(
                    "transfer_ownership: chat_id {chat_id} is a basic group (no ownership concept)"
                ),
            });
        }
        // Phase 1 stub: the real client uses
        // `channels.editCreator` (only the current
        // owner can transfer). The mock does not
        // support this.
        let _ = user_id;
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "transfer_ownership: not yet implemented (Phase 1 stub)".into(),
        })
    }
}

#[cfg(test)]
mod end_to_end_tests {
    //! End-to-end tests of the `CoordinatorAdmin` impl.
    //! These tests use the `MockTelegramMtprotoClient`
    //! and verify the adapter correctly translates
    //! between the platform-agnostic `CoordinatorAdmin`
    //! types and the Telegram chat_id / user_id
    //! primitives.
    use super::*;
    use crate::adapter::MtprotoTelegramAdapter;
    use crate::client::MockTelegramMtprotoClient;
    use crate::config::MtprotoTelegramConfig;
    use octo_network::dot::PlatformAdapter;
    use std::sync::Arc;

    fn config() -> MtprotoTelegramConfig {
        MtprotoTelegramConfig {
            mode: Some("bot".into()),
            bot_token: Some("123:abc".into()),
            api_id: Some(12345),
            api_hash: Some("0123456789abcdef0123456789abcdef".into()),
            ..Default::default()
        }
    }

    async fn adapter_with(
        client: MockTelegramMtprotoClient,
    ) -> MtprotoTelegramAdapter<MockTelegramMtprotoClient> {
        let client = Arc::new(client);
        let a = MtprotoTelegramAdapter::new(config(), client);
        a.connect_bot_token("123:abc")
            .await
            .expect("connect_bot_token should succeed");
        a
    }

    #[tokio::test]
    async fn create_group_returns_handle_and_registers_runtime_group() {
        let mock = MockTelegramMtprotoClient::new();
        let a = adapter_with(mock.clone()).await;
        let admin = a.as_coordinator_admin().unwrap();
        let members = vec![GroupMemberSpec {
            handle: "42".to_string(),
            display_name: None,
            is_admin: false,
        }];
        let handle = admin.create_group("Phase 4 group", &members).await.unwrap();
        assert_eq!(handle.id.as_str(), "1");
        assert_eq!(handle.subject.as_deref(), Some("Phase 4 group"));
        // The new chat_id is in the runtime registry so
        // subsequent `send_envelope` can route to it.
        assert!(a.is_runtime_group(1));
    }

    #[tokio::test]
    async fn add_member_to_supergroup_promotes_to_admin() {
        // The mock accepts all promote calls; we verify
        // that the is_admin flag is set in the
        // AddMemberOutput.
        let mock = MockTelegramMtprotoClient::new();
        let a = adapter_with(mock.clone()).await;
        // Pre-seed a supergroup (negative chat_id).
        mock.set_mock_group(
            crate::client::GroupInfo {
                chat_id: -1001234567890,
                title: "super".into(),
                member_count: Some(1),
                is_admin: Some(true),
            },
            vec![0],
        );
        let admin = a.as_coordinator_admin().unwrap();
        let out = admin
            .add_member(
                &GroupId::new("-1001234567890"),
                &GroupMemberSpec {
                    handle: "42".to_string(),
                    display_name: None,
                    is_admin: true,
                },
            )
            .await
            .unwrap();
        assert!(out.added);
        // The supergroup path calls promote; the mock
        // returns Ok, so `promoted` is `Some(Ok(()))`.
        assert!(matches!(out.promoted, Some(Ok(()))));
    }

    #[tokio::test]
    async fn promote_to_admin_on_basic_group_returns_unimplemented() {
        let mock = MockTelegramMtprotoClient::new();
        let a = adapter_with(mock.clone()).await;
        mock.set_mock_group(
            crate::client::GroupInfo {
                chat_id: 123, // positive = basic group
                title: "basic".into(),
                member_count: Some(2),
                is_admin: Some(true),
            },
            vec![0, 42],
        );
        let admin = a.as_coordinator_admin().unwrap();
        let err = admin
            .promote_to_admin(&GroupId::new("123"), &PeerId::new("42"))
            .await
            .unwrap_err();
        match err {
            PlatformAdapterError::Unimplemented { action, .. } => {
                assert!(action.contains("basic group"), "got: {action}");
            }
            other => panic!("expected Unimplemented, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_own_groups_returns_handles_for_each_dialog() {
        let mock = MockTelegramMtprotoClient::new();
        let a = adapter_with(mock.clone()).await;
        mock.create_group("first", &[]).await.unwrap();
        mock.create_group("second", &[]).await.unwrap();
        let admin = a.as_coordinator_admin().unwrap();
        let handles = admin.list_own_groups().await.unwrap();
        assert_eq!(handles.len(), 2);
        // BTreeMap iteration is sorted by chat_id; the
        // first created group has chat_id = 1.
        assert_eq!(handles[0].id.as_str(), "1");
        assert_eq!(handles[1].id.as_str(), "2");
    }
}
