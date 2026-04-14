//! Admin API server for key and team management.
//!
//! This module provides the HTTP REST API for managing API keys, teams,
//! and budgets per RFC-0903. It is entirely separate from the proxy
//! server (proxy.rs) which handles LLM request forwarding.
//!
//! ## Architecture
//!
//! - `AdminServer` - HTTP server for admin API
//! - Key management handlers - create, list, update, revoke, rotate keys
//! - Team management handlers - create, get, update teams
//!
//! ## API Routes
//!
//! | Method | Path | Handler |
//! |--------|------|---------|
//! | POST | /key/generate | handle_create_key |
//! | GET | /key/list | handle_list_keys |
//! | PUT | /key/:id | handle_update_key |
//! | DELETE | /key/:id | handle_revoke_key |
//! | POST | /key/:id/regenerate | handle_rotate_key |
//! | POST | /team | handle_create_team |
//! | GET | /team/:team_id | handle_get_team |
//! | PUT | /team/:team_id | handle_update_team |
//! | GET | /key/info | handle_get_key_info |

use crate::keys::{
    check_team_key_limit, compute_key_hash, generate_key_id, generate_key_string, ApiKey,
    CreateTeamRequest, GenerateKeyRequest, GenerateKeyResponse, KeyType, KeyUpdates,
    RevokeKeyRequest, Team, UpdateTeamRequest,
};
use crate::storage::{KeyStorage, StoolapKeyStorage};
use http::{HeaderMap, Request, StatusCode, Uri};
use http_body::Body as HttpBody;
use http_body_util::BodyExt;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::Response;
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;

/// Admin API server for key and team management.
pub struct AdminServer {
    port: u16,
    storage: Arc<StoolapKeyStorage>,
}

impl AdminServer {
    /// Create a new AdminServer with the given storage and port.
    pub fn new(storage: StoolapKeyStorage, port: u16) -> Self {
        Self {
            port,
            storage: Arc::new(storage),
        }
    }

    /// Start the admin server.
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        let listener = TcpListener::bind(addr).await?;

        info!("Admin API server listening on http://{}", addr);

        let storage = Arc::clone(&self.storage);

        tokio::spawn(async move {
            let storage = storage;

            while let Ok((stream, _)) = listener.accept().await {
                let storage = Arc::clone(&storage);

                tokio::spawn(async move {
                    let io = TokioIo::new(stream);

                    if let Err(err) = http1::Builder::new()
                        .serve_connection(
                            io,
                            service_fn(move |req| {
                                let storage = Arc::clone(&storage);
                                async move {
                                    Ok::<_, std::convert::Infallible>(
                                        handle_request(req, storage.as_ref()).await,
                                    )
                                }
                            }),
                        )
                        .await
                    {
                        eprintln!("Error serving admin connection: {}", err);
                    }
                });
            }
        })
        .await?;

        Ok(())
    }
}

