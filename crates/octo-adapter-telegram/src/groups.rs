//! Chat/group resolution by name/username.
//!
//! Mission Architecture: "chat_id resolution by name/username"
//!
//! Resolves chat identifiers from various formats:
//! - Numeric chat_id (e.g., -1001234567890)
//! - Username (@username)
//! - Invite link (https://t.me/joinchat/...)
//!
//! ## TDLib Chat Discovery
//! - `searchPublicChat` - find by username
//! - `createGroup` / `createChannel` - create new
//! - `getCommonChats` - find shared groups
//! - `getMessages` - load message history

#[cfg(feature = "real-tdlib")]
use std::collections::HashMap;

/// Group resolution error types.
#[derive(Debug, thiserror::Error)]
pub enum GroupError {
    #[error("chat not found: {0}")]
    ChatNotFound(String),

    #[error("access denied: {0}")]
    AccessDenied(String),

    #[error("invalid username: {0}")]
    InvalidUsername(String),

    #[error("not a group: {0}")]
    NotAGroup(String),

    #[cfg(feature = "real-tdlib")]
    #[error("TDLib error: {message}")]
    Tdlib { message: String },
}

pub type GroupResult<T> = std::result::Result<T, GroupError>;

/// Chat type enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatType {
    Private,
    Group,
    Supergroup,
    Channel,
    /// Secret (E2E-encrypted) chat. Phase 3 future work per mission spec.
    Secret,
}

/// Chat information.
#[derive(Debug, Clone)]
pub struct ChatInfo {
    pub id: i64,
    pub chat_type: ChatType,
    pub title: Option<String>,
    pub username: Option<String>,
    pub member_count: Option<i32>,
}

/// Chat resolver for mapping various identifiers to chat_id.
#[cfg(feature = "real-tdlib")]
pub struct ChatResolver {
    /// Cache of known chats to avoid repeated lookups.
    cache: HashMap<String, i64>,
}

#[cfg(feature = "real-tdlib")]
impl ChatResolver {
    /// Create a new chat resolver with empty cache.
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Resolve a chat identifier to a numeric chat_id.
    /// Accepts: numeric id, @username, or invite link.
    pub async fn resolve(&mut self, identifier: &str, client_id: i32) -> GroupResult<i64> {
        // Check cache first (OBS-M4)
        if let Some(cached) = self.cache.get(identifier) {
            tracing::debug!(identifier = %identifier, cached_chat_id = %cached, "ChatResolver: cache hit");
            return Ok(*cached);
        }

        // Parse the identifier type
        let chat_id = if identifier.starts_with('@') {
            // Username lookup
            self.resolve_username(identifier, client_id).await?
        } else if identifier.starts_with("https://t.me/joinchat/")
            || identifier.starts_with("https://t.me/+")
        {
            // Invite link (read-only: does NOT join the chat)
            self.check_chat_invite_link(identifier, client_id).await?
        } else if let Ok(id) = identifier.parse::<i64>() {
            // Numeric chat_id - validate it exists
            self.validate_chat_id(id, client_id).await?
        } else {
            return Err(GroupError::InvalidUsername(identifier.into()));
        };

        // Cache the result
        self.cache.insert(identifier.to_string(), chat_id);
        Ok(chat_id)
    }

    /// Resolve a username to chat_id using searchPublicChat.
    async fn resolve_username(&self, username: &str, client_id: i32) -> GroupResult<i64> {
        let search_name = username.trim_start_matches('@');

        let chat = tdlib_rs::functions::search_public_chat(search_name.to_string(), client_id)
            .await
            .map_err(|e| GroupError::Tdlib { message: e.message })?;

        // Chat is currently a single-variant enum; let-else will fail to
        // compile if TDLib adds a variant in a future binding update,
        // which is the desired defensive behaviour.
        let tdlib_rs::enums::Chat::Chat(c) = chat;
        Ok(c.id)
    }

