use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::gateway::{GatewayClass, GatewayIdentity};
use octo_network::dot::{BroadcastDomainId, PlatformType};
use octo_network::gdp::identity::GdpGatewayIdentity;
use octo_network::gdp::overlay_endpoint::OverlayEndpoint;
use octo_network::gdp::types::GatewayCapability;
use octo_network::sync::{
    GossipDispatcher, SyncDgpHandler, SyncNetworkBridge,
    SYNC_SNAPSHOT_OBJECT_TYPE,
};
use octo_sync::adapter::DatabaseSyncAdapter;
use octo_sync::config::{SyncConfig, SyncRole};
use octo_sync::identity::SyncPeerId;
use octo_sync::session::SyncSessionManager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use octo_transport::discovery::TransportDiscovery;

#[derive(Parser)]
#[command(name = "stoolap-node")]
#[command(about = "Minimal Stoolap node for L4 cross-process E2E sync tests")]
struct Args {
    #[arg(short, long)]
    dsn: String,
    #[arg(short, long)]
    listen: u16,
    #[arg(short = 'p', long = "peer")]
    peers: Vec<String>,
    #[arg(
        long,
        default_value = "abcd000000000000000000000000000000000000000000000000000000000000"
    )]
    mission_id: String,
    #[arg(
        long,
        default_value = "0100000000000000000000000000000000000000000000000000000000000000"
    )]
    node_id: String,
    #[arg(long, default_value = "0")]
    commit: usize,
    #[arg(long)]
    status_file: Option<String>,
    /// Artificial delay (ms) when applying each WAL entry (for backpressure testing).
    #[arg(long, default_value = "0")]
    slow_apply_ms: u64,
    /// Platform adapter to load (e.g., "p2p", "webhook", "quic"). Can be repeated.
    #[arg(long = "adapter")]
    adapters: Vec<String>,
    /// Directories to scan for adapter plugin `.so` files.
    #[arg(long = "adapter-dir")]
    adapter_dirs: Vec<String>,
}

fn parse_hex32(s: &str) -> [u8; 32] {
    let bytes = hex::decode(s).expect("invalid hex");
    assert_eq!(bytes.len(), 32);
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    arr
}

fn adapter_name_to_platform_type(name: &str) -> Option<PlatformType> {
    PlatformType::from_name(name)
}

/// Peer ID sentinel for the transport-based outbound subscriber.
const TRANSPORT_PEER_ID: [u8; 32] = [0xFE; 32];