/// Handle admin API requests - routes to appropriate handler.
async fn handle_request<B>(req: Request<B>, storage: &StoolapKeyStorage) -> Response<String>
where
    B: HttpBody + Send,
    B::Data: Send,
{
    // Split request into parts and body upfront
    let (parts, body) = req.into_parts();
    let path = parts.uri.path();
    let method_str: &str = parts.method.as_ref();

    // Key routes
    match (method_str, path) {
        // POST /key/generate - create key
        ("POST", "/key/generate") => {
            let bytes = match body.collect().await {
                Ok(b) => b.to_bytes(),
                Err(_) => {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body("Failed to read request body".to_string())
                        .unwrap();
                }
            };
            let req: GenerateKeyRequest = match serde_json::from_slice(&bytes) {
                Ok(r) => r,
                Err(e) => {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(format!("Invalid JSON: {}", e))
                        .unwrap();
                }
            };
            return handle_create_key(storage, &req);
        }

        // GET /key/list - list all keys
        ("GET", "/key/list") => return handle_list_keys(storage, None),

        // GET /key/list?team_id=xxx - list keys by team
        ("GET", p) if p.starts_with("/key/list") => {
            return handle_list_keys(storage, extract_query_param(&parts.uri, "team_id"));
        }

        // PUT /key/:id - update key
        ("PUT", p)
            if p.starts_with("/key/")
                && !p.starts_with("/key/list")
                && !p.contains("/regenerate") =>
        {
            let key_id = p.trim_start_matches("/key/");
            if !key_id.is_empty() && !key_id.contains('/') {
                let bytes = match body.collect().await {
                    Ok(b) => b.to_bytes(),
                    Err(_) => {
                        return Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body("Failed to read request body".to_string())
                            .unwrap();
                    }
                };
                let updates: KeyUpdates = match serde_json::from_slice(&bytes) {
                    Ok(u) => u,
                    Err(e) => {
                        return Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body(format!("Invalid JSON: {}", e))
                            .unwrap();
                    }
                };
                return handle_update_key(storage, key_id, updates);
            }
        }

        // DELETE /key/:id - revoke key
        ("DELETE", p) if p.starts_with("/key/") && !p.contains("/regenerate") => {
            let key_id = p.trim_start_matches("/key/");
            if !key_id.is_empty() && !key_id.contains('/') {
                let bytes = match body.collect().await {
                    Ok(b) => b.to_bytes(),
                    Err(_) => {
                        return Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body("Failed to read request body".to_string())
                            .unwrap();
                    }
                };
                let revoke_req: RevokeKeyRequest = match serde_json::from_slice(&bytes) {
                    Ok(r) => r,
                    Err(_) => {
                        // If no body, use defaults
                        RevokeKeyRequest {
                            revoked_by: Some("api".to_string()),
                            reason: Some("Revoked via API".to_string()),
                        }
                    }
                };
                return handle_revoke_key(storage, key_id, revoke_req);
            }
        }

        // POST /key/:id/regenerate - rotate key
        ("POST", p) if p.starts_with("/key/") && p.contains("/regenerate") => {
            if let Some(key_id) = extract_key_id_from_regenerate_path(p) {
                let bytes = match body.collect().await {
                    Ok(b) => b.to_bytes(),
                    Err(_) => {
                        return Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body("Failed to read request body".to_string())
                            .unwrap();
                    }
                };
                let gen_req: Option<GenerateKeyRequest> = serde_json::from_slice(&bytes).ok();
                return handle_rotate_key(storage, key_id, gen_req);
            }
        }

        // Team routes
        // POST /team - create team
        ("POST", "/team") => {
            let bytes = match body.collect().await {
                Ok(b) => b.to_bytes(),
                Err(_) => {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body("Failed to read request body".to_string())
                        .unwrap();
                }
            };
            let req: CreateTeamRequest = match serde_json::from_slice(&bytes) {
                Ok(r) => r,
                Err(e) => {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(format!("Invalid JSON: {}", e))
                        .unwrap();
                }
            };
            return handle_create_team(storage, req);
        }

        // GET /team/:team_id - get team info
        ("GET", p) if p.starts_with("/team/") => {
            let team_id = p.trim_start_matches("/team/");
            if !team_id.is_empty() && !team_id.contains('/') {
                return handle_get_team(storage, team_id);
            }
        }

        // PUT /team/:team_id - update team
        ("PUT", p) if p.starts_with("/team/") => {
            let team_id = p.trim_start_matches("/team/");
            if !team_id.is_empty() && !team_id.contains('/') {
                let bytes = match body.collect().await {
                    Ok(b) => b.to_bytes(),
                    Err(_) => {
                        return Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body("Failed to read request body".to_string())
                            .unwrap();
                    }
                };
                let update_req: UpdateTeamRequest = match serde_json::from_slice(&bytes) {
                    Ok(u) => u,
                    Err(e) => {
                        return Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body(format!("Invalid JSON: {}", e))
                            .unwrap();
                    }
                };
                return handle_update_team(storage, team_id, update_req);
            }
        }

        // GET /key/info - key info from token
        ("GET", "/key/info") => {
            return handle_get_key_info(storage, &parts.headers);
        }

        _ => {}
    }

    // Not found
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body("Not found".to_string())
        .unwrap()
}

// =============================================================================
// Key management handlers
// =============================================================================

fn handle_create_key(storage: &StoolapKeyStorage, req: &GenerateKeyRequest) -> Response<String> {
    // Check team key limit if team_id is specified
    if let Some(ref team_id) = req.team_id {
        match storage.count_keys_for_team(team_id) {
            Ok(count) => {
                if let Err(e) = check_team_key_limit(count as u32) {
                    return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(format!("Team key limit exceeded: {}", e))
                        .unwrap();
                }
            }
            Err(e) => {
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(format!("Failed to count team keys: {}", e))
                    .unwrap();
            }
        }
    }

    let key_string = generate_key_string();
    let key_id = generate_key_id();
    let key_hash = compute_key_hash(&key_string);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Compute expiration if rotation_interval_days is set
    let expires_at = req
        .rotation_interval_days
        .map(|days| now + (days as i64 * 86400));

    let api_key = ApiKey {
        key_id: key_id.clone(),
        key_hash: key_hash.to_vec(),
        key_prefix: key_string.chars().take(7).collect(),
        team_id: req.team_id.clone(),
        budget_limit: req.budget_limit as i64,
        rpm_limit: req.rpm_limit.map(|r| r as i32),
        tpm_limit: req.tpm_limit.map(|t| t as i32),
        created_at: now,
        expires_at,
        revoked: false,
        revoked_at: None,
        revoked_by: None,
        revocation_reason: None,
        key_type: req.key_type,
        allowed_routes: None,
        auto_rotate: req.auto_rotate.unwrap_or(false),
        rotation_interval_days: req.rotation_interval_days.map(|d| d as i32),
        description: req.description.clone(),
        metadata: req.metadata.as_ref().map(|v| v.to_string()),
    };

    if let Err(e) = storage.create_key(&api_key) {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(format!("Failed to create key: {}", e))
            .unwrap();
    }

    let response = GenerateKeyResponse {
        key: key_string,
        key_id: key_id.clone(),
        expires: expires_at,
        team_id: req.team_id.clone(),
        key_type: req.key_type,
        created_at: now,
    };

    Response::builder()
        .status(StatusCode::CREATED)
        .body(serde_json::to_string(&response).unwrap())
        .unwrap()
}