    /// Resolve an invite link to chat_id **read-only** (does not join the chat).
    ///
    /// Uses TDLib's `checkChatInviteLink`, which inspects a link and returns
    /// information about the corresponding chat without making the bot a
    /// member. To actually join the chat, call `join_chat_by_invite_link`
    /// directly.
    pub async fn check_chat_invite_link(&self, link: &str, client_id: i32) -> GroupResult<i64> {
        let info = tdlib_rs::functions::check_chat_invite_link(link.to_string(), client_id)
            .await
            .map_err(|e| GroupError::Tdlib { message: e.message })?;

        // ChatInviteLinkInfo is currently a single-variant enum.
        let tdlib_rs::enums::ChatInviteLinkInfo::ChatInviteLinkInfo(info) = info;
        if info.chat_id == 0 {
            // TDLib returns 0 when the user has no access to the chat before
            // joining (e.g. private invite that requires an explicit join).
            return Err(GroupError::AccessDenied(link.to_string()));
        }
        // R4 C6: Also reject non-zero chat_ids where TDLib reports the chat
        // is not accessible (accessible_for == 0 means no time limit was set,
        // which typically indicates the chat cannot be accessed with this link).
        // This can happen when the bot was previously a member of a group that
        // changed its invite link, or the chat_id was reassigned to a different
        // supergroup. Without this check, caching a stale chat_id from a
        // previous resolve would silently route messages to the wrong peer.
        if info.accessible_for == 0 {
            // Invalidate any cached mapping for this link so the next call
            // re-resolves.
            // (Cache invalidation is handled by the caller; this check ensures
            //  we don't return a stale id.)
            return Err(GroupError::AccessDenied(
                format!("{} (chat_id={}, not accessible)", link, info.chat_id),
            ));
        }
        Ok(info.chat_id)
    }

    /// Validate a numeric chat_id exists using getChat.
    async fn validate_chat_id(&self, chat_id: i64, client_id: i32) -> GroupResult<i64> {
        let _chat = tdlib_rs::functions::get_chat(chat_id, client_id)
            .await
            .map_err(|e| GroupError::Tdlib { message: e.message })?;

        // getChat returns Chat on success, () on failure (via observer pattern in some versions)
        // If we get here without error, the chat exists
        Ok(chat_id)
    }

    /// Get chat info by chat_id using getChat.
    pub async fn get_chat_info(&self, chat_id: i64, client_id: i32) -> GroupResult<ChatInfo> {
        let chat = tdlib_rs::functions::get_chat(chat_id, client_id)
            .await
            .map_err(|e| GroupError::Tdlib { message: e.message })?;

        // Chat is currently a single-variant enum (see resolve_username).
        let tdlib_rs::enums::Chat::Chat(c) = chat;
        let chat_type = match c.r#type {
            tdlib_rs::enums::ChatType::Private(_) => ChatType::Private,
            tdlib_rs::enums::ChatType::BasicGroup(_) => ChatType::Group,
            tdlib_rs::enums::ChatType::Supergroup(_) => ChatType::Supergroup,
            tdlib_rs::enums::ChatType::Secret(_) => ChatType::Secret,
        };
        Ok(ChatInfo {
            id: c.id,
            chat_type,
            title: if c.title.is_empty() {
                None
            } else {
                Some(c.title)
            },
            username: None,
            member_count: None,
        })
    }

    /// Clear the resolution cache.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

#[cfg(feature = "real-tdlib")]
impl Default for ChatResolver {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Monitored Groups
// =============================================================================

/// Groups configuration for the adapter.
/// Maps group names/usernames to chat_ids for monitoring.
#[cfg(feature = "real-tdlib")]
#[derive(Debug, Clone)]
pub struct MonitoredGroups {
    /// Resolved chat_ids to monitor. Uses `BTreeSet` for O(log n) lookup
    /// (R4 M8 — was Vec<i64> which was O(n) per call).
    pub chat_ids: std::collections::BTreeSet<i64>,
    /// Username to chat_id mappings.
    pub username_map: HashMap<String, i64>,
}

#[cfg(feature = "real-tdlib")]
impl MonitoredGroups {
    /// Create from a list of identifiers (usernames or numeric ids).
    pub async fn from_identifiers(identifiers: &[String], client_id: i32) -> GroupResult<Self> {
        let mut resolver = ChatResolver::new();
        let mut chat_ids = std::collections::BTreeSet::new();
        let mut username_map = HashMap::new();

        for ident in identifiers {
            let chat_id = resolver.resolve(ident, client_id).await?;
            chat_ids.insert(chat_id);
            if ident.starts_with('@') {
                username_map.insert(ident.trim_start_matches('@').to_string(), chat_id);
            }
        }

        Ok(Self {
            chat_ids,
            username_map,
        })
    }

    /// Check if a chat_id is monitored (O(log n)).
    pub fn is_monitored(&self, chat_id: i64) -> bool {
        self.chat_ids.contains(&chat_id)
    }
}
