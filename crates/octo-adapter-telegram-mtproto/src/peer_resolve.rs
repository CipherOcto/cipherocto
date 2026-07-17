//! Peer resolution and input-peer construction helpers for
//! `real_network::RealTelegramMtprotoClient`.
//!
//! Telegram's MTProto API distinguishes between a peer's
//! "stable" identifier (a `chat_id`, `user_id`, or
//! `channel_id`) and its "input" form, which carries the
//! `access_hash` bound to the local session. The access
//! hash is required for users and channels; basic groups
//! don't need one.
//!
//! Most group ops in `real_client.rs` need to:
//!   1. Look up the input form for a given `chat_id`.
//!   2. Look up the input form for a given `user_id`
//!      (e.g., when adding a participant).
//!   3. Distinguish basic groups from supergroups so the
//!      correct TL RPC is called (`messages.*` vs.
//!      `channels.*`).
//!
//! This module centralises that logic. Each helper here is
//! a thin adapter between the high-level `grammers_client`
//! API (`Client::resolve_peer`, `Peer::to_ref`) and the
//! raw TL types (`tl::enums::InputPeer`, etc.) that the
//! generated TL functions expect.

use std::sync::Arc;

use grammers_client::peer::Peer;
use grammers_session::types::{PeerAuth, PeerId, PeerKind, PeerRef};
use tracing::debug;

use crate::error::MtprotoTelegramError;

// Re-export `grammers_tl_types` under `tl` for the file
// scope. Generated functions live in `grammers_tl_types`,
// not `grammers_client::tl` (the latter is just a re-export).
#[cfg(feature = "real-network")]
use grammers_tl_types as tl;

/// The high-level grammers client type. Used in `Arc` form
/// to match `real_client.rs`.
pub(super) type GrammersClient = grammers_client::Client;

/// Telegram assigns negative IDs to chats/supergroups.
///
/// Historically there were two encodings:
///   * Basic group: a plain negative integer, e.g., `-12345`.
///   * Supergroup/channel: `-(chat_id + 1_000_000_000_000)`.
///
/// In 2018 Telegram retroactively migrated most basic
/// groups to supergroups. After migration the basic-group
/// negative ID becomes "stale" but the chat still resolves
/// as a basic group via TL `Chat` (not `Channel`) until
/// the session learns otherwise. The `chat_id < 0` heuristic
/// was historically sufficient but is wrong for legacy
/// migrated basic groups (negative without the `-1e12`
/// offset). The audit (R19-C1) tightened this.
///
/// The boundary is documented in TL: a supergroup's chat_id
/// is `<= -1_000_000_000_001` (the `-1_000_000_000_000`
/// offset added back in by clients, and the bare channel
/// ID is at least 1). `chat_id == -1_000_000_000_000` is
/// not a valid Telegram ID at all.
///
/// Note that `peer_id_to_chat_id` (in the inverse helper)
/// reverses this transformation.
/// Largest negative chat_id that still denotes a basic
/// (legacy) group. Anything `<= -1_000_000_000_001` is a
/// supergroup/channel. We expose this so call sites that
/// take a `chat_id` and need to dispatch by chat kind
/// (basic vs supergroup) can use the same boundary the
/// rest of this module uses.
pub(super) const SUPERGROUP_CHAT_ID_MAX_NEG: i64 = -1_000_000_000_001;

/// Convert a Telegram `chat_id` (i64) to a `PeerId`.
/// Positive IDs are users; small negative IDs (in
/// `[-999_999_999_999, -1]`) are basic groups; very
/// negative IDs (`<= -1_000_000_000_001`) are supergroups
/// or channels. `chat_id == 0` and
/// `chat_id == -1_000_000_000_000` are not valid Telegram
/// IDs and will panic via the `*_unchecked` constructors.
pub(super) fn chat_id_to_peer_id(chat_id: i64) -> PeerId {
    if chat_id > 0 {
        // Positive IDs are users in Telegram's scheme.
        PeerId::user_unchecked(chat_id)
    } else if chat_id <= SUPERGROUP_CHAT_ID_MAX_NEG {
        // Supergroups/channels: strip the `-1e12` offset
        // and take the bare (positive) channel id.
        let bare_channel_id = chat_id
            .checked_add(1_000_000_000_000)
            .expect("supergroup chat_id underflow");
        // Make it positive.
        let bare_channel_id = bare_channel_id.unsigned_abs() as i64;
        PeerId::channel_unchecked(bare_channel_id)
    } else if chat_id < 0 {
        // Basic groups: a plain negative integer; the
        // constructor takes the bare (positive) chat id.
        let bare_chat_id = chat_id.unsigned_abs();
        PeerId::chat_unchecked(bare_chat_id as i64)
    } else {
        // chat_id == 0 is not a valid Telegram id. Panic
        // loudly so the caller can fix the input rather
        // than silently treating it as a user.
        panic!("chat_id_to_peer_id: chat_id 0 is not a valid Telegram id");
    }
}