fn make_gdp_identity(node_id: [u8; 32], network_id: u32) -> GdpGatewayIdentity {
    let base = GatewayIdentity::new(node_id, network_id, GatewayClass::Edge, 1);
    GdpGatewayIdentity::new(base)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "stoolap_node=info".parse().unwrap()),
        )
        .init();

    let args = Args::parse();
    let mission_id = parse_hex32(&args.mission_id);
    let node_id = parse_hex32(&args.node_id);

    let sync_config = stoolap::sync_adapter::SyncConfig::new(mission_id, node_id);
    let (db, adapter) = stoolap::Database::open_with_sync(&args.dsn, sync_config)?;

    if args.commit > 0 {
        tracing::info!(count = args.commit, "committing rows on startup");
        db.execute(
            "CREATE TABLE IF NOT EXISTS sync_test (id INTEGER PRIMARY KEY, data TEXT)",
            (),
        )
        .expect("failed to create table");
        for i in 0..args.commit {
            let sql = format!(
                "INSERT INTO sync_test (id, data) VALUES ({}, 'row-{}')",
                i, i
            );
            db.execute(&sql, ()).expect("failed to insert row");
        }
        tracing::info!(
            lsn = adapter.current_lsn().unwrap_or(0),
            "committed rows"
        );
    }

    tracing::info!(listen = %args.listen, peers = ?args.peers, "stoolap-node starting");

    let adapter_arc: Arc<dyn DatabaseSyncAdapter> = adapter;

    // Create SyncSessionManager for the transport path
    let session_config = SyncConfig::new(mission_id, SyncRole::Replicator, vec![0x01; 32]);
    let session = Arc::new(SyncSessionManager::new(
        adapter_arc.clone(),
        session_config,
        &node_id,
    )?);

    // Shared discovery state for TCP advertisement exchange
    let gdp_identity = make_gdp_identity(node_id, 1);
    let discovery = Arc::new(Mutex::new(TransportDiscovery::new(
        gdp_identity,
        mission_id,
        256,
    )));

    // Load platform adapters and wire transport when --adapter is provided
    let transport_peer = SyncPeerId(TRANSPORT_PEER_ID);
    let mut bg_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    let transport_opt: Option<Arc<octo_transport::NodeTransport>> = if !args.adapters.is_empty() {
        let plugin_dirs: Vec<std::path::PathBuf> = args
            .adapter_dirs
            .iter()
            .map(std::path::PathBuf::from)
            .collect();
        let mut registry =
            octo_network::dot::adapters::registry::AdapterRegistry::new(plugin_dirs);
        if let Err(e) = registry.discover_and_load() {
            tracing::warn!(
                errors = ?e,
                "adapter plugin load errors (continuing with built-in adapters)"
            );
        }

        let requested: Vec<PlatformType> = args
            .adapters
            .iter()
            .filter_map(|name| adapter_name_to_platform_type(name))
            .collect();

        let domain = BroadcastDomainId::new(PlatformType::NativeP2P, &args.node_id);

        let adapter_refs: Vec<(Arc<dyn PlatformAdapter>, BroadcastDomainId)> = registry
            .drain()
            .into_iter()
            .filter(|(_, entry)| {
                entry.health
                    != octo_network::dot::adapters::registry::AdapterHealth::Unhealthy
            })
            .filter(|(pt, _)| {
                if let Some(platform_type) = PlatformType::from_u16(*pt) {
                    requested.iter().any(|r| r.name() == platform_type.name())
                } else {
                    false
                }
            })
            .map(|(_pt, entry)| {
                let adapter: Arc<dyn PlatformAdapter> = Arc::from(entry.adapter);
                (adapter, domain)
            })
            .collect();

        if !adapter_refs.is_empty() {
            tracing::info!(adapters = adapter_refs.len(), "transport adapters loaded");

            let senders: Vec<Arc<dyn octo_transport::sender::NetworkSender>> = adapter_refs
                .iter()
                .map(|(adapter, domain)| {
                    Arc::new(octo_transport::adapter_bridge::PlatformAdapterBridge::new(
                        adapter.clone(),
                        *domain,
                    )) as Arc<dyn octo_transport::sender::NetworkSender>
                })
                .collect();

            let transport = Arc::new(octo_transport::NodeTransport::new(senders));

            // Build local GDP advertisement from transport capabilities
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let adv = {
                let disc = discovery.lock().unwrap();
                disc.build_advertisement(&transport, 1, now)
            };
            tracing::info!(
                gateway_id = hex::encode(adv.gateway_id),
                endpoints = adv.overlay_endpoints.len(),
                "built GDP advertisement"
            );

            // --- Outbound: subscribe transport peer, spawn drain task ---
            session.subscribe_peer(transport_peer).unwrap();
            let session_clone = session.clone();
            let transport_clone = transport.clone();
            let mission_id_clone = mission_id;
            let drain_handle = tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(50));
                let send_ctx = octo_transport::sender::SendContext {
                    mission_id: mission_id_clone,
                    priority: 0,
                    source_peer: node_id,
                    origin_gateway: node_id,
                };
                loop {
                    interval.tick().await;
                    let chunks = session_clone.streamer().drain_outbox(&transport_peer);
                    for chunk in &chunks {
                        let encoded = chunk.encode();
                        match transport_clone.send_best(&encoded, &send_ctx).await {
                            Ok(()) => {
                                tracing::debug!(
                                    from = chunk.from_lsn,
                                    to = chunk.to_lsn,
                                    entries = chunk.entries.len(),
                                    "transport send_best WAL chunk"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "transport send_best failed");
                            }
                        }
                    }
                }
            });

            bg_handles.push(drain_handle);

            // --- Inbound: GossipDispatcher -> SyncNetworkBridge -> session ---
            let handler = Arc::new(SyncDgpHandler::new(session.clone()));
            let sync_bridge = SyncNetworkBridge::new(mission_id, handler.clone());
            let dispatcher = Arc::new(GossipDispatcher::new().with_sync(sync_bridge));

            let adapters_for_receive: Vec<Arc<dyn PlatformAdapter>> =
                adapter_refs.iter().map(|(a, _)| a.clone()).collect();
            let dispatcher_clone = dispatcher;
            let receive_handle = tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
                loop {
                    interval.tick().await;
                    for adapter in &adapters_for_receive {
                        let pt = adapter.platform_type();
                        let domain = BroadcastDomainId::new(pt, &hex::encode(node_id));
                        match adapter.receive_messages(&domain).await {
                            Ok(messages) => {
                                for msg in messages {
                                    match adapter.canonicalize(&msg) {
                                        Ok(_envelope) => {
                                            let peer_id: [u8; 32] = {
                                                let mut id = [0u8; 32];
                                                let src = msg.platform_id.as_bytes();
                                                let len = src.len().min(32);
                                                id[..len].copy_from_slice(&src[..len]);
                                                id
                                            };
                                            match dispatcher_clone.on_gossip_object(
                                                SYNC_SNAPSHOT_OBJECT_TYPE,
                                                0xB1,
                                                peer_id,
                                                msg.payload,
                                            ) {
                                                Ok(()) => {
                                                    tracing::debug!(
                                                        peer = ?peer_id,
                                                        "inbound: dispatched through GossipDispatcher"
                                                    );
                                                }
                                                Err(e) => {
                                                    tracing::debug!(
                                                        peer = ?peer_id,
                                                        error = %e,
                                                        "inbound: dispatch failed"
                                                    );
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            tracing::debug!(
                                                peer = ?msg.platform_id,
                                                error = %e,
                                                "inbound: canonicalize failed"
                                            );
                                        }
                                    }
                                }
                            }
                            Err(_e) => {}
                        }
                    }
                }
            });
            bg_handles.push(receive_handle);

            // --- Periodic tick: heartbeat timeouts, peer state transitions ---
            let session_tick = session.clone();
            let tick_handle = tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
                loop {
                    interval.tick().await;
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let actions = session_tick.tick(now);
                    for action in actions {
                        tracing::debug!(?action, "tick action");
                    }
                }
            });
            bg_handles.push(tick_handle);

            // --- PoRelay trust score feed: registry → sync peer scoring ---
            use octo_network::porelay::registry::TrustRegistry;
            use octo_network::porelay::score::RelayScore;
            let trust_registry = Arc::new(Mutex::new(TrustRegistry::new(100)));
            {
                // Bootstrap: register any currently-known peers with default scores
                let mut reg = trust_registry.lock().unwrap();
                for (peer_id, _state) in session.peer_states() {
                    reg.update_score(RelayScore {
                        gateway_id: peer_id.0,
                        epoch: 1,
                        forwarding_score: 500,
                        availability_score: 500,
                        bandwidth_score: 500,
                        uptime_score: 500,
                        diversity_bonus: 0,
                        stake_multiplier: 1000,
                        composite: 0,
                    });
                    reg.scores.get_mut(&peer_id.0).unwrap().compute_composite();
                }
            }
            let session_porelay = session.clone();
            let registry_porelay = trust_registry.clone();
            let porelay_handle = tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    let reg = registry_porelay.lock().unwrap();
                    let updated = reg.feed_sync_session(&session_porelay);
                    if updated > 0 {
                        tracing::debug!(updated, "PoRelay trust scores synced to session");
                    }
                }
            });
            bg_handles.push(porelay_handle);

            tracing::info!("transport inbound receive loop + tick + porelay feed started");
            Some(transport)
        } else {
            None
        }
    } else {
        None
    };

    // TCP sync path (default, backward-compatible)
    let listener = TcpListener::bind(format!("0.0.0.0:{}", args.listen)).await?;
    let adapter_for_accept = adapter_arc.clone();
    let discovery_for_accept = discovery.clone();
    let accept_handle = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    tracing::info!(peer = %addr, "accepted connection");
                    let adapter = adapter_for_accept.clone();
                    let disc = discovery_for_accept.clone();
                    tokio::spawn(async move {
                        if let Err(e) =
                            serve_writer(stream, adapter, disc).await
                        {
                            tracing::error!(peer = %addr, error = %e, "connection error");
                        }
                    });
                }
                Err(e) => tracing::error!(error = %e, "accept error"),
            }
        }
    });

    let mut peer_handles = Vec::new();
    for peer_addr in &args.peers {
        let peer = peer_addr.clone();
        let adapter = adapter_arc.clone();
        let db_ref = db.clone();
        let status_file = args.status_file.clone();
        let slow_apply_ms = args.slow_apply_ms;
        let disc = discovery.clone();
        let sess = session.clone();
        let handle = tokio::spawn(async move {
            match TcpStream::connect(&peer).await {
                Ok(stream) => {
                    tracing::info!(peer = %peer, "connected to peer");
                    if let Err(e) =
                        serve_reader(stream, adapter, db_ref, status_file, slow_apply_ms, disc, sess)
                            .await
                    {
                        tracing::error!(peer = %peer, error = %e, "peer error");
                    }
                }
                Err(e) => tracing::error!(peer = %peer, error = %e, "failed to connect"),
            }
        });
        peer_handles.push(handle);
    }

    let _ = transport_opt;

    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down");
    for h in bg_handles {
        h.abort();
    }
    accept_handle.abort();
    for h in peer_handles {
        h.abort();
    }
    Ok(())
}

