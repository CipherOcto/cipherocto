//! Coordinator / admin actions on a group (RFC-0850 §8 extension).
//!
//! This trait is a **separate capability** from [`PlatformAdapter`]:
//!
//! - `PlatformAdapter` is the **envelope transport** hot path
//!   (`send_envelope`, `receive_messages`, `canonicalize`). It models
//!   "I carry envelopes in and out of a domain".
//! - `CoordinatorAdmin` is the **group management** surface
//!   (`create_group`, `add_member`, `promote`, `set_announce`, …).
//!   It models "I manage the domain itself".
//!
//! Every method on `CoordinatorAdmin` has a **default implementation
//! that returns [`PlatformAdapterError::Unimplemented`]**. Adapters opt
//! in by overriding only the methods they support; the rest remain
//! "not implemented" and the caller sees a structured error rather
//! than a panic or a missing-method break.
//!
//! This mirrors the `upload_media` / `download_media` default-
//! `Unimplemented` pattern that [`PlatformAdapter`] already uses.
//!
//! # Why a separate trait
//!
//! Putting these methods on `PlatformAdapter` would:
//! 1. Bloat the hot path (20+ methods, mostly `Unimplemented`).
//! 2. Explode the C ABI surface of plugin adapters (every method
//!    becomes an `extern "C"` export).
//! 3. Confuse the contract (transport vs. management are different
//!    responsibilities).
//!
//! # How callers use it
//!
//! The companion accessor [`PlatformAdapter::as_coordinator_admin`]
//! returns `Some(self)` only for adapters that opt in to the trait.
//! Callers do:
//!
//! ```ignore
//! if let Some(admin) = adapter.as_coordinator_admin() {
//!     if admin.admin_capabilities().can_create {
//!         let group = admin.create_group("DOT swarm A", &members).await?;
//!     }
//! }
//! ```
//!
//! # Supported platforms
//!
//! See `docs/research/coordinator-admin-actions.md` for the full
//! per-platform matrix. The short version: WhatsApp, Telegram TDLib,
//! Matrix, matrix-sdk, and IRC can plausibly support the full set;
//! Signal and nativep2p are partial; the other 13 adapters have no
//! group concept or use webhook-only modes that cannot administer
//! groups.
//!
//! # Research doc
//!
//! [`docs/research/coordinator-admin-actions.md`](https://github.com/CipherOcto/cipherocto/blob/next/docs/research/coordinator-admin-actions.md)

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::dot::error::PlatformAdapterError;

/// Platform-agnostic opaque reference to a group on some adapter.
///
/// Carries the platform-native identifier (e.g. a WhatsApp
/// `<id>@g.us` JID, a Telegram `chat_id`, a Matrix `room_id`, an
/// IRC `#channel@server`). The adapter always knows its own
/// `PlatformType` via [`PlatformAdapter::platform_type`], so the
/// `GroupId` itself only stores the native string.
///
/// For cross-adapter group references, callers should build their
/// own `(PlatformType, GroupId)` wrapper. We deliberately do not
/// bake `PlatformType` into `GroupId` to keep the type a thin
/// newtype around the platform's native format.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct GroupId(pub String);