/// Convert a Telegram `user_id` (i64) to a `PeerId`.
/// Telegram user IDs are positive integers.
pub(super) fn user_id_to_peer_id(user_id: i64) -> PeerId {
    debug_assert!(user_id >= 0, "user_id must be non-negative");
    if user_id == 0 {
        // 0 is not a valid Telegram user id (UserSelf
        // uses a sentinel value).
        panic!("user_id_to_peer_id: user_id 0 is not a valid Telegram id");
    }
    PeerId::user_unchecked(user_id)
}

/// Inverse of `chat_id_to_peer_id`. Useful when
/// constructing an `OctoChatId` from a `Peer`.
pub(super) fn peer_id_to_chat_id(peer_id: PeerId) -> i64 {
    match peer_id.kind() {
        PeerKind::User | PeerKind::UserSelf => peer_id.bare_id(),
        PeerKind::Chat => -peer_id.bare_id(),
        PeerKind::Channel => -(peer_id.bare_id() + 1_000_000_000_000),
    }
}

/// Convert a bare Telegram channel id (positive `long` as
/// it appears in TL `Channel.id` / `Update::Channel.channel_id`)
/// to the chat_id form used by this adapter. Channels are
/// `id + 1_000_000_000_000`, and the negative chat_id form is
/// `-(bare_id + 1_000_000_000_000)`.
pub(super) fn channel_id_to_chat_id(channel_id: i64) -> i64 {
    -(channel_id + 1_000_000_000_000)
}

/// Construct a `PeerRef` with no `access_hash` (i.e., the
/// ambient default). Use this as input to
/// `Client::resolve_peer`, which then populates the cache
/// after the TL lookup.
fn peer_ref_without_auth(peer_id: PeerId) -> PeerRef {
    PeerRef {
        id: peer_id,
        auth: PeerAuth::default(),
    }
}

/// Resolve a `chat_id` to a `Peer` via
/// `Client::resolve_peer`. The session cache is consulted
/// first; on miss the appropriate TL RPC is invoked.
///
/// `require_supergroup`: if true, the chat_id is required
/// to be a supergroup/channel (`chat_id <= -1_000_000_000_001`).
/// If false, a basic group is also accepted. This guards
/// against accidentally routing a basic group chat_id to
/// a `channels.*` RPC.
pub(super) async fn resolve_chat(
    client: &Arc<GrammersClient>,
    chat_id: i64,
    require_supergroup: bool,
) -> Result<Peer, MtprotoTelegramError> {
    let peer_id = chat_id_to_peer_id(chat_id);
    if require_supergroup && peer_id.kind() != PeerKind::Channel {
        return Err(MtprotoTelegramError::Config(format!(
            "chat_id {chat_id} is not a supergroup or channel (PeerKind: {:?})",
            peer_id.kind()
        )));
    }
    let peer_ref = peer_ref_without_auth(peer_id);
    client.resolve_peer(peer_ref).await.map_err(|e| {
        let code = tl_invoke_error_code(&e);
        MtprotoTelegramError::Rpc {
            code,
            message: format!("resolve_peer(chat_id={chat_id}): {e}"),
        }
    })
}

/// Resolve a `user_id` to a `Peer` via `Client::resolve_peer`.
pub(super) async fn resolve_user(
    client: &Arc<GrammersClient>,
    user_id: i64,
) -> Result<Peer, MtprotoTelegramError> {
    let peer_id = user_id_to_peer_id(user_id);
    let peer_ref = peer_ref_without_auth(peer_id);
    client.resolve_peer(peer_ref).await.map_err(|e| {
        let code = tl_invoke_error_code(&e);
        MtprotoTelegramError::Rpc {
            code,
            message: format!("resolve_peer(user_id={user_id}): {e}"),
        }
    })
}