/// Wire protocol handshake: exchange peer identities and transport capabilities.
///
/// Format: `[32-byte gateway_id][2-byte num_transport_types][transport_types...][2-byte num_capabilities][capabilities...]`
/// Length-prefixed with a 4-byte LE u32. Length=0 means no transport configured.
/// This handshake is ALWAYS exchanged (both sides), even when no transport is loaded.
async fn exchange_advertisements(
    stream: &mut TcpStream,
    discovery: &Arc<Mutex<TransportDiscovery>>,
    now: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let local_adv = {
        let disc = discovery.lock().unwrap();
        disc.build_advertisement_from_identity(now)
    };

    let mut local_buf = Vec::new();
    local_buf.extend_from_slice(&local_adv.gateway_id);
    let transport_types: Vec<u16> = local_adv
        .overlay_endpoints
        .iter()
        .map(|ep| ep.transport_type)
        .collect();
    local_buf.extend_from_slice(&(transport_types.len() as u16).to_le_bytes());
    for tt in &transport_types {
        local_buf.extend_from_slice(&tt.to_le_bytes());
    }
    let capabilities: Vec<u16> = local_adv
        .overlay_endpoints
        .iter()
        .map(|ep| ep.flags as u16)
        .collect();
    local_buf.extend_from_slice(&(capabilities.len() as u16).to_le_bytes());
    for cap in &capabilities {
        local_buf.extend_from_slice(&cap.to_le_bytes());
    }

    let len = local_buf.len() as u32;
    stream.write_all(&len.to_le_bytes()).await?;
    stream.write_all(&local_buf).await?;
    stream.flush().await?;

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let peer_len = u32::from_le_bytes(len_buf) as usize;
    const MAX_ADVERTISEMENT_SIZE: usize = 4096;
    if peer_len >= 34 && peer_len <= MAX_ADVERTISEMENT_SIZE {
        let mut peer_bytes = vec![0u8; peer_len];
        stream.read_exact(&mut peer_bytes).await?;

        let mut peer_gw_id = [0u8; 32];
        peer_gw_id.copy_from_slice(&peer_bytes[..32]);
        let mut off = 32;

        let num_tt = u16::from_le_bytes(peer_bytes[off..off + 2].try_into().unwrap()) as usize;
        off += 2;
        let mut endpoints = Vec::new();
        for _ in 0..num_tt {
            if off + 2 > peer_len {
                break;
            }
            let tt = u16::from_le_bytes(peer_bytes[off..off + 2].try_into().unwrap());
            off += 2;
            endpoints.push(OverlayEndpoint {
                transport_type: tt,
                endpoint_hash: [0u8; 32],
                priority: 100,
                bandwidth_class: 0,
                flags: 0,
            });
        }

        let num_caps = if off + 2 <= peer_len {
            let n = u16::from_le_bytes(peer_bytes[off..off + 2].try_into().unwrap()) as usize;
            off += 2;
            // Validate: num_caps u16 values require num_caps * 2 bytes
            if off + n * 2 <= peer_len {
                n
            } else {
                0
            }
        } else {
            0
        };

        let caps: Vec<GatewayCapability> = (0..num_caps).map(|_| GatewayCapability::Relay).collect();

        let entry = octo_network::gdp::cache::GatewayCacheEntry {
            advertisement_hash: blake3::hash(&peer_bytes).into(),
            first_seen: now,
            last_seen: now,
            trust_score: 500,
            identity: octo_network::dot::gateway::GatewayIdentity {
                gateway_id: peer_gw_id,
                public_key: peer_gw_id,
                network_id: 1,
                gateway_class: GatewayClass::Edge,
                creation_epoch: now,
                supported_platforms: 0,
                capabilities: 0,
            },
            capabilities: caps,
            endpoints,
        };
        discovery.lock().unwrap().cache_insert(entry, now);
        tracing::info!(
            peer_gateway = hex::encode(peer_gw_id),
            "registered peer via TCP advertisement exchange"
        );
    }
    Ok(())
}