impl GroupId {
    pub fn new(native: impl Into<String>) -> Self {
        let s: String = native.into();
        debug_assert!(!s.is_empty(), "GroupId::new called with empty string");
        Self(s)
    }
    /// Strict constructor: returns `None` if the input is empty.
    /// Use this when the input comes from user input or untrusted
    /// config and an empty value should fail loud rather than
    /// silently propagate.
    pub fn try_new(native: impl Into<String>) -> Option<Self> {
        let s: String = native.into();
        if s.is_empty() {
            None
        } else {
            Some(Self(s))
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for GroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for GroupId {
    fn from(s: String) -> Self {
        Self(s)
    }
}
impl From<&str> for GroupId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Platform-agnostic opaque reference to a peer (group member or
/// non-member).
///
/// Carries the platform-native handle in raw form: a phone number
/// for WhatsApp, an `@username` for Telegram, an `mxid` for Matrix,
/// a `did:plc:...` for Bluesky, a 32-byte hex pubkey for Nostr, a
/// nick for IRC. The adapter translates internally.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct PeerId(pub String);

impl PeerId {
    pub fn new(handle: impl Into<String>) -> Self {
        let s: String = handle.into();
        debug_assert!(!s.is_empty(), "PeerId::new called with empty string");
        Self(s)
    }
    /// Strict constructor: returns `None` if the input is empty.
    pub fn try_new(handle: impl Into<String>) -> Option<Self> {
        let s: String = handle.into();
        if s.is_empty() {
            None
        } else {
            Some(Self(s))
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for PeerId {
    fn from(s: String) -> Self {
        Self(s)
    }
}
impl From<&str> for PeerId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Caller's specification for a group member at create / add time.
///
/// Callers don't need to know the adapter's native handle format;
/// they just pass the human-readable identifier. Adapters that
/// support richer member spec (display name, admin role at create)
/// use the optional fields; adapters that don't ignore them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupMemberSpec {
    /// Native handle: phone / @user / mxid / pubkey / nick.
    /// Treated as opaque; the adapter parses.
    pub handle: String,
    /// Optional human-readable name. May be displayed by the
    /// platform or used as the contact's display name. Adapters
    /// that don't support a separate display name ignore it.
    pub display_name: Option<String>,
    /// Whether this member should be added as an admin. Only
    /// honoured at create time; `add_member` after create is
    /// always regular-member, then `promote_to_admin` if needed.
    pub is_admin: bool,
}

impl GroupMemberSpec {
    pub fn new(handle: impl Into<String>) -> Self {
        Self {
            handle: handle.into(),
            display_name: None,
            is_admin: false,
        }
    }
    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }
    pub fn as_admin(mut self) -> Self {
        self.is_admin = true;
        self
    }
}

/// Opaque reference to an invite code, URL, or token.
///
/// WhatsApp `https://chat.whatsapp.com/ABCD…`, Telegram
/// `https://t.me/+abc…`, Matrix `#alias:server`, etc.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct InviteRef(pub String);

impl InviteRef {
    pub fn new(s: impl Into<String>) -> Self {
        let v: String = s.into();
        debug_assert!(!v.is_empty(), "InviteRef::new called with empty string");
        Self(v)
    }
    /// Strict constructor: returns `None` if the input is empty.
    pub fn try_new(s: impl Into<String>) -> Option<Self> {
        let v: String = s.into();
        if v.is_empty() {
            None
        } else {
            Some(Self(v))
        }
    }
}

impl std::fmt::Display for InviteRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for InviteRef {
    fn from(s: String) -> Self {
        Self(s)
    }
}
impl From<&str> for InviteRef {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Handle returned by `create_group` / `resolve_invite` / similar.
///
/// Includes the `GroupId` plus the snapshot of fields the caller
/// is most likely to need right after create, so they don't have
/// to do a follow-up `get_group_metadata` round-trip. Any field
/// may be `None` if the platform didn't supply it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupHandle {
    /// The platform-native group ID.
    pub id: GroupId,
    /// Group subject / name (None if the platform hides it from
    /// the creator — rare but possible for invite-only groups).
    pub subject: Option<String>,
    /// Invite URL or code (None if the platform doesn't mint one,
    /// or if the creator's role doesn't permit it).
    pub invite_url: Option<String>,
    /// Whether the calling adapter is the group admin (true after
    /// `create_group`; depends on the invite-link / join path for
    /// `resolve_invite`).
    ///
    /// **Semantics (RFC-0861 §6 M14):** `true` means the calling
    /// adapter can perform admin actions (e.g. `set_locked`,
    /// `promote_to_admin`) on this group at this moment. `false`
    /// means either the adapter is not an admin, or the platform
    /// doesn't expose admin status for the bot. Adapters that
    /// don't track admin status (e.g. IRC, which doesn't read
    /// NAMES/MODE responses) MUST return `false`.
    pub is_admin: bool,
    /// Member count at create time. None if the platform doesn't
    /// surface it synchronously.
    pub member_count: Option<u32>,
    /// Mode flags at create time. None if the platform doesn't
    /// report them.
    pub mode_flags: Option<GroupModeFlags>,
    /// Whether the platform-side initial-admin promotion step has
    /// already completed for this group. `false` means the bot is
    /// still considered a regular member by the platform, even
    /// though the SDK returned it as the creator. The
    /// `create_group` post-create flow uses this to gate the
    /// optional `promote_to_admin` self-call (see RFC-0861 §3 M4
    /// and §3 H1).
    ///
    /// `#[serde(default)]` so old serialized `GroupHandle`s
    /// without this field deserialize as `false` (RFC-0861 M4).
    #[serde(default)]
    pub initial_admins_promoted: bool,
}

/// Result of an [`CoordinatorAdmin::add_member`] call.
///
/// Surfaces the partial-success semantics required by RFC-0861 §3
/// H6: the platform may report the add succeeded but the optional
/// promote-to-admin (only attempted if `GroupMemberSpec::is_admin`
/// was true at the call site) may have failed independently.
/// Callers can branch on `promoted` rather than treat the whole
/// call as a single binary outcome.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddMemberOutput {
    /// True if the platform confirmed the add. False indicates
    /// the add itself failed — in that case `promoted` is always
    /// `None` (no promote was attempted because there was no
    /// member to promote).
    pub added: bool,
    /// The result of the optional promote-to-admin. `None` if
    /// `is_admin` was false at the call site (no promote was
    /// attempted). `Some(Ok(()))` if the promote succeeded.
    /// `Some(Err(_))` if the add succeeded but the promote
    /// failed (partial-success case).
    pub promoted: Option<Result<(), PlatformAdapterError>>,
}

/// Group mode flags (subset of the underlying platform's mode
/// toggles that the `CoordinatorAdmin` trait exposes uniformly).
///
/// Each flag is a best-effort mapping; an adapter that supports a
/// given flag overrides the corresponding `set_*` method. An
/// adapter that doesn't support it leaves the flag in its default
/// state and returns `Unimplemented` from the setter.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroupModeFlags {
    /// Only admins can add new members (WhatsApp "locked",
    /// Telegram invite-link approval, IRC `MODE +l`, Matrix
    /// `m.room.join_rules` restricted).
    pub locked: bool,
    /// Only admins can post (WhatsApp "announce", Telegram
    /// `permissions.send_messages == false`, Matrix power level
    /// `events.default == 100`, IRC `MODE +m` moderated).
    pub announce_only: bool,
    /// Disappearing-message TTL. None = disabled.
    pub ephemeral_ttl: Option<Duration>,
    /// New joiners must be approved by an admin before they can
    /// read history. (WhatsApp "membership approval mode",
    /// Telegram invite-link with `creates_join_request`,
    /// Matrix `m.room.join_rules` + `m.room.member` state).
    pub requires_approval: bool,
}

/// Snapshot of a group's metadata at a point in time. Returned by
/// `get_group_metadata`. Fields are `None` if the platform didn't
/// surface them or if the caller doesn't have permission to see
/// them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupMetadata {
    pub id: GroupId,
    pub subject: Option<String>,
    pub description: Option<String>,
    pub members: Vec<PeerId>,
    pub admins: Vec<PeerId>,
    pub invite_url: Option<String>,
    pub mode_flags: GroupModeFlags,
}

/// Per-action capability bit-flags.
///
/// Returned by [`CoordinatorAdmin::admin_capabilities`]. Callers
/// use this to detect support before calling a method, so they can
/// fall back gracefully (e.g. fall back to `remove_member` when
/// `ban_member` is not supported).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdminCapabilityReport {
    // ── A. Lifecycle ──────────────────────────────────────────
    pub can_create: bool,
    /// Can join an existing group by ID or alias. False on
    /// platforms where join is always invite-gated (WhatsApp,
    /// Signal, Telegram bots).
    pub can_join_by_id: bool,
    /// Can join an existing group via an invite code/URL.
    pub can_join_by_invite: bool,
    pub can_leave: bool,
    /// Best-effort destroy (leave + revoke invite). False if the
    /// platform doesn't expose a way to revoke the invite link
    /// separately from leaving.
    pub can_destroy: bool,

    // ── B. Membership ─────────────────────────────────────────
    pub can_add_member: bool,
    pub can_remove_member: bool,
    /// Ban = remove + prevent rejoin. False on platforms that
    /// don't enforce a deny-list (WhatsApp — kicked members can
    /// rejoin via the invite link).
    pub can_ban: bool,
    pub can_promote: bool,
    pub can_demote: bool,
    /// Approve a pending join request (groups with
    /// `requires_approval`).
    pub can_approve_join: bool,

    // ── C. Mode ───────────────────────────────────────────────
    pub can_rename: bool,
    pub can_describe: bool,
    pub can_lock: bool,
    pub can_announce: bool,
    pub can_set_ephemeral: bool,
    pub can_require_approval: bool,

    // ── D. Discovery ──────────────────────────────────────────
    /// List groups the adapter is a member of.
    pub can_list_own_groups: bool,
    pub can_get_metadata: bool,
    pub can_resolve_invite: bool,

    // ── E. Handoff ────────────────────────────────────────────
    /// True only on platforms with a first-class
    /// "transfer ownership" primitive (Telegram TDLib). Adapters
    /// that implement handoff as promote + demote + leave return
    /// `false` here and callers see the multi-step dance in
    /// `transfer_ownership`'s docs.
    pub can_transfer_ownership: bool,
}

/// Coordinator / admin actions on a group.
///
/// **Optional capability.** Adapters that support any of these
/// implement this trait and override only the methods they
/// support. Adapters that don't implement the trait at all (the
/// default) are not usable for admin actions; the
/// `PlatformAdapter::as_coordinator_admin()` accessor returns
/// `None` for them.
///
/// The default for every method is
/// `Err(PlatformAdapterError::Unimplemented { .. })`. Overriding
/// one method does not require overriding any other.
#[async_trait]
pub trait CoordinatorAdmin: Send + Sync {
    /// Report which admin actions this adapter supports. Adapters
    /// must return a report that truthfully reflects which methods
    /// they override. Default: all-false.
    fn admin_capabilities(&self) -> AdminCapabilityReport {
        AdminCapabilityReport::default()
    }