/// After `Client::resolve_peer` returns a `Peer`, the peer
/// has been added to the session cache. Call
/// `Peer::to_ref().await` to obtain a `PeerRef` with the
/// real `access_hash`. This is the canonical way to build
/// TL inputs that need an access_hash.
pub(super) async fn peer_to_ref(peer: &Peer) -> Result<PeerRef, MtprotoTelegramError> {
    peer.to_ref()
        .await
        .ok_or_else(|| MtprotoTelegramError::Rpc {
            code: 404,
            message: "peer.to_ref(): no access_hash in session cache".into(),
        })
}

/// Extract the `InputPeer` from a resolved `Peer`. For
/// users this is `InputPeer::User { user_id, access_hash }`
/// (or `InputPeer::PeerSelf` for self); for basic chats
/// `InputPeer::Chat { chat_id }`; for channels
/// `InputPeer::Channel { channel_id, access_hash }`.
pub(super) async fn peer_to_input_peer(
    peer: &Peer,
) -> Result<tl::enums::InputPeer, MtprotoTelegramError> {
    let peer_ref = peer_to_ref(peer).await?;
    Ok(match peer_ref.id.kind() {
        PeerKind::UserSelf => tl::enums::InputPeer::PeerSelf,
        PeerKind::User => tl::enums::InputPeer::User(tl::types::InputPeerUser {
            user_id: peer_ref.id.bare_id(),
            access_hash: peer_ref.auth.hash(),
        }),
        PeerKind::Chat => tl::enums::InputPeer::Chat(tl::types::InputPeerChat {
            chat_id: peer_ref.id.bare_id(),
        }),
        PeerKind::Channel => tl::enums::InputPeer::Channel(tl::types::InputPeerChannel {
            channel_id: peer_ref.id.bare_id(),
            access_hash: peer_ref.auth.hash(),
        }),
    })
}

/// Extract `InputUser` from a resolved user-peer. The
/// caller must ensure `peer` is a `Peer::User`. Returns
/// an error otherwise.
pub(super) async fn peer_to_input_user(
    peer: &Peer,
) -> Result<tl::enums::InputUser, MtprotoTelegramError> {
    let peer_ref = peer_to_ref(peer).await?;
    match peer_ref.id.kind() {
        PeerKind::UserSelf => Ok(tl::enums::InputUser::UserSelf),
        PeerKind::User => Ok(tl::enums::InputUser::User(tl::types::InputUser {
            user_id: peer_ref.id.bare_id(),
            access_hash: peer_ref.auth.hash(),
        })),
        other => Err(MtprotoTelegramError::Config(format!(
            "peer_to_input_user: peer is not a user (PeerKind: {other:?})"
        ))),
    }
}

/// Resolve a user_id to an `InputUser` in one step.
/// Convenience wrapper over `resolve_user` +
/// `peer_to_input_user`. Returns `InputUser::UserSelf`
/// if the user_id matches the signed-in user.
pub(super) async fn user_id_to_input_user(
    client: &Arc<GrammersClient>,
    user_id: i64,
) -> Result<tl::enums::InputUser, MtprotoTelegramError> {
    let peer = resolve_user(client, user_id).await?;
    peer_to_input_user(&peer).await
}

/// Extract `InputChannel` from a resolved channel-peer.
/// The caller must ensure `peer` is a `Peer::Channel`.
pub(super) async fn peer_to_input_channel(
    peer: &Peer,
) -> Result<tl::enums::InputChannel, MtprotoTelegramError> {
    let peer_ref = peer_to_ref(peer).await?;
    if peer_ref.id.kind() != PeerKind::Channel {
        return Err(MtprotoTelegramError::Config(format!(
            "peer_to_input_channel: peer is not a channel (PeerKind: {:?})",
            peer_ref.id.kind()
        )));
    }
    Ok(tl::enums::InputChannel::Channel(tl::types::InputChannel {
        channel_id: peer_ref.id.bare_id(),
        access_hash: peer_ref.auth.hash(),
    }))
}

/// Convert a `grammers_client::InvocationError` to the
/// integer RPC code carried by `MtprotoTelegramError::Rpc`.
///
/// Some `InvocationError` variants carry an RPC error code
/// directly (the `Rpc` variant). Others represent transport
/// problems and don't; we map them to sensible HTTP-ish
/// status codes:
///   * `Dropped`, `InvalidDc`, `MigrateDc` -> 503
///   * `Io`, `Parse`, `Deserialize`, `Authentication` -> 500
///   * `Transport` -> 502 (bad gateway)
///   * `TimedOut` -> 504
fn tl_invoke_error_code(e: &grammers_client::InvocationError) -> i32 {
    use grammers_client::InvocationError;
    match e {
        InvocationError::Rpc(rpc_err) => rpc_err.code,
        InvocationError::Dropped => 503,
        InvocationError::InvalidDc => 503,
        InvocationError::Authentication(_) => 500,
        InvocationError::Io(_) => 500,
        InvocationError::Transport(_) => 502,
        InvocationError::Deserialize(_) => 500,
    }
}

