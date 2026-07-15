//! CLI for `octo-whatsapp`. Subcommand tree mirrors the RPC surface.
//!
//! Phase 1 wires each top-level command to its corresponding RPC method.
//! The `onboard` subcommand does NOT require a running daemon — it prints a
//! message instructing the user to invoke the standalone `octo-whatsapp-onboard`
//! binary. All other subcommands connect to the daemon socket.
//!
//! See `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-phase1.md` §Part K.

use std::path::PathBuf;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "octo-whatsapp",
    version,
    about = "WhatsApp runtime daemon + operator CLI + MCP mirror"
)]
pub struct Cli {
    /// Daemon socket path. Defaults to $XDG_RUNTIME_DIR/octo-whatsapp-{name}.sock.
    #[arg(long, global = true)]
    pub socket: Option<PathBuf>,

    /// Daemon instance name (multi-instance). Default: "default".
    #[arg(long, global = true, default_value = "default")]
    pub name: String,

    /// Emit JSON instead of human-friendly text.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Args)]
pub struct CommunityCmd {
    #[command(subcommand)]
    pub action: CommunityAction,
}

#[derive(Debug, Subcommand)]
pub enum CommunityAction {
    /// Create a new community.
    Create {
        #[arg(value_name = "NAME")]
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long, default_value_t = false)]
        closed: bool,
        #[arg(long, default_value_t = false)]
        allow_non_admin_sub_group_creation: bool,
        #[arg(long, default_value_t = true)]
        create_general_chat: bool,
    },
    /// Deactivate (delete) a community.
    Deactivate {
        #[arg(value_name = "COMMUNITY_JID")]
        community_jid: String,
    },
    /// Link subgroups to a community.
    LinkSubgroups {
        #[arg(value_name = "COMMUNITY_JID")]
        community_jid: String,
        #[arg(value_name = "SUBGROUP_JID", required = true)]
        subgroup_jids: Vec<String>,
    },
    /// Unlink subgroups from a community.
    UnlinkSubgroups {
        #[arg(value_name = "COMMUNITY_JID")]
        community_jid: String,
        #[arg(value_name = "SUBGROUP_JID", required = true)]
        subgroup_jids: Vec<String>,
        #[arg(long, default_value_t = false)]
        remove_orphan_members: bool,
    },
    /// List subgroups of a community.
    GetSubgroups {
        #[arg(value_name = "COMMUNITY_JID")]
        community_jid: String,
    },
    /// Get participant counts per subgroup.
    GetSubgroupParticipantCounts {
        #[arg(value_name = "COMMUNITY_JID")]
        community_jid: String,
    },
    /// Query one linked subgroup's metadata.
    QueryLinkedGroup {
        #[arg(value_name = "COMMUNITY_JID")]
        community_jid: String,
        #[arg(value_name = "SUBGROUP_JID")]
        subgroup_jid: String,
    },
    /// Join a linked subgroup via the parent.
    JoinSubgroup {
        #[arg(value_name = "COMMUNITY_JID")]
        community_jid: String,
        #[arg(value_name = "SUBGROUP_JID")]
        subgroup_jid: String,
    },
    /// Get participants across all linked groups.
    GetLinkedGroupsParticipants {
        #[arg(value_name = "COMMUNITY_JID")]
        community_jid: String,
    },
}

#[derive(Debug, Args)]
pub struct NewsletterCmd {
    #[command(subcommand)]
    pub action: NewsletterAction,
}

#[derive(Debug, Subcommand)]
pub enum NewsletterAction {
    /// List every newsletter (channel) this account is subscribed to.
    ListSubscribed,

    /// Fetch metadata for one newsletter by its JID.
    GetMetadata {
        #[arg(value_name = "NEWSLETTER_JID")]
        jid: String,
    },

    /// Create a new newsletter (channel).
    Create {
        #[arg(value_name = "NAME")]
        name: String,
        #[arg(long)]
        description: Option<String>,
    },

    /// Join (subscribe to) a newsletter by its JID.
    Join {
        #[arg(value_name = "NEWSLETTER_JID")]
        jid: String,
    },

    /// Leave (unsubscribe from) a newsletter by its JID.
    Leave {
        #[arg(value_name = "NEWSLETTER_JID")]
        jid: String,
    },

    /// Send a reaction emoji to a newsletter message.
    SendReaction {
        #[arg(value_name = "NEWSLETTER_JID")]
        jid: String,
        #[arg(value_name = "SERVER_ID")]
        server_id: u64,
        #[arg(value_name = "EMOJI")]
        reaction: String,
    },

    /// Edit a message in a newsletter (channel owner only).
    EditMessage {
        #[arg(value_name = "NEWSLETTER_JID")]
        jid: String,
        #[arg(value_name = "MESSAGE_ID")]
        message_id: String,
        #[arg(value_name = "NEW_TEXT")]
        new_text: String,
    },

    /// Revoke (delete) a message in a newsletter (channel owner only).
    RevokeMessage {
        #[arg(value_name = "NEWSLETTER_JID")]
        jid: String,
        #[arg(value_name = "MESSAGE_ID")]
        message_id: String,
    },

    /// Update the name/description of a newsletter you own.
    Update {
        #[arg(value_name = "NEWSLETTER_JID")]
        jid: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },

    /// Mute / unmute a channel you subscribe to.
    SetFollowerMute {
        #[arg(value_name = "NEWSLETTER_JID")]
        jid: String,
        #[arg(long)]
        muted: bool,
    },

    /// Mute / unmute notifications on a channel you own.
    SetAdminMute {
        #[arg(value_name = "NEWSLETTER_JID")]
        jid: String,
        #[arg(long)]
        muted: bool,
    },

    /// Look up a channel by its invite-code (e.g. `ABCD1234`).
    GetMetadataByInvite {
        #[arg(value_name = "INVITE")]
        invite: String,
    },

    /// Subscribe to live-update push notifications for a channel.
    /// Server returns the duration (in seconds) the subscription is active.
    SubscribeLiveUpdates {
        #[arg(value_name = "NEWSLETTER_JID")]
        jid: String,
    },

