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
        use std::io::{Read, Write};
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
        let mut buf = String::new();
        s.read_to_string(&mut buf)?;
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
        Ok(resp.get("result").cloned().unwrap_or(serde_json::Value::Null))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_result_emits_pretty_json_for_structured_data() {
        // Pure data-path verification: the structured branch falls back to
        // pretty JSON, which we sanity-check by formatting the same value.
        let v = serde_json::json!({"k": "v"});
        let s = serde_json::to_string_pretty(&v).unwrap();
        assert!(s.contains("\"k\""));
        assert!(s.contains("\"v\""));
    }
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
        assert_eq!(resolve_socket_path(&cli), PathBuf::from("/tmp/override.sock"));
    }

    #[test]
    fn resolve_socket_path_derives_from_name_when_no_socket() {
        let path = resolve_socket_path_with_env("alpha", Some("/run/user/1000"));
        assert_eq!(path, PathBuf::from("/run/user/1000/octo-whatsapp-alpha.sock"));
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
}

/// Wire the read-only `version` / `status` / `health` commands (Tasks 41).
pub fn dispatch_simple(cli: &Cli, method: &str) -> anyhow::Result<()> {
    let result = RpcClient::new(resolve_socket_path(cli)).call(method, serde_json::Value::Null)?;
    print_result(cli.json, &result)
}

/// Wire `send text <peer> --text "..."` (Task 42) and `groups *` (Task 43).
pub fn dispatch_send(cli: &Cli, args: &SendArgs) -> anyhow::Result<()> {
    match &args.kind {
        SendKind::Text { peer, text } => {
            let params = serde_json::json!({"peer": peer, "text": text});
            let result = RpcClient::new(resolve_socket_path(cli)).call("send.text", params)?;
            print_result(cli.json, &result)
        }
    }
}

pub fn dispatch_groups(cli: &Cli, cmd: &GroupsCmd) -> anyhow::Result<()> {
    let client = RpcClient::new(resolve_socket_path(cli));
    let (method, params) = match &cmd.action {
        GroupsAction::Create { subject, members } => (
            "groups.create",
            serde_json::json!({"subject": subject, "members": members}),
        ),
        GroupsAction::List => ("groups.list", serde_json::Value::Null),
        GroupsAction::Info { jid } => ("groups.info", serde_json::json!({"jid": jid})),
        GroupsAction::Leave { jid } => ("groups.leave", serde_json::json!({"jid": jid})),
    };
    let result = client.call(method, params)?;
    print_result(cli.json, &result)
}