/// Map any `InvocationError` to `MtprotoTelegramError::Rpc`.
/// Centralised so every RPC site in `real_client.rs` can
/// use one helper instead of hand-rolling the match.
pub(super) fn map_invoke_err(
    prefix: &str,
    e: grammers_client::InvocationError,
) -> MtprotoTelegramError {
    debug!(prefix, error = %e, "grammers RPC failed");
    MtprotoTelegramError::Rpc {
        code: tl_invoke_error_code(&e),
        message: format!("{prefix}: {e}"),
    }
}

/// Map a `grammers_client::SignInError` (only relevant on
/// `sign_in`) to `MtprotoTelegramError::Auth`.
///
/// Currently not used by `RealTelegramMtprotoClient`
/// (sign-in errors are handled via the existing
/// `MtprotoAuthError` flow). Kept as a public helper so
/// the coordinator-admin and any future sign-in
/// integration can use it without duplicating the format.
#[allow(dead_code)]
pub(super) fn map_signin_err(
    prefix: &str,
    e: grammers_client::SignInError,
) -> MtprotoTelegramError {
    debug!(prefix, error = %e, "grammers sign-in failed");
    MtprotoTelegramError::Auth(format!("{prefix}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip positive user IDs.
    #[test]
    fn user_id_round_trip() {
        let pid = user_id_to_peer_id(42_000_000);
        assert_eq!(pid.kind(), PeerKind::User);
        assert_eq!(peer_id_to_chat_id(pid), 42_000_000);
    }

    /// Round-trip a supergroup chat_id (negative, very large magnitude).
    #[test]
    fn supergroup_chat_id_round_trip() {
        // Telegram supergroup chat_id example:
        //   bare_id = 123_456_7890 -> chat_id = -(1_000_000_000_000 + 123_456_7890)
        let original_chat_id = -(1_000_000_000_000 + 1_234_567_890);
        let pid = chat_id_to_peer_id(original_chat_id);
        assert_eq!(pid.kind(), PeerKind::Channel);
        assert_eq!(peer_id_to_chat_id(pid), original_chat_id);
    }

    /// Round-trip a basic-group chat_id (negative, small magnitude).
    #[test]
    fn basic_group_chat_id_round_trip() {
        let original_chat_id = -123_456;
        let pid = chat_id_to_peer_id(original_chat_id);
        assert_eq!(pid.kind(), PeerKind::Chat);
        assert_eq!(peer_id_to_chat_id(pid), original_chat_id);
    }

    /// Boundary: chat_id == -1_000_000_000_001 is a supergroup
    /// (the minimum valid supergroup chat_id).
    #[test]
    fn boundary_minus_one_trillion_minus_one_is_supergroup() {
        let pid = chat_id_to_peer_id(SUPERGROUP_CHAT_ID_MAX_NEG);
        assert_eq!(pid.kind(), PeerKind::Channel);
    }

    /// Boundary: chat_id == -1_000_000_000_000 is NOT a
    /// supergroup; it's an invalid Telegram ID, but our
    /// classifier treats it as a basic group attempt (which
    /// would fail via PeerId::chat_unchecked because abs
    /// exceeds CHAT_ID_RANGE). Verify it doesn't panic
    /// through the supergroup branch.
    #[test]
    #[should_panic(expected = "chat ID out of range")]
    fn boundary_just_above_min_is_out_of_range() {
        let _ = chat_id_to_peer_id(SUPERGROUP_CHAT_ID_MAX_NEG + 1);
    }

    /// Legacy migrated basic group (negative without the
    /// `-1e12` offset) is correctly classified as basic.
    #[test]
    fn legacy_migrated_basic_group_is_basic() {
        let pid = chat_id_to_peer_id(-12345);
        assert_eq!(pid.kind(), PeerKind::Chat);
    }

    /// Helper: tl_invoke_error_code on the `Dropped` variant
    /// returns 503. We construct an `InvocationError::Dropped`
    /// directly since it's a unit variant.
    #[test]
    fn invoke_error_dropped_maps_to_503() {
        let e = grammers_client::InvocationError::Dropped;
        assert_eq!(tl_invoke_error_code(&e), 503);
    }
}
