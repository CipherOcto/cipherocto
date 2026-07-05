//! CLI for `octo-whatsapp`. Subcommand tree mirrors the RPC surface.
//!
//! Phase 1 wires each top-level command to its corresponding RPC method.
//! The `onboard` subcommand does NOT require a running daemon — it prints a
//! message instructing the user to invoke the standalone `octo-whatsapp-onboard`
//! binary. All other subcommands connect to the daemon socket.
//!
//! See `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-phase1.md` §Part K.

use std::path::PathBuf;

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
    /// Send a text message.
    Send(SendArgs),
    /// Group operations.
    Groups(GroupsCmd),
    /// Message operations.
    Messages(MessagesCmd),
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
        limit: Option<u32>,
    },
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
}

#[derive(Debug, Args)]
pub struct EventsCmd {
    #[command(subcommand)]
    pub action: EventsAction,
}

#[derive(Debug, Subcommand)]
pub enum EventsAction {
    /// List recent events.
    List,
    /// Show a single event by id.
    Show { id: String },
}

#[derive(Debug, Args)]
pub struct OnboardCmd {
    #[command(subcommand)]
    pub action: OnboardAction,
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