async fn serve_writer(
    mut stream: TcpStream,
    adapter: Arc<dyn DatabaseSyncAdapter>,
    discovery: Arc<Mutex<TransportDiscovery>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Handshake: always exchange advertisements (mandatory, prevents protocol desync)
    exchange_advertisements(&mut stream, &discovery, now).await?;

    let mut lsn_buf = [0u8; 8];
    stream.read_exact(&mut lsn_buf).await?;
    let request_lsn = u64::from_le_bytes(lsn_buf);
    tracing::info!(request_lsn, "peer requested WAL from");

    let current = adapter.current_lsn()?;
    if current > request_lsn {
        let entries = adapter.read_wal_range(request_lsn + 1, current)?;
        tracing::info!(
            from = request_lsn + 1,
            to = current,
            count = entries.len(),
            "sending initial WAL batch"
        );
        for entry in &entries {
            let mut frame = Vec::with_capacity(1 + entry.len());
            frame.push(0x01);
            frame.extend_from_slice(entry);
            write_frame(&mut stream, &frame).await?;
        }
    }
    write_frame(&mut stream, &[0x03]).await?;

    let mut last_lsn = current;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let current = adapter.current_lsn()?;
        if current <= last_lsn {
            continue;
        }
        let entries = adapter.read_wal_range(last_lsn + 1, current)?;
        tracing::debug!(
            from = last_lsn + 1,
            to = current,
            count = entries.len(),
            "sending incremental WAL"
        );
        for entry in &entries {
            let mut frame = Vec::with_capacity(1 + entry.len());
            frame.push(0x01);
            frame.extend_from_slice(entry);
            write_frame(&mut stream, &frame).await?;
        }
        write_frame(&mut stream, &[0x03]).await?;
        last_lsn = current;
    }
}