fn handle_list_keys(storage: &StoolapKeyStorage, team_id: Option<&str>) -> Response<String> {
    let keys: Vec<ApiKey> = match storage.list_keys(team_id) {
        Ok(keys) => keys,
        Err(e) => {
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(format!("Failed to list keys: {}", e))
                .unwrap();
        }
    };

    let keys_json: Vec<serde_json::Value> = keys
        .iter()
        .map(|k| {
            serde_json::json!({
                "key_id": k.key_id,
                "key_prefix": k.key_prefix,
                "team_id": k.team_id,
                "budget_limit": k.budget_limit,
                "rpm_limit": k.rpm_limit,
                "tpm_limit": k.tpm_limit,
                "revoked": k.revoked,
                "expires_at": k.expires_at,
            })
        })
        .collect();

    Response::builder()
        .status(StatusCode::OK)
        .body(serde_json::json!({ "keys": keys_json }).to_string())
        .unwrap()
}

fn handle_update_key(
    storage: &StoolapKeyStorage,
    key_id: &str,
    updates: KeyUpdates,
) -> Response<String> {
    if let Err(e) = storage.update_key(key_id, &updates) {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(format!("Failed to update key: {}", e))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::OK)
        .body(
            serde_json::json!({
                "key_id": key_id,
                "updated": true,
            })
            .to_string(),
        )
        .unwrap()
}

fn handle_revoke_key(
    storage: &StoolapKeyStorage,
    key_id: &str,
    req: RevokeKeyRequest,
) -> Response<String> {
    let updates = KeyUpdates {
        budget_limit: None,
        rpm_limit: None,
        tpm_limit: None,
        expires_at: None,
        revoked: Some(true),
        revoked_by: req.revoked_by,
        revocation_reason: req.reason,
        key_type: None,
        description: None,
    };

    if let Err(e) = storage.update_key(key_id, &updates) {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(format!("Failed to revoke key: {}", e))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::OK)
        .body(
            serde_json::json!({
                "key_id": key_id,
                "revoked": true,
            })
            .to_string(),
        )
        .unwrap()
}

fn handle_rotate_key(
    storage: &StoolapKeyStorage,
    key_id: &str,
    gen_req: Option<GenerateKeyRequest>,
) -> Response<String> {
    // Use provided values or defaults
    let (
        budget_limit,
        rpm_limit,
        tpm_limit,
        team_id,
        key_type,
        auto_rotate,
        rotation_interval_days,
        description,
    ) = if let Some(ref req) = gen_req {
        (
            req.budget_limit as i64,
            req.rpm_limit.map(|r| r as i32),
            req.tpm_limit.map(|t| t as i32),
            req.team_id.clone(),
            req.key_type,
            req.auto_rotate.unwrap_or(false),
            req.rotation_interval_days.map(|d| d as i32),
            req.description.clone(),
        )
    } else {
        (
            1000,
            Some(60),
            Some(1000),
            None,
            KeyType::Default,
            false,
            None,
            None,
        )
    };

    // Generate new key
    let new_key_string = generate_key_string();
    let new_key_id = generate_key_id();
    let new_key_hash = compute_key_hash(&new_key_string);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let expires_at = rotation_interval_days.map(|days| now + (days as i64 * 86400));

    let new_api_key = ApiKey {
        key_id: new_key_id.clone(),
        key_hash: new_key_hash.to_vec(),
        key_prefix: new_key_string.chars().take(7).collect(),
        team_id,
        budget_limit,
        rpm_limit,
        tpm_limit,
        created_at: now,
        expires_at,
        revoked: false,
        revoked_at: None,
        revoked_by: None,
        revocation_reason: None,
        key_type,
        allowed_routes: None,
        auto_rotate,
        rotation_interval_days,
        description,
        metadata: None,
    };

    if let Err(e) = storage.create_key(&new_api_key) {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(format!("Failed to create rotated key: {}", e))
            .unwrap();
    }

    // Revoke old key
    let updates = KeyUpdates {
        budget_limit: None,
        rpm_limit: None,
        tpm_limit: None,
        expires_at: None,
        revoked: Some(true),
        revoked_by: Some("system".to_string()),
        revocation_reason: Some("Rotated".to_string()),
        key_type: None,
        description: None,
    };

    if let Err(e) = storage.update_key(key_id, &updates) {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(format!("Failed to revoke old key: {}", e))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::OK)
        .body(
            serde_json::json!({
                "key_id": key_id,
                "new_key_id": new_key_id,
                "new_key": new_key_string,
                "rotated": true,
            })
            .to_string(),
        )
        .unwrap()
}