    // ── A. Lifecycle ──────────────────────────────────────────

    /// Create a new group with `subject`. The calling adapter
    /// becomes the creator/admin by default. Returns a
    /// [`GroupHandle`] with the new ID and a snapshot of the
    /// fields the platform supplied at create time.
    async fn create_group(
        &self,
        _subject: &str,
        _initial_members: &[GroupMemberSpec],
    ) -> Result<GroupHandle, PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "create_group".into(),
        })
    }

    /// Leave a group the adapter is a member of. Idempotent:
    /// leaving a group that the adapter is no longer in is a
    /// no-op or a structured `Ok(())`, not an error.
    async fn leave_group(&self, _group_id: &GroupId) -> Result<(), PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "leave_group".into(),
        })
    }

    /// Best-effort destroy: leave the group and revoke any
    /// outstanding invite link. Most platforms do not expose a
    /// "this group is gone forever" operation, so this method
    /// must not be assumed to fully remove the group server-side.
    /// The group ID may still be queryable after `destroy_group`
    /// returns `Ok(())`; callers should not reuse the ID.
    async fn destroy_group(&self, group_id: &GroupId) -> Result<(), PlatformAdapterError> {
        // Default implementation: try leave, then try revoke-invite
        // (if `can_destroy` is false this whole method is
        // Unimplemented; the override is expected to do both).
        let _ = group_id;
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "destroy_group".into(),
        })
    }

    // ── B. Membership ─────────────────────────────────────────

    async fn add_member(
        &self,
        group_id: &GroupId,
        member: &GroupMemberSpec,
    ) -> Result<AddMemberOutput, PlatformAdapterError> {
        let _ = (group_id, member);
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "add_member".into(),
        })
    }

    async fn remove_member(
        &self,
        group_id: &GroupId,
        member: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (group_id, member);
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "remove_member".into(),
        })
    }

    /// Ban a member: remove them and prevent rejoin. `duration =
    /// None` means indefinite. On platforms without a true ban
    /// (WhatsApp), the adapter should return `Unimplemented` and
    /// the caller should fall back to `remove_member` + a
    /// local-side deny-list (the typical "coordinator-level ban"
    /// pattern).
    async fn ban_member(
        &self,
        group_id: &GroupId,
        member: &PeerId,
        duration: Option<Duration>,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (group_id, member, duration);
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "ban_member".into(),
        })
    }

    async fn promote_to_admin(
        &self,
        group_id: &GroupId,
        member: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (group_id, member);
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "promote_to_admin".into(),
        })
    }

    async fn demote_from_admin(
        &self,
        group_id: &GroupId,
        member: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (group_id, member);
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "demote_from_admin".into(),
        })
    }

    /// Approve a pending join request. Only meaningful on groups
    /// with `requires_approval = true`.
    async fn approve_join_request(
        &self,
        group_id: &GroupId,
        requester: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (group_id, requester);
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "approve_join_request".into(),
        })
    }

    // ── C. Mode ───────────────────────────────────────────────

    async fn rename_group(
        &self,
        group_id: &GroupId,
        new_subject: &str,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (group_id, new_subject);
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "rename_group".into(),
        })
    }

    async fn set_group_description(
        &self,
        group_id: &GroupId,
        description: &str,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (group_id, description);
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "set_group_description".into(),
        })
    }

    async fn set_locked(
        &self,
        group_id: &GroupId,
        locked: bool,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (group_id, locked);
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "set_locked".into(),
        })
    }

    async fn set_announce(
        &self,
        group_id: &GroupId,
        announce_only: bool,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (group_id, announce_only);
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "set_announce".into(),
        })
    }

    /// Set the ephemeral (disappearing-messages) TTL for a group.
    ///
    /// `ttl = None` disables ephemeral mode (equivalent to a TTL
    /// of 0). Implementations should interpret `None` as "disable".
    /// Adapters that cannot disable ephemeral mode once enabled
    /// SHOULD still return `Ok(())` from `set_ephemeral(_, None)` —
    /// the trait contract treats disable as a soft request, not an
    /// error. (RFC-0861 §6 M12.)
    ///
    /// **TTL bounds (RFC-0861 §3 M1).** Implementations MAY clamp
    /// `ttl` to a platform-specific maximum and SHOULD return
    /// `PlatformAdapterError::ApiError { code: 400, message: ... }`
    /// if the caller requests a value that would overflow the
    /// platform's wire format. WhatsApp, for example, encodes TTL
    /// as `u32` seconds; the WhatsApp impl rejects
    /// `ttl > u32::MAX` seconds with `ApiError { code: 400 }`
    /// rather than silently truncating.
    async fn set_ephemeral(
        &self,
        group_id: &GroupId,
        ttl: Option<Duration>,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (group_id, ttl);
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "set_ephemeral".into(),
        })
    }

    async fn set_require_approval(
        &self,
        group_id: &GroupId,
        require: bool,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (group_id, require);
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "set_require_approval".into(),
        })
    }

    // ── D. Discovery ──────────────────────────────────────────

    async fn list_own_groups(&self) -> Result<Vec<GroupHandle>, PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "list_own_groups".into(),
        })
    }

    /// List the bot's groups together with their active invite
    /// URLs (where available). Returned in the same order as
    /// `list_own_groups` where the platform's API permits a
    /// 1:1 pairing; adapters that can't fetch invite URLs in
    /// parallel fall back to `None` for that field. See
    /// RFC-0861 §3 M13.
    async fn list_own_groups_with_invites(
        &self,
    ) -> Result<Vec<GroupHandle>, PlatformAdapterError> {
        // Default: delegate to `list_own_groups` (the simpler
        // variant) and let the caller call `get_group_metadata`
        // per-group for the invite URL. Adapters that have a
        // batch API override this to do the join_all-style fan-out.
        self.list_own_groups().await
    }

    async fn get_group_metadata(
        &self,
        group_id: &GroupId,
    ) -> Result<GroupMetadata, PlatformAdapterError> {
        let _ = group_id;
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "get_group_metadata".into(),
        })
    }

    /// Resolve an invite code/URL to a `GroupHandle` without
    /// joining. The caller can inspect the metadata, then decide
    /// whether to call `join_by_invite`.
    async fn resolve_invite(
        &self,
        invite: &InviteRef,
    ) -> Result<GroupHandle, PlatformAdapterError> {
        let _ = invite;
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "resolve_invite".into(),
        })
    }

    /// Join a group by an invite code/URL. Distinct from
    /// `resolve_invite` because the side effect (joining) is
    /// separate from the inspection step.
    async fn join_by_invite(
        &self,
        invite: &InviteRef,
    ) -> Result<GroupHandle, PlatformAdapterError> {
        let _ = invite;
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "join_by_invite".into(),
        })
    }

    // ── E. Handoff ────────────────────────────────────────────

    /// Transfer ownership of a group to `new_owner`. Atomic on
    /// platforms with a first-class transfer primitive
    /// (Telegram TDLib's `transferChatOwnership`). On platforms
    /// without one, the adapter should override with a multi-step
    /// promote-and-demote-and-leave dance and report
    /// `can_transfer_ownership = false` in the capability report.
    async fn transfer_ownership(
        &self,
        group_id: &GroupId,
        new_owner: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        let _ = (group_id, new_owner);
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "transfer_ownership".into(),
        })
    }

    /// Helper used by the default-method error paths. Adapters
    /// override this to return the platform's short name
    /// (e.g. `"whatsapp"`, `"telegram"`, `"matrix"`). Default:
    /// `"unknown"`.
    fn platform_name(&self) -> String {
        "unknown".into()
    }
}