async fn serve_reader(
    mut stream: TcpStream,
    adapter: Arc<dyn DatabaseSyncAdapter>,
    db: stoolap::Database,
    status_file: Option<String>,
    slow_apply_ms: u64,
    discovery: Arc<Mutex<TransportDiscovery>>,
    session: Arc<SyncSessionManager>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last_lsn = adapter.current_lsn()?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Handshake: always exchange advertisements (mandatory, prevents protocol desync)
    exchange_advertisements(&mut stream, &discovery, now).await?;

    stream.write_all(&last_lsn.to_le_bytes()).await?;
    stream.flush().await?;
    tracing::info!(last_lsn, "sent request_lsn to writer");

    // Auto-subscribe the writer peer for WAL tail streaming
    {
        let disc = discovery.lock().unwrap();
        for (gw_id, entry) in disc.cache_entries() {
            let _ = session.subscribe_peer(SyncPeerId(gw_id));
            tracing::debug!(
                peer = hex::encode(gw_id),
                endpoints = entry.endpoints.len(),
                "auto-subscribed discovered peer"
            );
        }
    }

    loop {
        let len = match read_u32(&mut stream).await {
            Some(l) => l as usize,
            None => {
                tracing::info!("writer closed connection");
                break;
            }
        };
        if len == 0 || len > 16 * 1024 * 1024 {
            break;
        }

        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).await?;

        match payload[0] {
            0x01 => {
                if slow_apply_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(slow_apply_ms)).await;
                }
                match adapter.apply_wal_entry(&payload[1..]) {
                    Ok(()) => tracing::debug!("applied WAL entry"),
                    Err(e) => tracing::warn!(error = %e, "failed to apply WAL entry"),
                }
            }
            0x03 => {
                last_lsn = adapter.current_lsn()?;
                tracing::debug!(last_lsn, "batch complete");
                if let Some(ref path) = status_file {
                    let count: i64 = db
                        .query_one("SELECT COUNT(*) FROM sync_test", ())
                        .unwrap_or(-1);
                    let _ = std::fs::write(path, count.to_string());
                    tracing::info!(count, "wrote status file");
                }
            }
            other => {
                tracing::warn!(msg_type = other, "unknown message type");
            }
        }
    }
    Ok(())
}

async fn read_u32(stream: &mut TcpStream) -> Option<u32> {
    let mut buf = [0u8; 4];
    match stream.read_exact(&mut buf).await {
        Ok(_) => Some(u32::from_be_bytes(buf)),
        Err(_) => None,
    }
}

async fn write_frame(stream: &mut TcpStream, data: &[u8]) -> Result<(), std::io::Error> {
    let len = data.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(data).await?;
    stream.flush().await?;
    Ok(())
}