// =============================================================================
// Team management handlers
// =============================================================================

fn handle_create_team(storage: &StoolapKeyStorage, req: CreateTeamRequest) -> Response<String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let team = Team {
        team_id: req.team_id.clone(),
        name: req.name,
        budget_limit: req.budget_limit,
        created_at: now,
    };

    if let Err(e) = storage.create_team(&team) {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(format!("Failed to create team: {}", e))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::CREATED)
        .body(
            serde_json::json!({
                "team_id": team.team_id,
                "name": team.name,
                "budget_limit": team.budget_limit,
                "created_at": team.created_at,
            })
            .to_string(),
        )
        .unwrap()
}

fn handle_get_team(storage: &StoolapKeyStorage, team_id: &str) -> Response<String> {
    match storage.get_team(team_id) {
        Ok(Some(team)) => Response::builder()
            .status(StatusCode::OK)
            .body(
                serde_json::json!({
                    "team_id": team.team_id,
                    "name": team.name,
                    "budget_limit": team.budget_limit,
                    "created_at": team.created_at,
                })
                .to_string(),
            )
            .unwrap(),
        Ok(None) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(format!("Team {} not found", team_id))
            .unwrap(),
        Err(e) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(format!("Failed to get team: {}", e))
            .unwrap(),
    }
}

fn handle_update_team(
    storage: &StoolapKeyStorage,
    team_id: &str,
    req: UpdateTeamRequest,
) -> Response<String> {
    let (name, budget_limit) = (req.name.as_deref(), req.budget_limit);

    if name.is_none() && budget_limit.is_none() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("No updates provided".to_string())
            .unwrap();
    }

    // For partial updates, get current team and merge
    let current = match storage.get_team(team_id) {
        Ok(Some(t)) => t,
        Ok(None) => {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(format!("Team {} not found", team_id))
                .unwrap();
        }
        Err(e) => {
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(format!("Failed to get team: {}", e))
                .unwrap();
        }
    };

    let new_name = name.unwrap_or(&current.name);
    let new_budget = budget_limit.unwrap_or(current.budget_limit);

    if let Err(e) = storage.update_team(team_id, new_name, new_budget) {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(format!("Failed to update team: {}", e))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::OK)
        .body(
            serde_json::json!({
                "team_id": team_id,
                "updated": true,
            })
            .to_string(),
        )
        .unwrap()
}

// =============================================================================
// Key info handler
// =============================================================================

fn handle_get_key_info(storage: &StoolapKeyStorage, headers: &HeaderMap) -> Response<String> {
    // Extract key from Authorization header
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let key_string = match auth_header {
        Some(key) => key,
        None => {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body("Missing Authorization header".to_string())
                .unwrap();
        }
    };

    // Hash the key and lookup
    let key_hash = compute_key_hash(key_string);

    match storage.lookup_by_hash(&key_hash) {
        Ok(Some(api_key)) => Response::builder()
            .status(StatusCode::OK)
            .body(
                serde_json::json!({
                    "key_id": api_key.key_id,
                    "key_prefix": api_key.key_prefix,
                    "team_id": api_key.team_id,
                    "budget_limit": api_key.budget_limit,
                    "rpm_limit": api_key.rpm_limit,
                    "tpm_limit": api_key.tpm_limit,
                    "expires_at": api_key.expires_at,
                    "key_type": api_key.key_type.to_string(),
                    "auto_rotate": api_key.auto_rotate,
                })
                .to_string(),
            )
            .unwrap(),
        Ok(None) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body("Key not found or revoked".to_string())
            .unwrap(),
        Err(e) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(format!("Failed to lookup key: {}", e))
            .unwrap(),
    }
}

// =============================================================================
// Helper functions
// =============================================================================

fn extract_query_param<'a>(uri: &'a Uri, param: &str) -> Option<&'a str> {
    uri.query().and_then(|query| {
        query
            .split('&')
            .find(|p| p.starts_with(&format!("{}=", param)))
            .and_then(|p| p.split('=').nth(1))
    })
}

fn extract_key_id_from_regenerate_path(path: &str) -> Option<&str> {
    let without_suffix = path.trim_end_matches("/regenerate");
    without_suffix.strip_prefix("/key/")
}