    /// Fetch recent messages of a channel (history backfill).
    GetMessages {
        #[arg(value_name = "NEWSLETTER_JID")]
        jid: String,
        #[arg(long, default_value = "20")]
        count: u32,
        #[arg(long)]
        before: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run as a long-lived daemon (the default for systemd).
    Daemon,
    /// Run as an MCP server over stdio (JSON-RPC 2.0).
    Mcp,
    /// Print daemon version info.
    Version,
    /// Print daemon status (boot/connected/session-lost/etc).
    Status,
    /// Print daemon health.
    Health,
    /// Send a message (text/image/video/audio/voice/sticker/reaction/poll/contact/location/delete).
    Send(SendArgs),
    /// Group operations.
    Groups(GroupsCmd),
    /// Message operations.
    Messages(MessagesCmd),
    /// Chat operations (list/info/pin/unpin/mute/archive/delete/typing).
    Chats(ChatsCmd),
    /// Envelope operations (encode/decode/send/send-native).
    Envelope(EnvelopeCmd),
    /// Media operations (info).
    Media(MediaCmd),
    /// Platform capabilities (payload sizes, media caps, flags).
    Capabilities,
    /// Domain operations (compute-hash).
    Domain(DomainCmd),
    /// Rule operations (Phase 1: read-only).
    Rules(RulesCmd),
    /// Trigger operations (Phase 1: read-only).
    Triggers(TriggersCmd),
    /// Event operations (Phase 1: list/show only).
    Events(EventsCmd),
    /// Force a reconnect of the underlying WebSocket.
    Reconnect,
    /// Gracefully shut down the daemon.
    Shutdown,
    /// Onboarding passthrough (delegates to octo-whatsapp-onboard-core).
    Onboard(OnboardCmd),
    /// Phase 1 (query layer): full-text + semantic search over
    /// the persisted `messages` + `events` views. Requires the
    /// `query` cargo feature on the daemon binary.
    #[cfg(feature = "query")]
    Query(QueryCmd),
    /// Contact lookup / business profile / save_contact / block / unblock
    /// (Phase 7.J). Mirrors the `contacts.*` and `contact.*` RPCs.
    Contacts(ContactsCmd),
    /// Identity reads (Phase 7.J / Tier 6.4). Mirrors the `identity.*`
    /// RPCs.
    Identity(IdentityCmd),
    /// Client session discovery (Phase 3).
    Clients(ClientsCmd),
    /// Phase 6.1: multi-account management.
    Accounts(AccountsCmd),
    /// Daemon method discovery (Phase 3). `methods list|help METHOD`.
    Methods(MethodsCmd),
    /// Security token operations (Phase 5 Part A).
    Tokens(TokenCmd),
    /// Audit log operations (Phase 5 Part E; Phase 4 RPC surface).
    Audit(AuditCmd),
    /// Action dispatcher operations (Phase 5 Part E; Phase 4 RPC surface).
    Actions(ActionsCmd),
    /// Community RPCs (Tier 7.G).
    Community(CommunityCmd),
    /// Newsletter (channel) RPCs (Tier 7.E+).
    Newsletter(NewsletterCmd),
}

#[derive(Debug, Args)]
pub struct SendArgs {
    #[command(subcommand)]
    pub kind: SendKind,
}

#[derive(Debug, Subcommand)]
pub enum SendKind {
    /// Send a text payload to a peer.
    Text {
        /// Peer phone number (E.164), JID, or `name` from contacts.
        peer: String,
        /// Text payload.
        #[arg(long)]
        text: String,
    },
    /// Send an image with optional caption.
    Image {
        peer: String,
        /// Path to the image file on disk.
        file: PathBuf,
        /// Optional caption.
        #[arg(long)]
        caption: Option<String>,
    },
    /// Send a video with optional caption.
    Video {
        peer: String,
        file: PathBuf,
        #[arg(long)]
        caption: Option<String>,
    },
    /// Send an audio file (non-voice).
    Audio { peer: String, file: PathBuf },
    /// Send a voice-note (PTT) audio file.
    Voice { peer: String, file: PathBuf },
    /// Send a sticker (WEBP image).
    Sticker { peer: String, file: PathBuf },
    /// React to a message with an emoji.
    Reaction {
        peer: String,
        msg_id: String,
        #[arg(long)]
        emoji: String,
    },
    /// Send a poll with question + options.
    Poll {
        peer: String,
        #[arg(long)]
        question: String,
        #[arg(long, value_delimiter = ',')]
        options: Vec<String>,
        #[arg(long)]
        multi: bool,
    },
    /// Send a vCard contact.
    Contact {
        peer: String,
        /// Path to a vCard (.vcf) file.
        vcard: PathBuf,
    },
    /// Send a location pin.
    Location {
        peer: String,
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lon: f64,
        #[arg(long)]
        name: String,
    },
    /// Delete (revoke) a previously sent message.
    Delete {
        peer: String,
        msg_id: String,
        #[arg(long)]
        msg_timestamp: i64,
    },
}

#[derive(Debug, Args)]
pub struct GroupsCmd {
    #[command(subcommand)]
    pub action: GroupsAction,
}

#[derive(Debug, Subcommand)]
pub enum GroupsAction {
    /// Create a new group.
    Create {
        #[arg(long)]
        subject: String,
        #[arg(long, value_delimiter = ',')]
        members: Vec<String>,
    },
    /// List groups the daemon belongs to.
    List,
    /// Show info about a single group.
    Info { jid: String },
    /// Leave a group.
    Leave { jid: String },
    /// Destroy (delete) a group. Irreversible server-side.
    Destroy { jid: String },
    /// Resolve an invite link/code to a group handle.
    ResolveInvite { code: String },
    /// Resolve LID → phone for every participant of a group you belong to.
    /// One `w:g2` IQ; the server populates `phone_number=` for every
    /// LID participant. Orthogonal to `contacts get-lid-pn-mappings`
    /// (usync-based, business-only).
    ParticipantsLidToPhone {
        /// Group JID (e.g. `120363411021224818@g.us`). Server returns
        /// `Forbidden` for non-members.
        #[arg(value_name = "GROUP_JID")]
        group_jid: String,
    },
    /// Add a single member to a group.
    AddMember {
        jid: String,
        #[arg(long)]
        member: String,
        #[arg(long, default_value_t = false)]
        is_admin: bool,
    },
    /// Add multiple members to a group (partial-success per element).
    AddMembers {
        jid: String,
        #[arg(long, value_delimiter = ',')]
        members: Vec<String>,
    },
    /// Remove a single member from a group.
    RemoveMember {
        jid: String,
        #[arg(long)]
        member: String,
    },
    /// Remove multiple members from a group (partial-success per element).
    RemoveMembers {
        jid: String,
        #[arg(long, value_delimiter = ',')]
        members: Vec<String>,
    },
    /// Promote a member to admin.
    Promote {
        jid: String,
        #[arg(long)]
        member: String,
    },
    /// Demote an admin to member.
    Demote {
        jid: String,
        #[arg(long)]
        member: String,
    },
    /// Ban a member. Pass `--duration-seconds` for a timed ban; omit for indefinite.
    Ban {
        jid: String,
        #[arg(long)]
        member: String,
        #[arg(long)]
        duration_seconds: Option<u64>,
    },
    /// Approve a pending join request.
    ApproveJoin {
        jid: String,
        #[arg(long)]
        member: String,
    },
    /// Rename the group subject.
    Rename {
        jid: String,
        #[arg(long)]
        subject: String,
    },
    /// Set the group description.
    SetDescription {
        jid: String,
        #[arg(long)]
        description: String,
    },
    /// Lock or unlock the group (admins-only messaging when locked).
    SetLocked {
        jid: String,
        #[arg(long)]
        locked: bool,
    },
    /// Transfer group ownership (irreversible).
    TransferOwnership {
        jid: String,
        #[arg(long)]
        member: String,
    },
    /// Lock or unlock announce-only mode (only admins can post when on).
    SetAnnounce {
        jid: String,
        /// True to enable announce-only, false to allow all members to post.
        #[arg(long)]
        announce: bool,
    },
    /// Set message expiry timer. Omit `--ttl-seconds` (pass an empty value)
    /// to disable; otherwise pass the lifetime in seconds (0 = disable).
    SetEphemeral {
        jid: String,
        /// Message lifetime in seconds. When omitted/zero the timer is disabled.
        #[arg(long)]
        ttl_seconds: Option<u32>,
    },
    /// Require admin approval before members can join.
    SetRequireApproval {
        jid: String,
        #[arg(long)]
        require: bool,
    },
    /// List groups the daemon belongs to plus pending join invites.
    ListWithInvites,
    /// Join a group via invite link or short code.
    JoinByInvite {
        /// Invite link (`https://chat.whatsapp.com/…`) or short code (e.g. `CXYZ…`).
        code: String,
    },
    /// Join a known group by JID.
    JoinById {
        /// Group JID to join (group must allow on-demand joins).
        jid: String,
    },
    /// Get the current invite link for a group. Pass `--reset` to rotate the link (revokes the prior URL).
    GetInviteLink {
        jid: String,
        /// Rotate the link server-side; the returned URL supersedes any prior invite.
        #[arg(long, default_value_t = false)]
        reset: bool,
    },
    /// Set or clear a per-member label (e.g. nickname) within a group. Pass an empty `--label` to clear.
    UpdateMemberLabel {
        jid: String,
        #[arg(long)]
        label: String,
    },
    /// Fetch profile pictures for one or more groups. Pass `--preview` to request the small preview variant.
    GetProfilePictures {
        #[arg(long, value_delimiter = ',')]
        jids: Vec<String>,
        #[arg(long, default_value_t = false)]
        preview: bool,
    },
    /// Set the group icon. The `--file` argument must point to a JPEG/PNG.
    SetProfilePicture {
        jid: String,
        #[arg(long)]
        file: PathBuf,
    },
    /// Remove the group icon.
    RemoveProfilePicture { jid: String },
}

#[derive(Debug, Args)]
pub struct MessagesCmd {
    #[command(subcommand)]
    pub action: MessagesAction,
}

#[derive(Debug, Subcommand)]
pub enum MessagesAction {
    /// List recent messages, optionally filtered by peer.
    List {
        #[arg(long)]
        peer: Option<String>,
        #[arg(long)]
        since: Option<i64>,
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Get a single message by id.
    Get { msg_id: String },
    /// Full-text search across message history.
    Search {
        query: String,
        #[arg(long)]
        peer: Option<String>,
    },
    /// Edit a previously sent text message (within the platform edit window).
    Edit {
        peer: String,
        msg_id: String,
        #[arg(long)]
        msg_timestamp: i64,
        #[arg(long)]
        new_text: String,
    },
    /// Mark messages up to a given id as read.
    MarkRead {
        peer: String,
        #[arg(long)]
        up_to: String,
    },
    /// Download a media-ref token to a local path.
    Download {
        media_ref_token: String,
        out: PathBuf,
    },
}

/// Top-level Chats subcommand tree (Task 55). Mirrors the `chats.*` RPC
/// surface: list/info/pin/unpin/mute/archive/delete/typing.
#[derive(Debug, Args)]
pub struct ChatsCmd {
    #[command(subcommand)]
    pub action: ChatsAction,
}

#[derive(Debug, Subcommand)]
pub enum ChatsAction {
    /// List known chats, optionally filtered by kind and limited.
    List {
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Show info about a single chat by JID.
    Info { jid: String },
    /// Pin a chat to the top of the list.
    Pin { jid: String },
    /// Unpin a previously pinned chat.
    Unpin { jid: String },
    /// Mute a chat until the given epoch-seconds timestamp.
    Mute {
        jid: String,
        #[arg(long)]
        until_epoch_secs: i64,
    },
    /// Archive a chat (hide from the default list).
    Archive { jid: String },
    /// Delete a chat and its history locally.
    Delete { jid: String },
    /// Set or clear the typing indicator on a chat.
    Typing {
        jid: String,
        #[arg(long)]
        on: bool,
    },
}

/// Top-level Envelope subcommand tree (Task 56). Mirrors the `envelope.*`
/// RPC surface: encode/decode/send/send-native.
#[derive(Debug, Args)]
pub struct EnvelopeCmd {
    #[command(subcommand)]
    pub action: EnvelopeAction,
}

#[derive(Debug, Subcommand)]
pub enum EnvelopeAction {
    /// Wrap raw bytes in a DOT/1 envelope. Reads from `--file` if given,
    /// otherwise from stdin.
    Encode {
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Decode a DOT/1 envelope from stdin (prints payload).
    Decode,
    /// Send a DOT/1 envelope file as a message.
    Send { peer: String, file: PathBuf },
    /// Send a DOT/1 envelope via the native transport.
    SendNative { peer: String, file: PathBuf },
}

/// Top-level Media subcommand tree (Task 56). Mirrors `media.info`.
#[derive(Debug, Args)]
pub struct MediaCmd {
    #[command(subcommand)]
    pub action: MediaAction,
}

#[derive(Debug, Subcommand)]
pub enum MediaAction {
    /// Return metadata for a media-ref token.
    Info { media_ref_token: String },
}

/// Top-level Domain subcommand tree (Task 56). Mirrors `domain.compute-hash`.
#[derive(Debug, Args)]
pub struct DomainCmd {
    #[command(subcommand)]
    pub action: DomainAction,
}

#[derive(Debug, Subcommand)]
pub enum DomainAction {
    /// Compute the deterministic domain id for a group JID.
    ComputeHash { group_jid: String },
}

#[derive(Debug, Args)]
pub struct RulesCmd {
    #[command(subcommand)]
    pub action: RulesAction,
}

#[derive(Debug, Subcommand)]
pub enum RulesAction {
    /// List all rules.
    List,
    /// Show a single rule by id.
    Get { id: String },
    /// Create a new rule (Phase 4). Pass a JSON body via --body.
    Create {
        /// JSON body for the new rule (e.g. `{"id":"r1","predicate":{...}}`).
        body: String,
    },
    /// Replace a rule (full etag-guarded update). Pass new body + etag.
    Update {
        id: String,
        /// Current etag (use `rules get` to read).
        etag: String,
        /// JSON body with new fields.
        body: String,
    },
    /// Apply a subset patch to a rule (etag-guarded).
    Patch {
        id: String,
        etag: String,
        /// JSON body with the subset of fields to change.
        body: String,
    },
    /// Delete a rule (etag-guarded).
    Delete { id: String, etag: String },
    /// Enable a rule (no etag required).
    Enable { id: String },
    /// Disable a rule (no etag required).
    Disable { id: String },
    /// Approve a Draft rule, transitioning it to Approved.
    Approve { id: String },
    /// Re-read rules.toml from disk.
    Reload,
    /// Force a sync of debounced disk writes.
    Flush,
    /// Evaluate an event against the ruleset (no-execute dry-run).
    Test {
        /// JSON body containing the inbound event under `event`.
        event_json: String,
    },
}

#[derive(Debug, Args)]
pub struct TriggersCmd {
    #[command(subcommand)]
    pub action: TriggersAction,
}

#[derive(Debug, Subcommand)]
pub enum TriggersAction {
    /// List all triggers.
    List,
    /// Show a single trigger by id.
    Get { id: String },
    /// Create a new trigger (Phase 4). Pass a JSON body via --body.
    Create {
        /// JSON body for the new trigger.
        body: String,
    },
    /// Replace a trigger (etag-guarded update).
    Update {
        id: String,
        etag: String,
        /// JSON body with new fields.
        body: String,
    },
    /// Delete a trigger (etag-guarded).
    Delete { id: String, etag: String },
    /// Invoke a trigger and return the RunRecord.
    Run {
        id: String,
        /// Optional JSON payload to wrap in an inbound event.
        payload_json: Option<String>,
    },
}

/// Phase 5 Part E: audit log operations (Phase 4 RPC surface).
#[derive(Debug, Args)]
pub struct AuditCmd {
    #[command(subcommand)]
    pub action: AuditAction,
}

#[derive(Debug, Subcommand)]
pub enum AuditAction {
    /// Tail audit log entries since a given sequence number.
    Tail {
        /// Lower-bound sequence number (exclusive).
        #[arg(long)]
        since_seq: Option<u64>,
        /// Max entries to return (1..=10000).
        #[arg(long)]
        limit: Option<u64>,
    },
    /// Walk the in-memory hash chain and verify each entry.
    Verify,
}

/// Phase 5 Part E: dispatch an escalation (Phase 4 RPC surface).
#[derive(Debug, Args)]
pub struct ActionsCmd {
    #[command(subcommand)]
    pub action: ActionsAction,
}

/// Phase 1 (query layer): CLI mirror for `daemon.search`,
/// `messages.context`, `events.find` RPCs.
#[cfg(feature = "query")]
#[derive(Debug, Args)]
pub struct QueryCmd {
    #[command(subcommand)]
    pub action: QueryAction,
}

#[cfg(feature = "query")]
#[derive(Debug, Subcommand)]
pub enum QueryAction {
    /// Full-text search across persisted messages.
    Search {
        query: String,
        #[arg(long)]
        peer: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        since_ts_unix_ms: Option<i64>,
        #[arg(long)]
        until_ts_unix_ms: Option<i64>,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Surrounding messages around an event_id.
    Context {
        event_id: i64,
        #[arg(long)]
        before: Option<usize>,
        #[arg(long)]
        after: Option<usize>,
    },
    /// Filter `events` rows by kind / variant / peer / ts.
    Find {
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        variant: Option<String>,
        #[arg(long)]
        peer: Option<String>,
        #[arg(long)]
        since_ts_unix_ms: Option<i64>,
        #[arg(long)]
        until_ts_unix_ms: Option<i64>,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Dynamic DDL/DML against the daemon's embedded SQL store.
    /// Accepts INSERT/UPDATE/DELETE/CREATE/DROP/ALTER only — for
    /// reads use `query` or `tables`.
    Execute {
        /// Single SQL statement. Anything multi-statement or non-DDL/DML
        /// is rejected by the daemon before reaching stoolap.
        sql: String,
    },
    /// Read-only SQL: SELECT/WITH/SHOW/EXPLAIN. Returns rows as JSON.
    Query {
        /// Single SQL statement.
        sql: String,
        /// Optional client-side cap (smaller than the server's 10000).
        #[arg(long)]
        limit: Option<usize>,
    },
    /// List existing tables (introspection; shortcut for SHOW TABLES).
    Tables,
}

/// Phase 7.J: CLI mirror for the 7 contact-handlers. Mirrors the
/// `contacts.*` and `contact.*` MCP tool descriptors.
#[derive(Debug, Args)]
pub struct ContactsCmd {
    #[command(subcommand)]
    pub action: ContactsAction,
}

#[derive(Debug, Subcommand)]
pub enum ContactsAction {
    /// Check whether a peer JID is a registered WhatsApp user.
    IsOnWhatsApp {
        /// Peer JID (canonical `<digits>@s.whatsapp.net`) or E.164.
        peer: String,
    },
    /// Fetch rich user info (status, picture id, business, devices, lid).
    GetUserInfo {
        /// Peer JID or E.164.
        peer: String,
    },
    /// Fetch a peer's business profile (description, address, hours).
    GetBusinessProfile {
        /// Peer JID (must be a phone-number JID).
        jid: String,
    },
    /// Batch-resolve phone-number JIDs to their corresponding LIDs via the
    /// WA server's `usync` IQ with the `<lid>` subprotocol.
    GetPnLidMappings {
        /// List of phone-number JIDs (`5521995544743@s.whatsapp.net`) or
        /// bare E.164 numbers (`5521995544743`). Max 100 per call.
        #[arg(required = true, num_args = 1..=100)]
        phones: Vec<String>,
    },
    /// Batch-resolve LID JIDs to their corresponding phone numbers.
    /// Inverse of `get-pn-lid-mappings`. Uses wacore's
    /// `Contacts::is_on_whatsapp` with LID-form JIDs; the server returns
    /// `<user jid="NN@lid" pn_jid="MM@s.whatsapp.net">`.
    GetLidPnMappings {
        /// List of LID user parts (`108074580897808`) or full
        /// `@lid` JIDs. Max 100 per call.
        #[arg(required = true, num_args = 1..=100)]
        lids: Vec<String>,
    },
    /// Fetch the profile-picture URL for a peer.
    GetProfilePicture {
        /// Peer JID or E.164.
        peer: String,
        /// `true` (default) requests the thumbnail; `false` for the full image.
        #[arg(long)]
        preview: Option<bool>,
    },
    /// Save or rename a contact in the local address book.
    SaveContact {
        /// Peer JID or E.164 (must be a phone-number JID).
        peer: String,
        /// Full name to record.
        full_name: String,
    },
    /// Add a peer to the local blocklist.
    Block {
        /// Peer JID or E.164.
        peer: String,
    },
    /// Remove a peer from the local blocklist. Reverses `block`.
    Unblock {
        /// Peer JID or E.164.
        peer: String,
    },
}

/// Phase 7.J / Tier 6.4: CLI mirror for the 3 identity readers.
#[derive(Debug, Args)]
pub struct IdentityCmd {
    #[command(subcommand)]
    pub action: IdentityAction,
}

#[derive(Debug, Subcommand)]
pub enum IdentityAction {
    /// Return this device's PN (phone-number) JID.
    GetPn,
    /// Return this device's LID (local-identifier) JID.
    GetLid,
    /// Return true if the device has completed the LID migration.
    IsLidMigrated,
}

#[derive(Debug, Subcommand)]
pub enum ActionsAction {
    /// Escalate to a target (e.g. oncall) with a free-text reason.
    Escalate {
        /// Escalation target identifier (e.g. `oncall`, `sre`).
        target: String,
        /// Reason / context for the escalation.
        reason: String,
    },
}

#[derive(Debug, Args)]
pub struct EventsCmd {
    #[command(subcommand)]
    pub action: EventsAction,
}

#[derive(Debug, Subcommand)]
pub enum EventsAction {
    /// List recent events (most recent first).
    List {
        /// Maximum number of events to return (1..=10000, default 100).
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Show a single event by id.
    Show {
        /// Event id (1-based, returned by `events.list`).
        id: String,
    },
    /// Replay events since a given id (Loss recovery).
    Replay {
        /// Start id (exclusive lower bound).
        #[arg(long)]
        since_id: Option<u64>,
        /// Maximum number of events to return (1..=10000, default 100).
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Tail the event stream (returns recent buffer snapshot).
    Tail {
        /// Maximum number of events to return (1..=10000, default 100).
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
}

#[derive(Debug, Args)]
pub struct OnboardCmd {
    #[command(subcommand)]
    pub action: OnboardAction,
}

#[derive(Debug, Args)]
pub struct ClientsCmd {
    #[command(subcommand)]
    pub action: ClientsAction,
}

#[derive(Debug, Subcommand)]
pub enum ClientsAction {
    /// List active MCP client sessions.
    List,
}

#[derive(Debug, Args)]
pub struct MethodsCmd {
    #[command(subcommand)]
    pub action: MethodsAction,
}

#[derive(Debug, Subcommand)]
pub enum MethodsAction {
    /// Print every method name.
    List,
    /// Print help for a single method.
    Show {
        /// Method name (e.g. `send.text`).
        method: String,
    },
}

/// Phase 6.1: multi-account management. Mirrors `daemon.accounts.list`,
/// `daemon.accounts.use`, and `daemon.accounts.info`.
#[derive(Debug, Args)]
pub struct AccountsCmd {
    #[command(subcommand)]
    pub action: AccountsAction,
}

#[derive(Debug, Subcommand)]
pub enum AccountsAction {
    /// List all linked WhatsApp accounts.
    List,
    /// Set the active account (writes the `active` symlink).
    Use {
        /// Account ID (e.g. E.164 phone without leading "+").
        account_id: String,
    },
    /// Show details for one account.
    Info {
        /// Account ID to look up.
        account_id: String,
    },
}

/// Phase 5 Part A: bearer-token operations. Mirrors the
/// `security.rotate_token`, `security.revoke_all_tokens`, and
/// `security.list_tokens` RPC methods.
#[derive(Debug, Args)]
pub struct TokenCmd {
    #[command(subcommand)]
    pub action: TokenAction,
}

#[derive(Debug, Subcommand)]
pub enum TokenAction {
    /// Rotate the active bearer token. The OLD token continues to
    /// verify until the grace window expires.
    Rotate {
        /// Existing token_id to rotate from. (Required.)
        old_token_id: String,
        /// New 256-bit (or larger) hex secret. (Required.)
        new_secret_hex: String,
        /// Grace window in milliseconds. Clamped to 1000..=300000.
        /// Default 60000.
        #[arg(long, default_value_t = 60_000)]
        grace_ms: i64,
        /// Human-readable label for the rotated token.
        #[arg(long, default_value = "rotated")]
        label: String,
    },
    /// Revoke every active token (incident response). Persists an
    /// empty grace file.
    RevokeAll,
    /// List active + grace tokens.
    List,
}

#[derive(Debug, Subcommand)]
pub enum OnboardAction {
    /// Print QR-code link for pairing (max age in seconds).
    QrLink {
        #[arg(long, default_value_t = 120)]
        timeout: u64,
    },
    /// Pair using a phone number link.
    PairLink { phone: String },
    /// Show the active session identity.
    Whoami,
    /// Session management.
    Session {
        #[command(subcommand)]
        action: SessionCmd,
    },
}

#[derive(Debug, Subcommand)]
pub enum SessionCmd {
    /// List known sessions.
    List,
    /// Verify a session's stored credentials.
    Verify { name: String },
    /// Remove a session's stored credentials.
    Remove { name: String },
}

/// Resolve the daemon socket path: `--socket` if set, otherwise derive from
/// `$XDG_RUNTIME_DIR/octo-whatsapp-{name}.sock` (falling back to
/// `/tmp/octo-whatsapp-{name}.sock`).
///
/// This MUST match the daemon's `WhatsAppRuntimeConfig::socket_path()` for the
/// default `--name = "default"` case so the CLI finds the daemon without flags.
pub fn resolve_socket_path(cli: &Cli) -> PathBuf {
    if let Some(s) = &cli.socket {
        return s.clone();
    }
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(dir).join(format!("octo-whatsapp-{}.sock", cli.name))
}

/// Test seam: derive the default socket path given an explicit env value.
/// Public so unit tests can verify both branches without mutating process env
/// (the crate denies `unsafe_code`, so `std::env::set_var` is not an option).
pub fn resolve_socket_path_with_env(name: &str, xdg_runtime_dir: Option<&str>) -> PathBuf {
    let dir = xdg_runtime_dir.unwrap_or("/tmp");
    PathBuf::from(dir).join(format!("octo-whatsapp-{name}.sock"))
}

/// Synchronous CLI→RPC client. Sends one newline-delimited JSON-RPC 2.0
/// request and returns the `result` field. Errors propagate as `anyhow::Error`
/// carrying the daemon's error message + data.
pub struct RpcClient {
    socket_path: PathBuf,
}

impl std::fmt::Debug for RpcClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcClient")
            .field("socket_path", &self.socket_path)
            .finish()
    }
}

impl RpcClient {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    /// Send `method` with `params` and return the `result` field. Returns
    /// `Err` with the daemon's error message (and JSON data) on RPC failure,
    /// or a connection error if the daemon socket is not reachable.
    pub fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixStream;

        let mut s = UnixStream::connect(&self.socket_path).map_err(|e| {
            anyhow::anyhow!(
                "failed to connect to daemon socket at {}: {e}; is the daemon running?",
                self.socket_path.display()
            )
        })?;
        let req = serde_json::json!({"id": 1, "method": method, "params": params});
        let mut line = serde_json::to_string(&req)?;
        line.push('\n');
        s.write_all(line.as_bytes())?;
        // Server keeps connections open for further requests, so read exactly
        // one line via BufReader::read_line instead of read_to_string (which
        // would block until EOF).
        let mut reader = BufReader::new(s);
        let mut buf = String::new();
        reader.read_line(&mut buf)?;
        let resp: serde_json::Value = serde_json::from_str(buf.trim())
            .map_err(|e| anyhow::anyhow!("malformed RPC response from daemon: {e}"))?;
        if let Some(err) = resp.get("error") {
            let code = err.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
            let message = err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("(no message)");
            let data = err.get("data").cloned().unwrap_or(serde_json::Value::Null);
            anyhow::bail!(
                "RPC {} error (code {}): {} [data={}]",
                method,
                code,
                message,
                serde_json::to_string(&data).unwrap_or_default()
            );
        }
        Ok(resp
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }
}

/// Pretty-print an RPC result to stdout. When `as_json` is set, print
/// `serde_json::to_string_pretty`. Otherwise, for scalars, print the bare
/// value; for objects/arrays, fall back to pretty JSON so operators can
/// still read it.
pub fn print_result(as_json: bool, value: &serde_json::Value) -> anyhow::Result<()> {
    if as_json {
        println!("{}", serde_json::to_string_pretty(value)?);
        return Ok(());
    }
    match value {
        serde_json::Value::Null => println!("(null)"),
        serde_json::Value::Bool(b) => println!("{b}"),
        serde_json::Value::Number(n) => println!("{n}"),
        serde_json::Value::String(s) => println!("{s}"),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            println!("{}", serde_json::to_string_pretty(value)?);
        }
    }
    Ok(())
}

/// Wire the read-only `version` / `status` / `health` commands (Tasks 41).
pub fn dispatch_simple(cli: &Cli, method: &str) -> anyhow::Result<()> {
    let result = RpcClient::new(resolve_socket_path(cli)).call(method, serde_json::Value::Null)?;
    print_result(cli.json, &result)
}

/// Wire `send *` subcommands (Task 42 + Task 54). The Phase 2 surface
/// includes image/video/audio/voice/sticker/reaction/poll/contact/location/
/// delete alongside the original `text` variant.
pub fn dispatch_send(cli: &Cli, args: &SendArgs) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &args.kind {
        SendKind::Text { peer, text } => {
            ("send.text", serde_json::json!({"peer": peer, "text": text}))
        }
        SendKind::Image {
            peer,
            file,
            caption,
        } => {
            let mut p = serde_json::Map::new();
            p.insert("peer".into(), serde_json::Value::String(peer.clone()));
            p.insert(
                "file".into(),
                serde_json::Value::String(file.to_string_lossy().into_owned()),
            );
            if let Some(cap) = caption {
                p.insert("caption".into(), serde_json::Value::String(cap.clone()));
            }
            ("send.image", serde_json::Value::Object(p))
        }
        SendKind::Video {
            peer,
            file,
            caption,
        } => {
            let mut p = serde_json::Map::new();
            p.insert("peer".into(), serde_json::Value::String(peer.clone()));
            p.insert(
                "file".into(),
                serde_json::Value::String(file.to_string_lossy().into_owned()),
            );
            if let Some(cap) = caption {
                p.insert("caption".into(), serde_json::Value::String(cap.clone()));
            }
            ("send.video", serde_json::Value::Object(p))
        }
        SendKind::Audio { peer, file } => (
            "send.audio",
            serde_json::json!({"peer": peer, "file": file.to_string_lossy()}),
        ),
        SendKind::Voice { peer, file } => (
            "send.voice",
            serde_json::json!({"peer": peer, "file": file.to_string_lossy()}),
        ),
        SendKind::Sticker { peer, file } => (
            "send.sticker",
            serde_json::json!({"peer": peer, "file": file.to_string_lossy()}),
        ),
        SendKind::Reaction {
            peer,
            msg_id,
            emoji,
        } => (
            "send.reaction",
            serde_json::json!({"peer": peer, "msg_id": msg_id, "emoji": emoji}),
        ),
        SendKind::Poll {
            peer,
            question,
            options,
            multi,
        } => (
            "send.poll",
            serde_json::json!({
                "peer": peer,
                "question": question,
                "options": options,
                "multi": multi,
            }),
        ),
        SendKind::Contact { peer, vcard } => (
            "send.contact",
            serde_json::json!({"peer": peer, "vcard": vcard.to_string_lossy()}),
        ),
        SendKind::Location {
            peer,
            lat,
            lon,
            name,
        } => (
            "send.location",
            serde_json::json!({
                "peer": peer,
                "lat": lat,
                "lon": lon,
                "name": name,
            }),
        ),
        SendKind::Delete {
            peer,
            msg_id,
            msg_timestamp,
        } => (
            "send.delete",
            serde_json::json!({
                "peer": peer,
                "msg_id": msg_id,
                "msg_timestamp": msg_timestamp,
            }),
        ),
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

pub fn dispatch_groups(cli: &Cli, cmd: &GroupsCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        GroupsAction::Create { subject, members } => {
            let member_specs: Vec<serde_json::Value> = members
                .iter()
                .map(|h| serde_json::json!({"handle": h, "is_admin": false}))
                .collect();
            (
                "groups.create",
                serde_json::json!({"subject": subject, "members": member_specs}),
            )
        }
        GroupsAction::List => ("groups.list", serde_json::Value::Null),
        GroupsAction::Info { jid } => ("groups.info", serde_json::json!({"jid": jid})),
        GroupsAction::Leave { jid } => ("groups.leave", serde_json::json!({"jid": jid})),
        GroupsAction::Destroy { jid } => ("groups.destroy", serde_json::json!({"jid": jid})),
        GroupsAction::ResolveInvite { code } => {
            ("groups.resolve_invite", serde_json::json!({"code": code}))
        }
        GroupsAction::ParticipantsLidToPhone { group_jid } => (
            "groups.participants_lid_to_phone",
            serde_json::json!({ "group_jid": group_jid }),
        ),
        GroupsAction::AddMember {
            jid,
            member,
            is_admin,
        } => (
            "groups.add_member",
            serde_json::json!({"jid": jid, "member": member, "is_admin": is_admin}),
        ),
        GroupsAction::AddMembers { jid, members } => {
            let member_specs: Vec<serde_json::Value> = members
                .iter()
                .map(|h| serde_json::json!({"handle": h, "is_admin": false}))
                .collect();
            (
                "groups.add_members",
                serde_json::json!({"jid": jid, "members": member_specs}),
            )
        }
        GroupsAction::RemoveMember { jid, member } => (
            "groups.remove_member",
            serde_json::json!({"jid": jid, "member": member}),
        ),
        GroupsAction::RemoveMembers { jid, members } => (
            "groups.remove_members",
            serde_json::json!({"jid": jid, "members": members}),
        ),
        GroupsAction::Promote { jid, member } => (
            "groups.promote",
            serde_json::json!({"jid": jid, "member": member}),
        ),
        GroupsAction::Demote { jid, member } => (
            "groups.demote",
            serde_json::json!({"jid": jid, "member": member}),
        ),
        GroupsAction::Ban {
            jid,
            member,
            duration_seconds,
        } => (
            "groups.ban",
            serde_json::json!({"jid": jid, "member": member, "duration_seconds": duration_seconds}),
        ),
        GroupsAction::ApproveJoin { jid, member } => (
            "groups.approve_join",
            serde_json::json!({"jid": jid, "member": member}),
        ),
        GroupsAction::Rename { jid, subject } => (
            "groups.rename",
            serde_json::json!({"jid": jid, "subject": subject}),
        ),
        GroupsAction::SetDescription { jid, description } => (
            "groups.set_description",
            serde_json::json!({"jid": jid, "description": description}),
        ),
        GroupsAction::SetLocked { jid, locked } => (
            "groups.set_locked",
            serde_json::json!({"jid": jid, "locked": locked}),
        ),
        GroupsAction::TransferOwnership { jid, member } => (
            "groups.transfer_ownership",
            serde_json::json!({"jid": jid, "member": member}),
        ),
        GroupsAction::SetAnnounce { jid, announce } => (
            "groups.set_announce",
            serde_json::json!({"jid": jid, "announce": announce}),
        ),
        GroupsAction::SetEphemeral { jid, ttl_seconds } => (
            "groups.set_ephemeral",
            serde_json::json!({"jid": jid, "ttl_seconds": ttl_seconds}),
        ),
        GroupsAction::SetRequireApproval { jid, require } => (
            "groups.set_require_approval",
            serde_json::json!({"jid": jid, "require": require}),
        ),
        GroupsAction::ListWithInvites => ("groups.list_with_invites", serde_json::json!({})),
        GroupsAction::JoinByInvite { code } => {
            ("groups.join_by_invite", serde_json::json!({"code": code}))
        }
        GroupsAction::JoinById { jid } => ("groups.join_by_id", serde_json::json!({"jid": jid})),
        GroupsAction::GetInviteLink { jid, reset } => (
            "groups.get_invite_link",
            serde_json::json!({"jid": jid, "reset": reset}),
        ),
        GroupsAction::UpdateMemberLabel { jid, label } => (
            "groups.update_member_label",
            serde_json::json!({"jid": jid, "label": label}),
        ),
        GroupsAction::GetProfilePictures { jids, preview } => (
            "groups.get_profile_pictures",
            serde_json::json!({"jids": jids, "preview": preview}),
        ),
        GroupsAction::SetProfilePicture { jid, file } => {
            // The IPC handler takes base64-encoded image data
            // (`image_data_b64`); the CLI takes a file path and
            // base64-encodes the bytes here so operators don't have
            // to do it manually.
            let bytes =
                std::fs::read(file).map_err(|e| anyhow::anyhow!("read {}: {e}", file.display()))?;
            let b64 = B64.encode(&bytes);
            (
                "groups.set_profile_picture",
                serde_json::json!({"jid": jid, "image_data_b64": b64}),
            )
        }
        GroupsAction::RemoveProfilePicture { jid } => (
            "groups.remove_profile_picture",
            serde_json::json!({"jid": jid}),
        ),
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Wire `messages *` subcommands (Task 44 + Task 55). Phase 2 extends
/// `messages list` with optional `--since` and adds `get`/`search`/`edit`/
/// `mark_read`/`download`.
pub fn dispatch_messages(cli: &Cli, cmd: &MessagesCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        MessagesAction::List { peer, since, limit } => {
            let mut p = serde_json::Map::new();
            if let Some(peer) = peer {
                p.insert("peer".into(), serde_json::Value::String(peer.clone()));
            }
            if let Some(since) = since {
                p.insert("since".into(), serde_json::Value::Number((*since).into()));
            }
            if let Some(limit) = limit {
                p.insert("limit".into(), serde_json::Value::Number((*limit).into()));
            }
            ("messages.list", serde_json::Value::Object(p))
        }
        MessagesAction::Get { msg_id } => ("messages.get", serde_json::json!({"msg_id": msg_id})),
        MessagesAction::Search { query, peer } => {
            let mut p = serde_json::Map::new();
            p.insert("query".into(), serde_json::Value::String(query.clone()));
            if let Some(peer) = peer {
                p.insert("peer".into(), serde_json::Value::String(peer.clone()));
            }
            ("messages.search", serde_json::Value::Object(p))
        }
        MessagesAction::Edit {
            peer,
            msg_id,
            msg_timestamp,
            new_text,
        } => (
            "messages.edit",
            serde_json::json!({
                "peer": peer,
                "msg_id": msg_id,
                "msg_timestamp": msg_timestamp,
                "new_text": new_text,
            }),
        ),
        MessagesAction::MarkRead { peer, up_to } => (
            "messages.mark_read",
            serde_json::json!({"peer": peer, "up_to": up_to}),
        ),
        MessagesAction::Download {
            media_ref_token,
            out,
        } => (
            "messages.download",
            serde_json::json!({
                "media_ref_token": media_ref_token,
                "out": out.to_string_lossy(),
            }),
        ),
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Wire `chats *` subcommands (Task 55). Mirrors the `chats.*` RPC surface:
/// list/info/pin/unpin/mute/archive/delete/typing.
pub fn dispatch_chats(cli: &Cli, cmd: &ChatsCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        ChatsAction::List { kind, limit } => {
            let mut p = serde_json::Map::new();
            if let Some(k) = kind {
                p.insert("kind".into(), serde_json::Value::String(k.clone()));
            }
            if let Some(limit) = limit {
                p.insert("limit".into(), serde_json::Value::Number((*limit).into()));
            }
            ("chats.list", serde_json::Value::Object(p))
        }
        ChatsAction::Info { jid } => ("chats.info", serde_json::json!({"jid": jid})),
        ChatsAction::Pin { jid } => ("chats.pin", serde_json::json!({"jid": jid})),
        ChatsAction::Unpin { jid } => ("chats.unpin", serde_json::json!({"jid": jid})),
        ChatsAction::Mute {
            jid,
            until_epoch_secs,
        } => (
            "chats.mute",
            serde_json::json!({"jid": jid, "until_epoch_secs": until_epoch_secs}),
        ),
        ChatsAction::Archive { jid } => ("chats.archive", serde_json::json!({"jid": jid})),
        ChatsAction::Delete { jid } => ("chats.delete", serde_json::json!({"jid": jid})),
        ChatsAction::Typing { jid, on } => {
            ("chats.typing", serde_json::json!({"jid": jid, "on": on}))
        }
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Wire `envelope *` subcommands (Task 56). Mirrors `envelope.*` RPC surface.
pub fn dispatch_envelope(cli: &Cli, cmd: &EnvelopeCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        EnvelopeAction::Encode { file } => {
            let mut p = serde_json::Map::new();
            if let Some(f) = file {
                p.insert(
                    "file".into(),
                    serde_json::Value::String(f.to_string_lossy().into_owned()),
                );
            }
            ("envelope.encode", serde_json::Value::Object(p))
        }
        EnvelopeAction::Decode => ("envelope.decode", serde_json::Value::Null),
        EnvelopeAction::Send { peer, file } => (
            "envelope.send",
            serde_json::json!({"peer": peer, "file": file.to_string_lossy()}),
        ),
        EnvelopeAction::SendNative { peer, file } => (
            "envelope.send-native",
            serde_json::json!({"peer": peer, "file": file.to_string_lossy()}),
        ),
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Wire `media *` subcommands (Task 56). Mirrors `media.*` RPC surface.
pub fn dispatch_media(cli: &Cli, cmd: &MediaCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        MediaAction::Info { media_ref_token } => (
            "media.info",
            serde_json::json!({"media_ref_token": media_ref_token}),
        ),
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Wire `octo-whatsapp capabilities` (Task 56). Single RPC, no params.
pub fn dispatch_capabilities(cli: &Cli) -> anyhow::Result<()> {
    dispatch_simple(cli, "capabilities")
}

/// Wire `domain *` subcommands (Task 56). Mirrors `domain.*` RPC surface.
pub fn dispatch_domain(cli: &Cli, cmd: &DomainCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        DomainAction::ComputeHash { group_jid } => (
            "domain.compute-hash",
            // Map the CLI's positional `group_jid` to the handler's
            // canonical `jid` field (consistent with the rest of the
            // chats/groups/messages RPC surface).
            serde_json::json!({"jid": group_jid}),
        ),
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Wire `rules *` subcommands. Phase 5 Part E adds the Phase 4 CRUD/dry-run
/// surface (`create`/`update`/`patch`/`delete`/`enable`/`disable`/`approve`/
/// `reload`/`flush`/`test`) on top of the Phase 1 read-only list/get.
pub fn dispatch_rules(cli: &Cli, cmd: &RulesCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        RulesAction::List => ("rules.list", serde_json::Value::Null),
        RulesAction::Get { id } => ("rules.get", serde_json::json!({"id": id})),
        RulesAction::Create { body } => {
            // The CLI accepts a raw JSON literal for `create` so operators
            // can pipe `jq`/heredocs without argument gymnastics. The
            // daemon validates every field.
            let body: serde_json::Value = serde_json::from_str(body)
                .map_err(|e| anyhow::anyhow!("invalid body for rules create: {e}"))?;
            ("rules.create", body)
        }
        RulesAction::Update { id, etag, body } => {
            let mut body: serde_json::Value = serde_json::from_str(body)
                .map_err(|e| anyhow::anyhow!("invalid body for rules update: {e}"))?;
            if let Some(o) = body.as_object_mut() {
                o.insert("id".into(), serde_json::Value::String(id.clone()));
                o.insert("etag".into(), serde_json::Value::String(etag.clone()));
            } else {
                anyhow::bail!("rules update body must be a JSON object");
            }
            ("rules.update", body)
        }
        RulesAction::Patch { id, etag, body } => {
            let mut body: serde_json::Value = serde_json::from_str(body)
                .map_err(|e| anyhow::anyhow!("invalid body for rules patch: {e}"))?;
            if let Some(o) = body.as_object_mut() {
                o.insert("id".into(), serde_json::Value::String(id.clone()));
                o.insert("etag".into(), serde_json::Value::String(etag.clone()));
            } else {
                anyhow::bail!("rules patch body must be a JSON object");
            }
            ("rules.patch", body)
        }
        RulesAction::Delete { id, etag } => {
            ("rules.delete", serde_json::json!({"id": id, "etag": etag}))
        }
        RulesAction::Enable { id } => ("rules.enable", serde_json::json!({"id": id})),
        RulesAction::Disable { id } => ("rules.disable", serde_json::json!({"id": id})),
        RulesAction::Approve { id } => ("rules.approve", serde_json::json!({"id": id})),
        RulesAction::Reload => ("rules.reload", serde_json::Value::Null),
        RulesAction::Flush => ("rules.flush", serde_json::Value::Null),
        RulesAction::Test { event_json } => {
            // `rules.test` expects `{ "event": {...} }`. The CLI accepts the
            // raw inbound event blob and wraps it here so the operator
            // can pipe the same JSON the daemon itself emits.
            let mut wrapper = serde_json::Map::new();
            let ev: serde_json::Value = serde_json::from_str(event_json)
                .map_err(|e| anyhow::anyhow!("invalid event-json for rules test: {e}"))?;
            wrapper.insert("event".into(), ev);
            ("rules.test", serde_json::Value::Object(wrapper))
        }
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Wire `triggers *` subcommands. Phase 5 Part E adds Phase 4 CRUD
/// (`create`/`update`/`delete`) and the `run` dry-run.
pub fn dispatch_triggers(cli: &Cli, cmd: &TriggersCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        TriggersAction::List => ("triggers.list", serde_json::Value::Null),
        TriggersAction::Get { id } => ("triggers.get", serde_json::json!({"id": id})),
        TriggersAction::Create { body } => {
            let body: serde_json::Value = serde_json::from_str(body)
                .map_err(|e| anyhow::anyhow!("invalid body for triggers create: {e}"))?;
            ("triggers.create", body)
        }
        TriggersAction::Update { id, etag, body } => {
            let mut body: serde_json::Value = serde_json::from_str(body)
                .map_err(|e| anyhow::anyhow!("invalid body for triggers update: {e}"))?;
            if let Some(o) = body.as_object_mut() {
                o.insert("id".into(), serde_json::Value::String(id.clone()));
                o.insert("etag".into(), serde_json::Value::String(etag.clone()));
            } else {
                anyhow::bail!("triggers update body must be a JSON object");
            }
            ("triggers.update", body)
        }
        TriggersAction::Delete { id, etag } => (
            "triggers.delete",
            serde_json::json!({"id": id, "etag": etag}),
        ),
        TriggersAction::Run { id, payload_json } => {
            let mut p = serde_json::Map::new();
            p.insert("id".into(), serde_json::Value::String(id.clone()));
            if let Some(raw) = payload_json {
                let ev: serde_json::Value = serde_json::from_str(raw)
                    .map_err(|e| anyhow::anyhow!("invalid --payload-json for triggers run: {e}"))?;
                p.insert("event".into(), ev);
            }
            ("triggers.run", serde_json::Value::Object(p))
        }
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Wire `audit *` subcommands. Phase 5 Part E exposes the Phase 4 audit
/// hash-chain surface (`tail` + `verify`).
pub fn dispatch_audit(cli: &Cli, cmd: &AuditCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        AuditAction::Tail { since_seq, limit } => {
            let mut p = serde_json::Map::new();
            if let Some(s) = since_seq {
                p.insert("since_seq".into(), serde_json::Value::Number((*s).into()));
            }
            if let Some(l) = limit {
                p.insert("limit".into(), serde_json::Value::Number((*l).into()));
            }
            ("audit.tail", serde_json::Value::Object(p))
        }
        AuditAction::Verify => ("audit.verify", serde_json::Value::Null),
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Wire `actions *` subcommands. Phase 5 Part E exposes the Phase 4
/// `actions.escalate` RPC (currently a stub that returns a token).
pub fn dispatch_actions(cli: &Cli, cmd: &ActionsCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        ActionsAction::Escalate { target, reason } => (
            "actions.escalate",
            serde_json::json!({"target": target, "reason": reason}),
        ),
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Tier 7.G: dispatch surface for the 9 community RPCs. Each arm
/// maps a CLI subcommand to the matching `community.*` JSON-RPC method
/// on the daemon.
pub fn dispatch_community(cli: &Cli, cmd: &CommunityCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        CommunityAction::Create {
            name,
            description,
            closed,
            allow_non_admin_sub_group_creation,
            create_general_chat,
        } => (
            "community.create",
            serde_json::json!({
                "name": name,
                "description": description,
                "closed": closed,
                "allow_non_admin_sub_group_creation": allow_non_admin_sub_group_creation,
                "create_general_chat": create_general_chat,
            }),
        ),
        CommunityAction::Deactivate { community_jid } => (
            "community.deactivate",
            serde_json::json!({ "jid": community_jid }),
        ),
        CommunityAction::LinkSubgroups {
            community_jid,
            subgroup_jids,
        } => (
            "community.link_subgroups",
            serde_json::json!({
                "community_jid": community_jid,
                "subgroup_jids": subgroup_jids,
            }),
        ),
        CommunityAction::UnlinkSubgroups {
            community_jid,
            subgroup_jids,
            remove_orphan_members,
        } => (
            "community.unlink_subgroups",
            serde_json::json!({
                "community_jid": community_jid,
                "subgroup_jids": subgroup_jids,
                "remove_orphan_members": remove_orphan_members,
            }),
        ),
        CommunityAction::GetSubgroups { community_jid } => (
            "community.get_subgroups",
            serde_json::json!({ "community_jid": community_jid }),
        ),
        CommunityAction::GetSubgroupParticipantCounts { community_jid } => (
            "community.get_subgroup_participant_counts",
            serde_json::json!({ "community_jid": community_jid }),
        ),
        CommunityAction::QueryLinkedGroup {
            community_jid,
            subgroup_jid,
        } => (
            "community.query_linked_group",
            serde_json::json!({
                "community_jid": community_jid,
                "subgroup_jid": subgroup_jid,
            }),
        ),
        CommunityAction::JoinSubgroup {
            community_jid,
            subgroup_jid,
        } => (
            "community.join_subgroup",
            serde_json::json!({
                "community_jid": community_jid,
                "subgroup_jid": subgroup_jid,
            }),
        ),
        CommunityAction::GetLinkedGroupsParticipants { community_jid } => (
            "community.get_linked_groups_participants",
            serde_json::json!({ "community_jid": community_jid }),
        ),
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Phase 7.E+ Newsletter bridge: CLI mirror for the 14 `newsletter.*` RPCs.
pub fn dispatch_newsletter(cli: &Cli, cmd: &NewsletterCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        NewsletterAction::ListSubscribed => ("newsletter.list_subscribed", serde_json::json!({})),
        NewsletterAction::GetMetadata { jid } => {
            ("newsletter.get_metadata", serde_json::json!({ "jid": jid }))
        }
        NewsletterAction::Create { name, description } => (
            "newsletter.create",
            serde_json::json!({
                "name": name,
                "description": description,
            }),
        ),
        NewsletterAction::Join { jid } => ("newsletter.join", serde_json::json!({ "jid": jid })),
        NewsletterAction::Leave { jid } => ("newsletter.leave", serde_json::json!({ "jid": jid })),
        NewsletterAction::SendReaction {
            jid,
            server_id,
            reaction,
        } => (
            "newsletter.send_reaction",
            serde_json::json!({
                "jid": jid,
                "server_id": server_id,
                "reaction": reaction,
            }),
        ),
        NewsletterAction::EditMessage {
            jid,
            message_id,
            new_text,
        } => (
            "newsletter.edit_message",
            serde_json::json!({
                "jid": jid,
                "message_id": message_id,
                "new_text": new_text,
            }),
        ),
        NewsletterAction::RevokeMessage { jid, message_id } => (
            "newsletter.revoke_message",
            serde_json::json!({
                "jid": jid,
                "message_id": message_id,
            }),
        ),
        NewsletterAction::Update {
            jid,
            name,
            description,
        } => (
            "newsletter.update",
            serde_json::json!({
                "jid": jid,
                "name": name,
                "description": description,
            }),
        ),
        NewsletterAction::SetFollowerMute { jid, muted } => (
            "newsletter.set_follower_mute",
            serde_json::json!({
                "jid": jid,
                "muted": muted,
            }),
        ),
        NewsletterAction::SetAdminMute { jid, muted } => (
            "newsletter.set_admin_mute",
            serde_json::json!({
                "jid": jid,
                "muted": muted,
            }),
        ),
        NewsletterAction::GetMetadataByInvite { invite } => (
            "newsletter.get_metadata_by_invite",
            serde_json::json!({ "invite": invite }),
        ),
        NewsletterAction::SubscribeLiveUpdates { jid } => (
            "newsletter.subscribe_live_updates",
            serde_json::json!({ "jid": jid }),
        ),
        NewsletterAction::GetMessages { jid, count, before } => (
            "newsletter.get_messages",
            serde_json::json!({
                "jid": jid,
                "count": count,
                "before": before,
            }),
        ),
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Phase 1 task 14: CLI mirror for the query-layer RPCs.
#[cfg(feature = "query")]
pub fn dispatch_query(cli: &Cli, cmd: &QueryCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        QueryAction::Search {
            query,
            peer,
            kind,
            since_ts_unix_ms,
            until_ts_unix_ms,
            limit,
        } => (
            "daemon.search",
            serde_json::json!({
                "query": query,
                "peer": peer,
                "kind": kind,
                "since_ts_unix_ms": since_ts_unix_ms,
                "until_ts_unix_ms": until_ts_unix_ms,
                "limit": limit,
            }),
        ),
        QueryAction::Context {
            event_id,
            before,
            after,
        } => (
            "messages.context",
            serde_json::json!({
                "event_id": event_id,
                "before": before,
                "after": after,
            }),
        ),
        QueryAction::Find {
            kind,
            variant,
            peer,
            since_ts_unix_ms,
            until_ts_unix_ms,
            limit,
        } => (
            "events.find",
            serde_json::json!({
                "kind": kind,
                "variant": variant,
                "peer": peer,
                "since_ts_unix_ms": since_ts_unix_ms,
                "until_ts_unix_ms": until_ts_unix_ms,
                "limit": limit,
            }),
        ),
        QueryAction::Execute { sql } => ("sql.execute", serde_json::json!({ "sql": sql })),
        QueryAction::Query { sql, limit } => (
            "sql.query",
            serde_json::json!({ "sql": sql, "limit": limit }),
        ),
        QueryAction::Tables => ("sql.tables", serde_json::Value::Null),
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Phase 7.J: CLI mirror for `contacts.*` and `contact.*` RPCs.
pub fn dispatch_contacts(cli: &Cli, cmd: &ContactsCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        ContactsAction::IsOnWhatsApp { peer } => (
            "contacts.is_on_whatsapp",
            serde_json::json!({ "peer": peer }),
        ),
        ContactsAction::GetUserInfo { peer } => (
            "contacts.get_user_info",
            serde_json::json!({ "peer": peer }),
        ),
        ContactsAction::GetBusinessProfile { jid } => (
            "contacts.get_business_profile",
            serde_json::json!({ "jid": jid }),
        ),
        ContactsAction::GetPnLidMappings { phones } => (
            "contacts.get_pn_lid_mappings",
            serde_json::json!({ "phones": phones }),
        ),
        ContactsAction::GetLidPnMappings { lids } => (
            "contacts.get_lid_pn_mappings",
            serde_json::json!({ "lids": lids }),
        ),
        ContactsAction::GetProfilePicture { peer, preview } => {
            let mut p = serde_json::json!({ "peer": peer });
            if let Some(prev) = preview {
                p["preview"] = serde_json::json!(prev);
            }
            ("contacts.get_profile_picture", p)
        }
        ContactsAction::SaveContact { peer, full_name } => (
            "contacts.save_contact",
            serde_json::json!({ "peer": peer, "full_name": full_name }),
        ),
        ContactsAction::Block { peer } => ("contact.block", serde_json::json!({ "peer": peer })),
        ContactsAction::Unblock { peer } => {
            ("contact.unblock", serde_json::json!({ "peer": peer }))
        }
    };
    let result = client.call(method, params)?;
    let _ = print_result(cli.json, &result);
    Ok(())
}

/// Phase 7.J / Tier 6.4: CLI mirror for `identity.*` RPCs.
pub fn dispatch_identity(cli: &Cli, cmd: &IdentityCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let method = match &cmd.action {
        IdentityAction::GetPn => "identity.get_pn",
        IdentityAction::GetLid => "identity.get_lid",
        IdentityAction::IsLidMigrated => "identity.is_lid_migrated",
    };
    let result = client.call(method, serde_json::Value::Null)?;
    let _ = print_result(cli.json, &result);
    Ok(())
}

/// Wire `events list` and `events show <id>` (Task 47).
pub fn dispatch_events(cli: &Cli, cmd: &EventsCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        EventsAction::List { limit } => ("events.list", serde_json::json!({ "limit": limit })),
        EventsAction::Show { id } => ("events.show", serde_json::json!({"id": id})),
        EventsAction::Replay { since_id, limit } => (
            "events.replay",
            serde_json::json!({
                "since_id": since_id.unwrap_or(0),
                "limit": limit,
            }),
        ),
        EventsAction::Tail { limit } => ("events.tail", serde_json::json!({ "limit": limit })),
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Phase 3: `clients list` discovery.
pub fn dispatch_clients(cli: &Cli, cmd: &ClientsCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        ClientsAction::List => ("clients.list", serde_json::Value::Null),
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Phase 3: `methods list|show` discovery.
pub fn dispatch_methods(cli: &Cli, cmd: &MethodsCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        MethodsAction::List => ("daemon.methods.list", serde_json::Value::Null),
        MethodsAction::Show { method } => (
            "daemon.methods.help",
            serde_json::json!({ "method": method }),
        ),
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Phase 5 Part A: bearer-token operations.
pub fn dispatch_tokens(cli: &Cli, cmd: &TokenCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        TokenAction::Rotate {
            old_token_id,
            new_secret_hex,
            grace_ms,
            label,
        } => (
            "security.rotate_token",
            serde_json::json!({
                "old_token_id": old_token_id,
                "new_secret_hex": new_secret_hex,
                "grace_ms": grace_ms,
                "label": label,
            }),
        ),
        TokenAction::RevokeAll => ("security.revoke_all_tokens", serde_json::Value::Null),
        TokenAction::List => ("security.list_tokens", serde_json::Value::Null),
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Phase 6.1: multi-account management RPCs.
pub fn dispatch_accounts(cli: &Cli, cmd: &AccountsCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        AccountsAction::List => ("daemon.accounts.list", serde_json::Value::Null),
        AccountsAction::Use { account_id } => (
            "daemon.accounts.use",
            serde_json::json!({ "account_id": account_id }),
        ),
        AccountsAction::Info { account_id } => (
            "daemon.accounts.info",
            serde_json::json!({ "account_id": account_id }),
        ),
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}

/// Wire `reconnect` and `shutdown` (Task 48).
pub fn dispatch_reconnect(cli: &Cli) -> anyhow::Result<()> {
    let result =
        RpcClient::new(resolve_socket_path(cli)).call("reconnect.now", serde_json::Value::Null)?;
    print_result(cli.json, &result)
}

pub fn dispatch_shutdown(cli: &Cli) -> anyhow::Result<()> {
    let result =
        RpcClient::new(resolve_socket_path(cli)).call("shutdown", serde_json::Value::Null)?;
    print_result(cli.json, &result)
}

/// Print a "this command requires the standalone `octo-whatsapp-onboard` binary"
/// delegation message. Onboarding is daemon-free by design.
pub fn onboard_passthrough_message(action: &str, args: &[&str]) -> anyhow::Result<()> {
    println!(
        "octo-whatsapp: onboard {action} {args} is provided by the standalone `octo-whatsapp-onboard` binary.",
        args = args.join(" ")
    );
    println!(
        "Run: octo-whatsapp-onboard {action} {args}",
        action = action,
        args = args.join(" ")
    );
    Ok(())
}

/// Wire `onboard *` subcommands (Task 49). Phase 1: passthrough only — the
/// runtime does not shell out to the standalone binary (cross-crate binary
/// invocation has its own edge cases); it instructs the operator.
pub fn dispatch_onboard(_cli: &Cli, cmd: &OnboardCmd) -> anyhow::Result<()> {
    match &cmd.action {
        OnboardAction::QrLink { timeout } => {
            onboard_passthrough_message("qr-link", &[&format!("--timeout={timeout}")])
        }
        OnboardAction::PairLink { phone } => onboard_passthrough_message("pair-link", &[phone]),
        OnboardAction::Whoami => onboard_passthrough_message("whoami", &[]),
        OnboardAction::Session { action } => match action {
            SessionCmd::List => onboard_passthrough_message("session", &["list"]),
            SessionCmd::Verify { name } => {
                onboard_passthrough_message("session", &["verify", name])
            }
            SessionCmd::Remove { name } => {
                onboard_passthrough_message("session", &["remove", name])
            }
        },
    }
}

/// Top-level dispatch. Called by `main()` after `Cli::parse()`. Routes each
/// `Command` variant to the appropriate leaf dispatcher.
pub fn dispatch(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Command::Daemon => {
            // Phase 7.J.2: install a stderr tracing subscriber early so
            // every `tracing::*!` from the daemon, wacore, octo_cable, etc.
            // lands in the operator's terminal (or journalctl) instead of
            // being swallowed by the default no-subscriber build. Opt-in
            // via the `tracing-stdout` cargo feature — see
            // `observability::tracing::init_tracing_stdout`.
            #[cfg(feature = "tracing-stdout")]
            crate::observability::init_tracing_stdout();

            // The daemon path needs to be async. Build a small runtime so
            // `main()` can stay sync (matching the plan's snippet).
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            runtime.block_on(async move {
                // Build a complete config from CLI flags (not just `name`).
                // The production daemon needs data_dir, log_dir, socket_dir,
                // media_buffer, events, security, observability, rules — all
                // of which `from_toml("name = ...")` does NOT populate.
                //
                // For Phase 6.0, we keep the historical "toml from cli.name"
                // pattern as a base, then layer a default `WhatsAppRuntimeConfig`
                // underneath so the runtime substructs (events, security, etc.)
                // have valid defaults.
                let config = crate::config::WhatsAppRuntimeConfig::from_toml(
                    format!("name = {:?}\n", cli.name).as_bytes(),
                )?;
                // Apply any additional CLI flags here (Phase 6.0: none).
                // Future: --config-file flag, --data-dir override, etc.

                let daemon = crate::daemon::Daemon::new(config.clone());

                // Construct the live WhatsApp Web adapter. Phase 6.12.5:
                // bind BEFORE start_bot so the connection-watcher
                // subscribes before any boot-time lifecycle events fire.
                // `start_bot` is now fire-and-forget; failures are logged
                // but the daemon continues — the watcher observes
                // whatever state the bot reaches (Connected or any of
                // the 6 non-Connected BotState variants).
                let adapter_cfg = config.adapter_config();
                let adapter = std::sync::Arc::new(octo_adapter_whatsapp::WhatsAppWebAdapter::new(
                    adapter_cfg.clone(),
                ));
                let adapter_for_start = adapter.clone();
                daemon.handle().bind_adapter_and_start(adapter, move || async move {
                    if let Err(e) = adapter_for_start.start_bot().await {
                        tracing::error!(
                            account = %config.name,
                            session = %adapter_cfg.session_path,
                            "start_bot failed; daemon continues (watcher will observe state): {e}"
                        );
                    }
                });

                daemon.run().await
            })
        }
        Command::Mcp => {
            // MCP server (Part L — Task 51-52). Spawns a multi-threaded
            // tokio runtime and forwards JSON-RPC requests from stdin to the
            // daemon's unix socket, writing responses on stdout. Stdio reads
            // are blocking, so we use a multi-threaded runtime so other tasks
            // can drive the daemon forward if needed.
            let socket = resolve_socket_path(&cli);
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            runtime.block_on(crate::mcp_server::serve(&socket))
        }
        Command::Version => dispatch_simple(&cli, "version.get"),
        Command::Status => dispatch_simple(&cli, "status.get"),
        Command::Health => dispatch_simple(&cli, "health.get"),
        Command::Send(ref args) => dispatch_send(&cli, args),
        Command::Groups(ref cmd) => dispatch_groups(&cli, cmd),
        Command::Messages(ref cmd) => dispatch_messages(&cli, cmd),
        Command::Chats(ref cmd) => dispatch_chats(&cli, cmd),
        Command::Envelope(ref cmd) => dispatch_envelope(&cli, cmd),
        Command::Media(ref cmd) => dispatch_media(&cli, cmd),
        Command::Capabilities => dispatch_capabilities(&cli),
        Command::Domain(ref cmd) => dispatch_domain(&cli, cmd),
        Command::Rules(ref cmd) => dispatch_rules(&cli, cmd),
        Command::Triggers(ref cmd) => dispatch_triggers(&cli, cmd),
        Command::Events(ref cmd) => dispatch_events(&cli, cmd),
        Command::Reconnect => dispatch_reconnect(&cli),
        Command::Shutdown => dispatch_shutdown(&cli),
        Command::Onboard(ref cmd) => dispatch_onboard(&cli, cmd),
        Command::Clients(ref cmd) => dispatch_clients(&cli, cmd),
        Command::Accounts(ref cmd) => dispatch_accounts(&cli, cmd),
        Command::Methods(ref cmd) => dispatch_methods(&cli, cmd),
        Command::Tokens(ref cmd) => dispatch_tokens(&cli, cmd),
        Command::Audit(ref cmd) => dispatch_audit(&cli, cmd),
        Command::Actions(ref cmd) => dispatch_actions(&cli, cmd),
        #[cfg(feature = "query")]
        Command::Query(ref cmd) => dispatch_query(&cli, cmd),
        Command::Contacts(ref cmd) => dispatch_contacts(&cli, cmd),
        Command::Identity(ref cmd) => dispatch_identity(&cli, cmd),
        Command::Community(ref cmd) => dispatch_community(&cli, cmd),
        Command::Newsletter(ref cmd) => dispatch_newsletter(&cli, cmd),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli_with(socket: Option<PathBuf>, name: &str) -> Cli {
        Cli {
            socket,
            name: name.to_string(),
            json: false,
            command: Command::Version,
        }
    }

    #[test]
    fn resolve_socket_path_uses_socket_override() {
        let cli = cli_with(Some(PathBuf::from("/tmp/override.sock")), "default");
        assert_eq!(
            resolve_socket_path(&cli),
            PathBuf::from("/tmp/override.sock")
        );
    }

    #[test]
    fn resolve_socket_path_derives_from_name_when_no_socket() {
        let path = resolve_socket_path_with_env("alpha", Some("/run/user/1000"));
        assert_eq!(
            path,
            PathBuf::from("/run/user/1000/octo-whatsapp-alpha.sock")
        );
    }

    #[test]
    fn resolve_socket_path_falls_back_to_tmp() {
        let path = resolve_socket_path_with_env("beta", None);
        assert_eq!(path, PathBuf::from("/tmp/octo-whatsapp-beta.sock"));
    }

    #[test]
    fn rpc_client_call_reports_socket_unreachable() {
        let c = RpcClient::new(PathBuf::from("/nonexistent/octo-whatsapp-test.sock"));
        let err = c.call("version.get", serde_json::Value::Null).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("is the daemon running"),
            "expected friendly hint in error, got: {msg}"
        );
    }

    /// `print_result` with `as_json=true` always emits `to_string_pretty`,
    /// regardless of the value shape — that's the --json contract.
    #[test]
    fn print_result_json_mode_emits_pretty_for_any_value() {
        // Pure-data verification: the json-mode branch is just
        // `serde_json::to_string_pretty(value)` plus a trailing newline.
        let v = serde_json::json!({"k": "v"});
        let s = serde_json::to_string_pretty(&v).unwrap();
        assert!(s.contains("\"k\""));
        assert!(s.contains("\"v\""));
    }

    /// `--json` is a global flag on `Cli`; verify clap wires it through.
    #[test]
    fn cli_parses_global_json_flag() {
        let c = Cli::try_parse_from(["octo-whatsapp", "--json", "version"]).expect("parse");
        assert!(c.json);
    }

    // -----------------------------------------------------------------------
    // Additional coverage: pure-function + clap parse + print_result tests.
    // -----------------------------------------------------------------------
    //
    // These cover the bulk of `cli.rs` without ever opening a unix socket.
    // Variant names are kept in sync with the `Command`/`SendKind`/etc.
    // definitions above; if a clap enum variant is renamed, these tests must
    // be updated alongside it.

    #[test]
    fn resolve_socket_path_with_env_uses_xdg_when_set() {
        let p = resolve_socket_path_with_env("test", Some("/tmp/xdg"));
        assert!(p.ends_with("octo-whatsapp-test.sock"));
        assert!(p.starts_with("/tmp/xdg/"));
    }

    #[test]
    fn resolve_socket_path_with_env_falls_back_to_tmp_when_xdg_unset() {
        let p = resolve_socket_path_with_env("test", None);
        assert!(p.ends_with("octo-whatsapp-test.sock"));
        // Falls back to /tmp when xdg_runtime_dir is None.
        assert!(
            p.starts_with("/tmp/"),
            "expected /tmp fallback, got {}",
            p.display()
        );
    }

    #[test]
    fn resolve_socket_path_honors_socket_override() {
        // When `--socket` is provided, the override must win over both the
        // env-derived default and the name. This guards the most common
        // operator workflow (manual debugging on a non-default port).
        let cli = Cli::try_parse_from([
            "octo-whatsapp",
            "--socket",
            "/var/run/octo.sock",
            "--name",
            "myinstance",
            "version",
        ])
        .unwrap();
        assert_eq!(
            resolve_socket_path(&cli),
            PathBuf::from("/var/run/octo.sock")
        );
    }

    #[test]
    fn resolve_socket_path_uses_cli_name_when_no_socket_or_env() {
        // Default name is "default"; with no env set and no --socket, the
        // socket path should still include the chosen instance name.
        let cli =
            Cli::try_parse_from(["octo-whatsapp", "--name", "myinstance", "version"]).unwrap();
        let p = resolve_socket_path(&cli);
        assert!(
            p.to_string_lossy().contains("myinstance"),
            "expected instance name in socket path, got {}",
            p.display()
        );
    }

    // ---- clap parse tests ----

    #[test]
    fn cli_parses_daemon() {
        let c = Cli::try_parse_from(["octo-whatsapp", "daemon"]).unwrap();
        assert!(matches!(c.command, Command::Daemon));
    }

    #[test]
    fn cli_parses_mcp() {
        let c = Cli::try_parse_from(["octo-whatsapp", "mcp"]).unwrap();
        assert!(matches!(c.command, Command::Mcp));
    }

    #[test]
    fn cli_parses_capabilities() {
        let c = Cli::try_parse_from(["octo-whatsapp", "capabilities"]).unwrap();
        assert!(matches!(c.command, Command::Capabilities));
    }

    #[test]
    fn cli_parses_reconnect() {
        let c = Cli::try_parse_from(["octo-whatsapp", "reconnect"]).unwrap();
        assert!(matches!(c.command, Command::Reconnect));
    }

    #[test]
    fn cli_parses_shutdown() {
        let c = Cli::try_parse_from(["octo-whatsapp", "shutdown"]).unwrap();
        assert!(matches!(c.command, Command::Shutdown));
    }

    #[test]
    fn cli_parses_groups_list() {
        let c = Cli::try_parse_from(["octo-whatsapp", "groups", "list"]).unwrap();
        assert!(matches!(c.command, Command::Groups(_)));
    }

    #[test]
    fn cli_parses_groups_create_with_members() {
        let c = Cli::try_parse_from([
            "octo-whatsapp",
            "groups",
            "create",
            "--subject",
            "My Group",
            "--members",
            "111,222,333",
        ])
        .unwrap();
        match c.command {
            Command::Groups(cmd) => match cmd.action {
                GroupsAction::Create {
                    ref subject,
                    ref members,
                } => {
                    assert_eq!(subject, "My Group");
                    assert_eq!(members, &vec!["111", "222", "333"]);
                }
                _ => panic!("expected GroupsAction::Create"),
            },
            _ => panic!("expected Command::Groups"),
        }
    }

    #[test]
    fn cli_parses_messages_list_no_filters() {
        let c = Cli::try_parse_from(["octo-whatsapp", "messages", "list"]).unwrap();
        match c.command {
            Command::Messages(cmd) => match cmd.action {
                MessagesAction::List {
                    ref peer,
                    ref since,
                    ref limit,
                } => {
                    assert!(peer.is_none());
                    assert!(since.is_none());
                    assert!(limit.is_none());
                }
                _ => panic!("expected MessagesAction::List"),
            },
            _ => panic!("expected Command::Messages"),
        }
    }

    #[test]
    fn cli_parses_messages_edit() {
        let c = Cli::try_parse_from([
            "octo-whatsapp",
            "messages",
            "edit",
            "+15551234567",
            "msg-1",
            "--msg-timestamp",
            "1700000000",
            "--new-text",
            "fixed",
        ])
        .unwrap();
        assert!(matches!(c.command, Command::Messages(_)));
    }

    #[test]
    fn cli_parses_chats_pin() {
        let c = Cli::try_parse_from([
            "octo-whatsapp",
            "chats",
            "pin",
            "12025550100@s.whatsapp.net",
        ])
        .unwrap();
        match c.command {
            Command::Chats(cmd) => match cmd.action {
                ChatsAction::Pin { ref jid } => {
                    assert_eq!(jid, "12025550100@s.whatsapp.net");
                }
                _ => panic!("expected ChatsAction::Pin"),
            },
            _ => panic!("expected Command::Chats"),
        }
    }

    #[test]
    fn cli_parses_chats_typing_on_and_off() {
        // `on` is a clap `bool` flag (action = SetTrue). Bare `--on` enables;
        // absence leaves it at the default `false`. Negative form is not
        // accepted because the type is `bool`, not `bool` with override.
        let on = Cli::try_parse_from(["octo-whatsapp", "chats", "typing", "jid", "--on"]).unwrap();
        match on.command {
            Command::Chats(cmd) => match cmd.action {
                ChatsAction::Typing { ref jid, on } => {
                    assert_eq!(jid, "jid");
                    assert!(on, "--on flag must set bool to true");
                }
                _ => panic!("expected ChatsAction::Typing"),
            },
            _ => panic!("expected Command::Chats"),
        }

        let off = Cli::try_parse_from(["octo-whatsapp", "chats", "typing", "jid"]).unwrap();
        match off.command {
            Command::Chats(cmd) => match cmd.action {
                ChatsAction::Typing { ref jid, on } => {
                    assert_eq!(jid, "jid");
                    assert!(!on, "absence of --on must default to false");
                }
                _ => panic!("expected ChatsAction::Typing"),
            },
            _ => panic!("expected Command::Chats"),
        }
    }

    #[test]
    fn cli_parses_envelope_encode_with_file() {
        let c = Cli::try_parse_from([
            "octo-whatsapp",
            "envelope",
            "encode",
            "--file",
            "/tmp/blob.bin",
        ])
        .unwrap();
        match c.command {
            Command::Envelope(cmd) => match cmd.action {
                EnvelopeAction::Encode { ref file } => {
                    assert_eq!(file.as_deref(), Some(std::path::Path::new("/tmp/blob.bin")));
                }
                _ => panic!("expected EnvelopeAction::Encode"),
            },
            _ => panic!("expected Command::Envelope"),
        }
    }

    #[test]
    fn cli_parses_envelope_decode_no_args() {
        let c = Cli::try_parse_from(["octo-whatsapp", "envelope", "decode"]).unwrap();
        match c.command {
            Command::Envelope(cmd) => match cmd.action {
                EnvelopeAction::Decode => {}
                _ => panic!("expected EnvelopeAction::Decode"),
            },
            _ => panic!("expected Command::Envelope"),
        }
    }

    #[test]
    fn cli_parses_envelope_send() {
        let c = Cli::try_parse_from([
            "octo-whatsapp",
            "envelope",
            "send",
            "+15551234567",
            "/tmp/dot.env",
        ])
        .unwrap();
        match c.command {
            Command::Envelope(cmd) => match cmd.action {
                EnvelopeAction::Send { ref peer, .. } => {
                    assert_eq!(peer, "+15551234567");
                }
                _ => panic!("expected EnvelopeAction::Send"),
            },
            _ => panic!("expected Command::Envelope"),
        }
    }

    #[test]
    fn cli_parses_media_info() {
        let c = Cli::try_parse_from(["octo-whatsapp", "media", "info", "tok-abc"]).unwrap();
        match c.command {
            Command::Media(cmd) => match cmd.action {
                MediaAction::Info {
                    ref media_ref_token,
                } => {
                    assert_eq!(media_ref_token, "tok-abc");
                }
            },
            _ => panic!("expected Command::Media"),
        }
    }

    #[test]
    fn cli_parses_domain_compute_hash() {
        let c = Cli::try_parse_from(["octo-whatsapp", "domain", "compute-hash", "groupjid-xyz"])
            .unwrap();
        match c.command {
            Command::Domain(cmd) => match cmd.action {
                DomainAction::ComputeHash { ref group_jid } => {
                    assert_eq!(group_jid, "groupjid-xyz");
                }
            },
            _ => panic!("expected Command::Domain"),
        }
    }

    #[test]
    fn cli_parses_rules_list_and_get() {
        let l = Cli::try_parse_from(["octo-whatsapp", "rules", "list"]).unwrap();
        match l.command {
            Command::Rules(cmd) => match cmd.action {
                RulesAction::List => {}
                _ => panic!("expected RulesAction::List"),
            },
            _ => panic!("expected Command::Rules"),
        }
        let g = Cli::try_parse_from(["octo-whatsapp", "rules", "get", "rule-1"]).unwrap();
        match g.command {
            Command::Rules(cmd) => match cmd.action {
                RulesAction::Get { ref id } => {
                    assert_eq!(id, "rule-1");
                }
                _ => panic!("expected RulesAction::Get"),
            },
            _ => panic!("expected Command::Rules"),
        }
    }

    #[test]
    fn cli_parses_rules_create_update_patch_delete() {
        let body_json = r#"{"predicate":{"kind":"event_kind","kinds":["message"]},"actions":[]}"#;
        let c = Cli::try_parse_from(["octo-whatsapp", "rules", "create", body_json]).unwrap();
        match c.command {
            Command::Rules(cmd) => match cmd.action {
                RulesAction::Create { ref body } => assert_eq!(body, body_json),
                _ => panic!("expected RulesAction::Create"),
            },
            _ => panic!("expected Command::Rules"),
        }

        let c2 = Cli::try_parse_from([
            "octo-whatsapp",
            "rules",
            "update",
            "r1",
            "etag-1",
            body_json,
        ])
        .unwrap();
        match c2.command {
            Command::Rules(cmd) => match cmd.action {
                RulesAction::Update {
                    ref id,
                    ref etag,
                    ref body,
                } => {
                    assert_eq!(id, "r1");
                    assert_eq!(etag, "etag-1");
                    assert_eq!(body, body_json);
                }
                _ => panic!("expected RulesAction::Update"),
            },
            _ => panic!("expected Command::Rules"),
        }

        let c3 =
            Cli::try_parse_from(["octo-whatsapp", "rules", "patch", "r1", "etag-1", body_json])
                .unwrap();
        match c3.command {
            Command::Rules(cmd) => match cmd.action {
                RulesAction::Patch {
                    ref id, ref etag, ..
                } => {
                    assert_eq!(id, "r1");
                    assert_eq!(etag, "etag-1");
                }
                _ => panic!("expected RulesAction::Patch"),
            },
            _ => panic!("expected Command::Rules"),
        }

        let c4 = Cli::try_parse_from(["octo-whatsapp", "rules", "delete", "r1", "etag-1"]).unwrap();
        match c4.command {
            Command::Rules(cmd) => match cmd.action {
                RulesAction::Delete { ref id, ref etag } => {
                    assert_eq!(id, "r1");
                    assert_eq!(etag, "etag-1");
                }
                _ => panic!("expected RulesAction::Delete"),
            },
            _ => panic!("expected Command::Rules"),
        }
    }

    #[test]
    fn cli_parses_rules_enable_disable_approve() {
        for (verb, expected) in [
            ("enable", "RulesAction::Enable"),
            ("disable", "RulesAction::Disable"),
            ("approve", "RulesAction::Approve"),
        ] {
            let c = Cli::try_parse_from(["octo-whatsapp", "rules", verb, "r1"]).expect(verb);
            match c.command {
                Command::Rules(cmd) => {
                    let got = match &cmd.action {
                        RulesAction::Enable { id }
                        | RulesAction::Disable { id }
                        | RulesAction::Approve { id } => {
                            assert_eq!(id, "r1");
                            expected
                        }
                        _ => panic!("unexpected variant"),
                    };
                    let _ = got;
                }
                _ => panic!("expected Command::Rules"),
            }
        }
    }

    #[test]
    fn cli_parses_rules_reload_flush_test() {
        let c = Cli::try_parse_from(["octo-whatsapp", "rules", "reload"]).unwrap();
        match c.command {
            Command::Rules(cmd) => assert!(matches!(cmd.action, RulesAction::Reload)),
            _ => panic!("expected Command::Rules"),
        }
        let c = Cli::try_parse_from(["octo-whatsapp", "rules", "flush"]).unwrap();
        match c.command {
            Command::Rules(cmd) => assert!(matches!(cmd.action, RulesAction::Flush)),
            _ => panic!("expected Command::Rules"),
        }
        let c = Cli::try_parse_from(["octo-whatsapp", "rules", "test", "{}"]).unwrap();
        match c.command {
            Command::Rules(cmd) => match cmd.action {
                RulesAction::Test { ref event_json } => assert_eq!(event_json, "{}"),
                _ => panic!("expected RulesAction::Test"),
            },
            _ => panic!("expected Command::Rules"),
        }
    }

    #[test]
    fn cli_parses_triggers_list_and_get() {
        let l = Cli::try_parse_from(["octo-whatsapp", "triggers", "list"]).unwrap();
        assert!(matches!(l.command, Command::Triggers(_)));

        let g = Cli::try_parse_from(["octo-whatsapp", "triggers", "get", "trig-1"]).unwrap();
        assert!(matches!(g.command, Command::Triggers(_)));
    }

    #[test]
    fn cli_parses_triggers_create_update_delete_run() {
        let body_json = r#"{"runner":{"kind":"agent","agent_id":"a1"}}"#;
        let c = Cli::try_parse_from(["octo-whatsapp", "triggers", "create", body_json]).unwrap();
        match c.command {
            Command::Triggers(cmd) => match cmd.action {
                TriggersAction::Create { ref body } => assert_eq!(body, body_json),
                _ => panic!("expected TriggersAction::Create"),
            },
            _ => panic!("expected Command::Triggers"),
        }

        let c2 = Cli::try_parse_from([
            "octo-whatsapp",
            "triggers",
            "update",
            "t1",
            "etag-1",
            body_json,
        ])
        .unwrap();
        match c2.command {
            Command::Triggers(cmd) => match cmd.action {
                TriggersAction::Update {
                    ref id, ref etag, ..
                } => {
                    assert_eq!(id, "t1");
                    assert_eq!(etag, "etag-1");
                }
                _ => panic!("expected TriggersAction::Update"),
            },
            _ => panic!("expected Command::Triggers"),
        }

        let c3 =
            Cli::try_parse_from(["octo-whatsapp", "triggers", "delete", "t1", "etag-1"]).unwrap();
        match c3.command {
            Command::Triggers(cmd) => match cmd.action {
                TriggersAction::Delete { ref id, ref etag } => {
                    assert_eq!(id, "t1");
                    assert_eq!(etag, "etag-1");
                }
                _ => panic!("expected TriggersAction::Delete"),
            },
            _ => panic!("expected Command::Triggers"),
        }

        let c4 = Cli::try_parse_from(["octo-whatsapp", "triggers", "run", "t1"]).unwrap();
        match c4.command {
            Command::Triggers(cmd) => match cmd.action {
                TriggersAction::Run {
                    ref id,
                    ref payload_json,
                } => {
                    assert_eq!(id, "t1");
                    assert!(payload_json.is_none());
                }
                _ => panic!("expected TriggersAction::Run"),
            },
            _ => panic!("expected Command::Triggers"),
        }

        let c5 =
            Cli::try_parse_from(["octo-whatsapp", "triggers", "run", "t1", "{\"k\":1}"]).unwrap();
        match c5.command {
            Command::Triggers(cmd) => match cmd.action {
                TriggersAction::Run {
                    ref payload_json, ..
                } => {
                    assert_eq!(payload_json.as_deref(), Some("{\"k\":1}"));
                }
                _ => panic!("expected TriggersAction::Run"),
            },
            _ => panic!("expected Command::Triggers"),
        }
    }

    #[test]
    fn cli_parses_audit_tail_and_verify() {
        let c = Cli::try_parse_from(["octo-whatsapp", "audit", "tail"]).unwrap();
        match c.command {
            Command::Audit(cmd) => match cmd.action {
                AuditAction::Tail { since_seq, limit } => {
                    assert!(since_seq.is_none());
                    assert!(limit.is_none());
                }
                _ => panic!("expected AuditAction::Tail"),
            },
            _ => panic!("expected Command::Audit"),
        }

        let c2 = Cli::try_parse_from([
            "octo-whatsapp",
            "audit",
            "tail",
            "--since-seq",
            "42",
            "--limit",
            "200",
        ])
        .unwrap();
        match c2.command {
            Command::Audit(cmd) => match cmd.action {
                AuditAction::Tail { since_seq, limit } => {
                    assert_eq!(since_seq, Some(42));
                    assert_eq!(limit, Some(200));
                }
                _ => panic!("expected AuditAction::Tail"),
            },
            _ => panic!("expected Command::Audit"),
        }

        let c3 = Cli::try_parse_from(["octo-whatsapp", "audit", "verify"]).unwrap();
        match c3.command {
            Command::Audit(cmd) => assert!(matches!(cmd.action, AuditAction::Verify)),
            _ => panic!("expected Command::Audit"),
        }
    }

    #[test]
    fn cli_parses_actions_escalate() {
        let c = Cli::try_parse_from([
            "octo-whatsapp",
            "actions",
            "escalate",
            "oncall",
            "alert: db down",
        ])
        .unwrap();
        match c.command {
            Command::Actions(cmd) => match cmd.action {
                ActionsAction::Escalate {
                    ref target,
                    ref reason,
                } => {
                    assert_eq!(target, "oncall");
                    assert_eq!(reason, "alert: db down");
                }
            },
            _ => panic!("expected Command::Actions"),
        }
    }

    #[test]
    fn cli_parses_events_list_and_show() {
        let l = Cli::try_parse_from(["octo-whatsapp", "events", "list"]).unwrap();
        match l.command {
            Command::Events(cmd) => match cmd.action {
                EventsAction::List { .. } => {}
                _ => panic!("expected EventsAction::List"),
            },
            _ => panic!("expected Command::Events"),
        }
        let s = Cli::try_parse_from(["octo-whatsapp", "events", "show", "ev-1"]).unwrap();
        match s.command {
            Command::Events(cmd) => match cmd.action {
                EventsAction::Show { ref id } => {
                    assert_eq!(id, "ev-1");
                }
                _ => panic!("expected EventsAction::Show"),
            },
            _ => panic!("expected Command::Events"),
        }
    }

    #[test]
    fn cli_parses_events_replay_and_tail() {
        let r = Cli::try_parse_from([
            "octo-whatsapp",
            "events",
            "replay",
            "--since-id",
            "42",
            "--limit",
            "200",
        ])
        .unwrap();
        match r.command {
            Command::Events(cmd) => match cmd.action {
                EventsAction::Replay { since_id, limit } => {
                    assert_eq!(since_id, Some(42));
                    assert_eq!(limit, 200);
                }
                _ => panic!("expected EventsAction::Replay"),
            },
            _ => panic!("expected Command::Events"),
        }
        let t = Cli::try_parse_from(["octo-whatsapp", "events", "tail", "--limit", "50"]).unwrap();
        match t.command {
            Command::Events(cmd) => match cmd.action {
                EventsAction::Tail { limit } => assert_eq!(limit, 50),
                _ => panic!("expected EventsAction::Tail"),
            },
            _ => panic!("expected Command::Events"),
        }
    }

    #[test]
    fn cli_parses_clients_list() {
        let c = Cli::try_parse_from(["octo-whatsapp", "clients", "list"]).unwrap();
        assert!(matches!(
            c.command,
            Command::Clients(ClientsCmd {
                action: ClientsAction::List
            })
        ));
    }

    #[test]
    fn cli_parses_methods_list_and_show() {
        let l = Cli::try_parse_from(["octo-whatsapp", "methods", "list"]).unwrap();
        match l.command {
            Command::Methods(cmd) => match cmd.action {
                MethodsAction::List => {}
                _ => panic!("expected MethodsAction::List"),
            },
            _ => panic!("expected Command::Methods"),
        }
        let h = Cli::try_parse_from(["octo-whatsapp", "methods", "show", "send.text"]).unwrap();
        match h.command {
            Command::Methods(cmd) => match cmd.action {
                MethodsAction::Show { method } => {
                    assert_eq!(method, "send.text");
                }
                _ => panic!("expected MethodsAction::Show"),
            },
            _ => panic!("expected Command::Methods"),
        }
    }

    #[test]
    fn cli_parses_accounts_list() {
        let c = Cli::try_parse_from(["octo-whatsapp", "accounts", "list"]).unwrap();
        match c.command {
            Command::Accounts(AccountsCmd {
                action: AccountsAction::List,
            }) => {}
            other => panic!("expected Command::Accounts(List), got {other:?}"),
        }
    }

    #[test]
    fn cli_parses_accounts_use_captures_account_id_arg() {
        let c = Cli::try_parse_from(["octo-whatsapp", "accounts", "use", "work-account"]).unwrap();
        match c.command {
            Command::Accounts(AccountsCmd {
                action: AccountsAction::Use { account_id },
            }) => {
                assert_eq!(account_id, "work-account");
            }
            other => panic!("expected Command::Accounts(Use), got {other:?}"),
        }
    }

    #[test]
    fn cli_parses_accounts_info_captures_account_id_arg() {
        let c = Cli::try_parse_from(["octo-whatsapp", "accounts", "info", "personal"]).unwrap();
        match c.command {
            Command::Accounts(AccountsCmd {
                action: AccountsAction::Info { account_id },
            }) => {
                assert_eq!(account_id, "personal");
            }
            other => panic!("expected Command::Accounts(Info), got {other:?}"),
        }
    }

    #[test]
    fn cli_parses_onboard_qr_link_with_default_timeout() {
        let c = Cli::try_parse_from(["octo-whatsapp", "onboard", "qr-link"]).unwrap();
        match c.command {
            Command::Onboard(cmd) => match cmd.action {
                OnboardAction::QrLink { timeout } => {
                    assert_eq!(timeout, 120, "default timeout must be 120s");
                }
                _ => panic!("expected OnboardAction::QrLink"),
            },
            _ => panic!("expected Command::Onboard"),
        }
    }

    #[test]
    fn cli_parses_onboard_pair_link() {
        let c =
            Cli::try_parse_from(["octo-whatsapp", "onboard", "pair-link", "+15551234567"]).unwrap();
        match c.command {
            Command::Onboard(cmd) => match cmd.action {
                OnboardAction::PairLink { ref phone } => {
                    assert_eq!(phone, "+15551234567");
                }
                _ => panic!("expected OnboardAction::PairLink"),
            },
            _ => panic!("expected Command::Onboard"),
        }
    }

    #[test]
    fn cli_parses_onboard_session_remove() {
        let c = Cli::try_parse_from([
            "octo-whatsapp",
            "onboard",
            "session",
            "remove",
            "old-session",
        ])
        .unwrap();
        match c.command {
            Command::Onboard(cmd) => match cmd.action {
                OnboardAction::Session {
                    action: SessionCmd::Remove { ref name },
                } => {
                    assert_eq!(name, "old-session");
                }
                _ => panic!("expected OnboardAction::Session::Remove"),
            },
            _ => panic!("expected Command::Onboard"),
        }
    }

    #[test]
    fn cli_parses_send_text() {
        let c = Cli::try_parse_from([
            "octo-whatsapp",
            "send",
            "text",
            "+15551234567",
            "--text",
            "hello",
        ])
        .unwrap();
        match c.command {
            Command::Send(args) => match args.kind {
                SendKind::Text { ref peer, ref text } => {
                    assert_eq!(peer, "+15551234567");
                    assert_eq!(text, "hello");
                }
                _ => panic!("expected SendKind::Text"),
            },
            _ => panic!("expected Command::Send"),
        }
    }

    #[test]
    fn cli_parses_send_image_with_caption() {
        let c = Cli::try_parse_from([
            "octo-whatsapp",
            "send",
            "image",
            "+15551234567",
            "/tmp/x.jpg",
            "--caption",
            "my image",
        ])
        .unwrap();
        match c.command {
            Command::Send(args) => match args.kind {
                SendKind::Image {
                    ref peer,
                    ref file,
                    ref caption,
                } => {
                    assert_eq!(peer, "+15551234567");
                    assert_eq!(file, &PathBuf::from("/tmp/x.jpg"));
                    assert_eq!(caption.as_deref(), Some("my image"));
                }
                _ => panic!("expected SendKind::Image"),
            },
            _ => panic!("expected Command::Send"),
        }
    }

    #[test]
    fn cli_parses_send_reaction() {
        let c = Cli::try_parse_from([
            "octo-whatsapp",
            "send",
            "reaction",
            "+15551234567",
            "msg-1",
            "--emoji",
            ":heart:",
        ])
        .unwrap();
        match c.command {
            Command::Send(args) => match args.kind {
                SendKind::Reaction {
                    ref peer,
                    ref msg_id,
                    ref emoji,
                } => {
                    assert_eq!(peer, "+15551234567");
                    assert_eq!(msg_id, "msg-1");
                    assert_eq!(emoji, ":heart:");
                }
                _ => panic!("expected SendKind::Reaction"),
            },
            _ => panic!("expected Command::Send"),
        }
    }

    #[test]
    fn cli_parses_send_poll_multi() {
        let c = Cli::try_parse_from([
            "octo-whatsapp",
            "send",
            "poll",
            "+15551234567",
            "--question",
            "yes or no?",
            "--options",
            "yes,no",
            "--multi",
        ])
        .unwrap();
        match c.command {
            Command::Send(args) => match args.kind {
                SendKind::Poll {
                    ref peer,
                    ref question,
                    ref options,
                    multi,
                } => {
                    assert_eq!(peer, "+15551234567");
                    assert_eq!(question, "yes or no?");
                    assert_eq!(options, &vec!["yes", "no"]);
                    assert!(multi);
                }
                _ => panic!("expected SendKind::Poll"),
            },
            _ => panic!("expected Command::Send"),
        }
    }

    #[test]
    fn cli_parses_send_location() {
        // Negative longitude needs `=` syntax (clap's leading-`-` guard):
        // `--lon=-122.4194`. This is the canonical way operators invoke it.
        let c = Cli::try_parse_from([
            "octo-whatsapp",
            "send",
            "location",
            "+15551234567",
            "--lat",
            "37.7749",
            "--lon=-122.4194",
            "--name",
            "SF",
        ])
        .unwrap();
        match c.command {
            Command::Send(args) => match args.kind {
                SendKind::Location {
                    ref peer,
                    lat,
                    lon,
                    ref name,
                } => {
                    assert_eq!(peer, "+15551234567");
                    assert!((lat - 37.7749).abs() < 1e-6);
                    assert!((lon - -122.4194).abs() < 1e-6);
                    assert_eq!(name, "SF");
                }
                _ => panic!("expected SendKind::Location"),
            },
            _ => panic!("expected Command::Send"),
        }
    }

    #[test]
    fn cli_parses_send_delete() {
        let c = Cli::try_parse_from([
            "octo-whatsapp",
            "send",
            "delete",
            "+15551234567",
            "msg-1",
            "--msg-timestamp",
            "1700000000",
        ])
        .unwrap();
        match c.command {
            Command::Send(args) => match args.kind {
                SendKind::Delete {
                    ref peer,
                    ref msg_id,
                    msg_timestamp,
                } => {
                    assert_eq!(peer, "+15551234567");
                    assert_eq!(msg_id, "msg-1");
                    assert_eq!(msg_timestamp, 1_700_000_000);
                }
                _ => panic!("expected SendKind::Delete"),
            },
            _ => panic!("expected Command::Send"),
        }
    }

    #[test]
    fn cli_global_name_flag_default() {
        let c = Cli::try_parse_from(["octo-whatsapp", "version"]).unwrap();
        assert_eq!(c.name, "default", "name default is 'default'");
    }

    #[test]
    fn cli_global_socket_flag_optional() {
        let c = Cli::try_parse_from(["octo-whatsapp", "version"]).unwrap();
        assert!(c.socket.is_none(), "socket defaults to None");
    }

    // ---- print_result: smoke tests for both branches ----

    /// `print_result(as_json=true)` round-trips through `serde_json::to_string_pretty`,
    /// independent of value shape. We exercise both scalar and object cases because
    /// the JSON branch is unconditional (no fallback).
    #[test]
    fn print_result_json_scalar_object_object() {
        let v = serde_json::json!({"hello": "world"});
        print_result(true, &v).expect("json path must succeed for object");
    }

    #[test]
    fn print_result_human_readable_scalar() {
        // Human-readable branches: scalars (Null/Bool/Number/String) print
        // the bare value; objects/arrays fall back to pretty JSON. Cover both.
        print_result(false, &serde_json::Value::Null).expect("print Null");
        print_result(false, &serde_json::json!(true)).expect("print bool");
        print_result(false, &serde_json::json!(42)).expect("print number");
        print_result(false, &serde_json::json!("hi")).expect("print string");
    }

    #[test]
    fn print_result_human_readable_object_falls_back_to_pretty() {
        let v = serde_json::json!({"status": "ok", "n": 1});
        print_result(false, &v).expect("object/array fallback must succeed");
    }

    // ---- onboard_passthrough_message (pure print, no socket) ----

    #[test]
    fn onboard_passthrough_message_runs_for_all_variants() {
        // The Onboard dispatcher delegates to this pure function — we verify
        // it succeeds for every shape of args without ever touching a socket.
        onboard_passthrough_message("qr-link", &["--timeout=120"]).expect("qr-link");
        onboard_passthrough_message("pair-link", &["+15551234567"]).expect("pair-link");
        onboard_passthrough_message("whoami", &[]).expect("whoami");
        onboard_passthrough_message("session", &["list"]).expect("session list");
        onboard_passthrough_message("session", &["verify", "name"]).expect("session verify");
        onboard_passthrough_message("session", &["remove", "name"]).expect("session remove");
    }

    // ---- dispatch_onboard (pure, daemon-free) ----

    /// The only dispatcher that does NOT touch the daemon socket. We invoke it
    /// directly with all five `OnboardAction` variants to validate the
    /// passthrough wiring without needing a live daemon.
    #[test]
    fn dispatch_onboard_runs_all_variants_without_daemon() {
        let cli = cli_with(None, "default");
        let cmds = vec![
            OnboardCmd {
                action: OnboardAction::QrLink { timeout: 60 },
            },
            OnboardCmd {
                action: OnboardAction::PairLink { phone: "+1".into() },
            },
            OnboardCmd {
                action: OnboardAction::Whoami,
            },
            OnboardCmd {
                action: OnboardAction::Session {
                    action: SessionCmd::List,
                },
            },
            OnboardCmd {
                action: OnboardAction::Session {
                    action: SessionCmd::Verify { name: "x".into() },
                },
            },
            OnboardCmd {
                action: OnboardAction::Session {
                    action: SessionCmd::Remove { name: "x".into() },
                },
            },
        ];
        for cmd in cmds {
            dispatch_onboard(&cli, &cmd).expect("onboard passthrough must succeed");
        }
    }

    // ---- rpc client error path (no socket) ----

    #[test]
    fn rpc_client_debug_includes_socket_path() {
        // The Debug impl intentionally surfaces `socket_path` so failures in
        // production can be diagnosed from `eprintln!({err:?})`. Lock that in.
        let c = RpcClient::new(PathBuf::from("/tmp/octo-cli-test.sock"));
        let s = format!("{c:?}");
        assert!(s.contains("/tmp/octo-cli-test.sock"), "got: {s}");
    }

    #[test]
    fn rpc_client_call_unreachable_socket_mentions_hint() {
        // Repros the existing `rpc_client_call_reports_socket_unreachable`
        // test with a more specific connection-refused scenario to ensure the
        // friendly hint always appears, even when the path is well-formed.
        let c = RpcClient::new(PathBuf::from("/this/path/does/not/exist.sock"));
        let err = c.call("version.get", serde_json::Value::Null).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("is the daemon running"),
            "expected operator hint in error, got: {msg}"
        );
    }
}