// ── PlatformAdapter bridge ────────────────────────────────────────
//
// `as_coordinator_admin` lives as a default method on
// `PlatformAdapter` itself (in `mod.rs`). That way the
// downcast-to-`CoordinatorAdmin` is part of the platform's own
// trait contract, not a separate `impl dyn` block. Adapters that
// implement `CoordinatorAdmin` override the default to return
// `Some(self)`; everything else gets `None`.

// ── Unit tests for the type layer (no platform logic) ─────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_id_from_str_and_string() {
        let a: GroupId = "120363012345678901@g.us".into();
        let b: GroupId = String::from("120363012345678901@g.us").into();
        assert_eq!(a, b);
        assert_eq!(a.as_str(), "120363012345678901@g.us");
        assert_eq!(a.to_string(), "120363012345678901@g.us");
    }

    #[test]
    fn peer_id_from_str() {
        let p = PeerId::new("+15551234567");
        assert_eq!(p.as_str(), "+15551234567");
    }

    #[test]
    fn group_member_spec_builder() {
        let m = GroupMemberSpec::new("+15551234567")
            .with_display_name("Alice")
            .as_admin();
        assert_eq!(m.handle, "+15551234567");
        assert_eq!(m.display_name.as_deref(), Some("Alice"));
        assert!(m.is_admin);
    }

    #[test]
    fn invite_ref_from_url() {
        let i: InviteRef = "https://chat.whatsapp.com/ABCD".into();
        assert_eq!(i.0, "https://chat.whatsapp.com/ABCD");
        assert_eq!(i.to_string(), "https://chat.whatsapp.com/ABCD");
    }

    #[test]
    fn admin_capability_report_defaults_to_all_false() {
        let r = AdminCapabilityReport::default();
        assert!(!r.can_create);
        assert!(!r.can_join_by_id);
        assert!(!r.can_join_by_invite);
        assert!(!r.can_leave);
        assert!(!r.can_destroy);
        assert!(!r.can_add_member);
        assert!(!r.can_remove_member);
        assert!(!r.can_ban);
        assert!(!r.can_promote);
        assert!(!r.can_demote);
        assert!(!r.can_approve_join);
        assert!(!r.can_rename);
        assert!(!r.can_describe);
        assert!(!r.can_lock);
        assert!(!r.can_announce);
        assert!(!r.can_set_ephemeral);
        assert!(!r.can_require_approval);
        assert!(!r.can_list_own_groups);
        assert!(!r.can_get_metadata);
        assert!(!r.can_resolve_invite);
        assert!(!r.can_transfer_ownership);
    }

    #[test]
    fn group_mode_flags_default_is_open_unlocked_unephemeral() {
        let f = GroupModeFlags::default();
        assert!(!f.locked);
        assert!(!f.announce_only);
        assert_eq!(f.ephemeral_ttl, None);
        assert!(!f.requires_approval);
    }

    /// A bare trait object with no overrides must return
    /// `Unimplemented` for every method. Uses a small
    /// `NoopAdmin` impl that overrides only `platform_name`.
    struct NoopAdmin;
    #[async_trait]
    impl CoordinatorAdmin for NoopAdmin {
        fn platform_name(&self) -> String {
            "noop".into()
        }
    }

    /// Assert that a default-`Unimplemented` method on a bare
    /// `CoordinatorAdmin` returns exactly
    /// `Err(Unimplemented { platform: "noop", action: <label> })`.
    /// The success type is explicit so the test works against
    /// every method which has a different `Ok` type.
    fn expect_unimplemented<T: std::fmt::Debug>(r: Result<T, PlatformAdapterError>, action: &str) {
        match r {
            Err(PlatformAdapterError::Unimplemented {
                platform,
                action: a,
            }) => {
                assert_eq!(platform, "noop", "{action}: platform");
                assert_eq!(a, action, "{action}: action");
            }
            other => panic!("expected Unimplemented for {action}, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn default_methods_return_unimplemented_with_platform_name() {
        let admin = NoopAdmin;
        let g = GroupId::new("test");
        let p = PeerId::new("test");
        let m = GroupMemberSpec::new("test");
        let inv = InviteRef::new("test");
        let ttl = Some(Duration::from_secs(60));

        expect_unimplemented::<GroupHandle>(admin.create_group("s", &[]).await, "create_group");
        expect_unimplemented::<()>(admin.leave_group(&g).await, "leave_group");
        expect_unimplemented::<()>(admin.destroy_group(&g).await, "destroy_group");
        expect_unimplemented::<AddMemberOutput>(
            admin.add_member(&g, &m).await,
            "add_member",
        );
        expect_unimplemented::<()>(admin.remove_member(&g, &p).await, "remove_member");
        expect_unimplemented::<()>(admin.ban_member(&g, &p, ttl).await, "ban_member");
        expect_unimplemented::<()>(admin.promote_to_admin(&g, &p).await, "promote_to_admin");
        expect_unimplemented::<()>(admin.demote_from_admin(&g, &p).await, "demote_from_admin");
        expect_unimplemented::<()>(
            admin.approve_join_request(&g, &p).await,
            "approve_join_request",
        );
        expect_unimplemented::<()>(admin.rename_group(&g, "x").await, "rename_group");
        expect_unimplemented::<()>(
            admin.set_group_description(&g, "x").await,
            "set_group_description",
        );
        expect_unimplemented::<()>(admin.set_locked(&g, true).await, "set_locked");
        expect_unimplemented::<()>(admin.set_announce(&g, true).await, "set_announce");
        expect_unimplemented::<()>(admin.set_ephemeral(&g, ttl).await, "set_ephemeral");
        expect_unimplemented::<()>(
            admin.set_require_approval(&g, true).await,
            "set_require_approval",
        );
        expect_unimplemented::<Vec<GroupHandle>>(admin.list_own_groups().await, "list_own_groups");
        expect_unimplemented::<GroupMetadata>(
            admin.get_group_metadata(&g).await,
            "get_group_metadata",
        );
        expect_unimplemented::<GroupHandle>(admin.resolve_invite(&inv).await, "resolve_invite");
        expect_unimplemented::<GroupHandle>(admin.join_by_invite(&inv).await, "join_by_invite");
        expect_unimplemented::<()>(admin.transfer_ownership(&g, &p).await, "transfer_ownership");
    }

    #[test]
    fn noop_admin_capability_report_is_all_false() {
        let admin = NoopAdmin;
        let r = admin.admin_capabilities();
        assert!(!r.can_create && !r.can_leave && !r.can_add_member);
    }

    // ── M2: try_new constructors ───────────────────────────────

    #[test]
    fn group_id_try_new_accepts_non_empty() {
        let g = GroupId::try_new("120363012345678901@g.us");
        assert_eq!(g.unwrap().as_str(), "120363012345678901@g.us");
    }

    #[test]
    fn group_id_try_new_rejects_empty() {
        assert!(GroupId::try_new("").is_none());
        assert!(GroupId::try_new(String::new()).is_none());
    }

    #[test]
    fn peer_id_try_new_accepts_non_empty() {
        let p = PeerId::try_new("+15551234567");
        assert_eq!(p.unwrap().as_str(), "+15551234567");
    }

    #[test]
    fn peer_id_try_new_rejects_empty() {
        assert!(PeerId::try_new("").is_none());
    }

    #[test]
    fn invite_ref_try_new_accepts_non_empty() {
        let i = InviteRef::try_new("https://chat.whatsapp.com/ABCD");
        assert_eq!(i.unwrap().0, "https://chat.whatsapp.com/ABCD");
    }

    #[test]
    fn invite_ref_try_new_rejects_empty() {
        assert!(InviteRef::try_new("").is_none());
    }

    // ── H6: AddMemberOutput discriminator ───────────────────────
    //
    // A fake admin that returns the three `promoted` variants
    // verifies callers can branch on each independently. The
    // trait's default returns `Unimplemented`, so we test the
    // variant construction directly (the trait impl detail is
    // covered by adapter tests in Phase 2/3).

    #[test]
    fn add_member_output_none_when_no_promote_attempted() {
        let out = AddMemberOutput {
            added: true,
            promoted: None,
        };
        assert!(out.added);
        assert!(out.promoted.is_none());
    }

    #[test]
    fn add_member_output_some_ok_when_promote_succeeded() {
        let out = AddMemberOutput {
            added: true,
            promoted: Some(Ok(())),
        };
        assert!(out.promoted.as_ref().unwrap().is_ok());
    }

    #[test]
    fn add_member_output_some_err_when_promote_failed_after_add_succeeded() {
        let out = AddMemberOutput {
            added: true,
            promoted: Some(Err(PlatformAdapterError::ApiError {
                code: 500,
                message: "promote failed".into(),
            })),
        };
        assert!(out.added);
        let promoted_err = out.promoted.unwrap().unwrap_err();
        match promoted_err {
            PlatformAdapterError::ApiError { code, message } => {
                assert_eq!(code, 500);
                assert_eq!(message, "promote failed");
            }
            other => panic!("expected ApiError, got {other:?}"),
        }
    }

    // ── M4: initial_admins_promoted default + serde roundtrip ──

    #[test]
    fn group_handle_initial_admins_promoted_defaults_to_false() {
        let h = GroupHandle {
            id: GroupId::new("g"),
            subject: None,
            invite_url: None,
            is_admin: false,
            member_count: None,
            mode_flags: None,
            initial_admins_promoted: false,
        };
        assert!(!h.initial_admins_promoted);
    }

    #[test]
    fn group_handle_deserializes_without_initial_admins_promoted_field() {
        // Simulates an old serialized GroupHandle written before
        // M4 added the field. #[serde(default)] should make the
        // missing field deserialize as false.
        let json = r#"{
            "id": "120363012345678901@g.us",
            "subject": null,
            "invite_url": null,
            "is_admin": false,
            "member_count": null,
            "mode_flags": null
        }"#;
        let h: GroupHandle = serde_json::from_str(json).unwrap();
        assert!(!h.initial_admins_promoted);
        assert_eq!(h.id.as_str(), "120363012345678901@g.us");
    }

    // ── M13: list_own_groups_with_invites default delegates ────

    /// A noop admin that overrides `list_own_groups` but NOT
    /// `list_own_groups_with_invites` exercises the trait's
    /// default delegation behavior.
    struct GroupsAdmin(Vec<GroupHandle>);
    #[async_trait]
    impl CoordinatorAdmin for GroupsAdmin {
        fn platform_name(&self) -> String {
            "groups".into()
        }
        async fn list_own_groups(&self) -> Result<Vec<GroupHandle>, PlatformAdapterError> {
            Ok(self.0.clone())
        }
    }

    #[tokio::test]
    async fn list_own_groups_with_invites_default_delegates_to_list_own_groups() {
        let admin = GroupsAdmin(vec![GroupHandle {
            id: GroupId::new("g1"),
            subject: None,
            invite_url: None,
            is_admin: false,
            member_count: None,
            mode_flags: None,
            initial_admins_promoted: false,
        }]);
        let via_default = admin.list_own_groups_with_invites().await.unwrap();
        let via_primary = admin.list_own_groups().await.unwrap();
        assert_eq!(via_default.len(), via_primary.len());
        assert_eq!(via_default.len(), 1);
    }
}
