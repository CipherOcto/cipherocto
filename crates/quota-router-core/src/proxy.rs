use crate::balance::Balance;
use crate::keys::{generate_key_id, generate_key_string, ApiKey, KeyType, KeyUpdates};
use crate::providers::Provider;
use crate::storage::{KeyStorage, StoolapKeyStorage};
use http::{HeaderMap, Method, Request, Uri};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Response, StatusCode};
use hyper_util::rt::TokioIo;
use parking_lot::Mutex;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;

pub struct ProxyServer {
    balance: Arc<Mutex<Balance>>,
    provider: Provider,
    port: u16,
    key_storage: Option<Arc<StoolapKeyStorage>>,
}

impl ProxyServer {
    pub fn new(balance: Balance, provider: Provider, port: u16) -> Self {
        Self {
            balance: Arc::new(Mutex::new(balance)),
            provider,
            port,
            key_storage: None,
        }
    }

    pub fn with_key_storage(mut self, storage: StoolapKeyStorage) -> Self {
        self.key_storage = Some(Arc::new(storage));
        self
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        let listener = TcpListener::bind(addr).await?;

        info!("Proxy server listening on http://{}", addr);

        let balance = Arc::clone(&self.balance);
        let provider = self.provider.clone();
        let key_storage = self.key_storage.clone();

        tokio::spawn(async move {
            let balance = Arc::clone(&balance);
            let provider = provider.clone();

            while let Ok((stream, _)) = listener.accept().await {
                let balance = Arc::clone(&balance);
                let provider = provider.clone();
                let key_storage = key_storage.clone();

                tokio::spawn(async move {
                    let io = TokioIo::new(stream);

                    if let Err(err) = http1::Builder::new()
                        .serve_connection(
                            io,
                            service_fn(move |req| {
                                let balance = Arc::clone(&balance);
                                let provider = provider.clone();
                                let key_storage = key_storage.clone();
                                async move {
                                    Ok::<_, Infallible>(handle_request(
                                        req,
                                        &balance,
                                        &provider,
                                        key_storage.as_ref(),
                                    ))
                                }
                            }),
                        )
                        .await
                    {
                        eprintln!("Error serving connection: {}", err);
                    }
                });
            }
        })
        .await?;

        Ok(())
    }
}

fn handle_request<B>(
    req: Request<B>,
    balance: &Arc<Mutex<Balance>>,
    provider: &Provider,
    key_storage: Option<&Arc<StoolapKeyStorage>>,
) -> Response<String> {
    let uri = req.uri();
    let path = uri.path();
    let method = req.method();

    // Key management routes
    if let Some(storage) = key_storage {
        // POST /api/keys - create key
        if method == Method::POST && path == "/api/keys" {
            return handle_create_key(storage);
        }
        // GET /api/keys - list keys
        if method == Method::GET && path == "/api/keys" {
            return handle_list_keys(storage, None);
        }
        // GET /api/keys?team_id=xxx - list keys by team
        if method == Method::GET && path.starts_with("/api/keys") {
            return handle_list_keys(storage, extract_query_param(uri, "team_id"));
        }
        // PUT /api/keys/:id - update key
        if method == Method::PUT && path.starts_with("/api/keys/") {
            let key_id = path.trim_start_matches("/api/keys/");
            if !key_id.is_empty() && !key_id.contains('/') {
                return handle_update_key(storage, key_id);
            }
        }
        // DELETE /api/keys/:id - revoke key (RFC: DELETE /key/{key_id})
        if method == Method::DELETE && path.starts_with("/api/keys/") {
            let key_id = path.trim_start_matches("/api/keys/");
            if !key_id.is_empty() && !key_id.contains('/') {
                return handle_revoke_key(storage, key_id);
            }
        }
        // POST /api/keys/:id/rotate - rotate key
        if method == Method::POST && path.contains("/api/keys/") && path.contains("/rotate") {
            if let Some(key_id) = extract_key_id_from_path(path, "/rotate") {
                return handle_rotate_key(storage, key_id);
            }
        }

        // Team routes
        // POST /api/team - create team
        if method == Method::POST && path == "/api/team" {
            return handle_create_team(storage);
        }
        // GET /api/team/:team_id - get team info
        if method == Method::GET && path.starts_with("/api/team/") {
            let team_id = path.trim_start_matches("/api/team/");
            if !team_id.is_empty() && !team_id.contains('/') {
                return handle_get_team(storage, team_id);
            }
        }
        // PUT /api/team/:team_id - update team
        if method == Method::PUT && path.starts_with("/api/team/") {
            let team_id = path.trim_start_matches("/api/team/");
            if !team_id.is_empty() && !team_id.contains('/') {
                return handle_update_team(storage, team_id);
            }
        }

        // GET /api/key/info - LiteLLM-compatible key info from token
        // Extracts key from Authorization header and returns key info
        if method == Method::GET && path == "/api/key/info" {
            return handle_get_key_info(storage, req.headers());
        }
    }

    // Check balance for proxy requests
    {
        let bal = balance.lock();
        if bal.check(1).is_err() {
            return Response::builder()
                .status(StatusCode::PAYMENT_REQUIRED)
                .body("Insufficient OCTO-W balance".to_string())
                .unwrap();
        }
    }

    // Get API key from environment
    let api_key = match provider.get_api_key() {
        Some(key) => key,
        None => {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body("API key not set in environment".to_string())
                .unwrap();
        }
    };

    // Deduct balance
    {
        let mut bal = balance.lock();
        bal.deduct(1);
    }

    // Forward request to provider (simplified - just return success for MVE)
    info!(
        "Request forwarded with API key: {}...",
        &api_key[..8.min(api_key.len())]
    );

    Response::builder()
        .status(StatusCode::OK)
        .body("Request forwarded successfully".to_string())
        .unwrap()
}

fn handle_create_key(storage: &StoolapKeyStorage) -> Response<String> {
    let key_string = generate_key_string();
    let key_id = generate_key_id();
    let key_hash = crate::keys::compute_key_hash(&key_string);

    let api_key = ApiKey {
        key_id: key_id.clone(),
        key_hash: key_hash.to_vec(),
        key_prefix: key_string.chars().take(7).collect(),
        team_id: None,
        budget_limit: 1000,
        rpm_limit: Some(60),
        tpm_limit: Some(1000),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
        expires_at: None,
        revoked: false,
        revoked_at: None,
        revoked_by: None,
        revocation_reason: None,
        key_type: KeyType::Default,
        allowed_routes: None,
        auto_rotate: false,
        rotation_interval_days: None,
        description: None,
        metadata: None,
    };

    if let Err(e) = storage.create_key(&api_key) {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(format!("Failed to create key: {}", e))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::CREATED)
        .body(
            serde_json::json!({
                "key_id": key_id,
                "key": key_string,
                "budget_limit": api_key.budget_limit,
                "rpm_limit": api_key.rpm_limit,
                "tpm_limit": api_key.tpm_limit,
            })
            .to_string(),
        )
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

fn extract_query_param<'a>(uri: &'a Uri, param: &str) -> Option<&'a str> {
    uri.query().and_then(|query| {
        query
            .split('&')
            .find(|p| p.starts_with(&format!("{}=", param)))
            .and_then(|p| p.split('=').nth(1))
    })
}

fn extract_key_id_from_path<'a>(path: &'a str, suffix: &str) -> Option<&'a str> {
    let without_suffix = path.trim_end_matches(suffix);
    without_suffix.strip_prefix("/api/keys/")
}

fn handle_update_key(storage: &StoolapKeyStorage, key_id: &str) -> Response<String> {
    let updates = KeyUpdates {
        budget_limit: Some(1000), // Default update for now
        rpm_limit: Some(60),
        tpm_limit: Some(1000),
        expires_at: None,
        revoked: None,
        revoked_by: None,
        revocation_reason: None,
        key_type: None,
        description: None,
    };

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

fn handle_revoke_key(storage: &StoolapKeyStorage, key_id: &str) -> Response<String> {
    let updates = KeyUpdates {
        budget_limit: None,
        rpm_limit: None,
        tpm_limit: None,
        expires_at: None,
        revoked: Some(true),
        revoked_by: Some("api".to_string()),
        revocation_reason: Some("Revoked via API".to_string()),
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

fn handle_rotate_key(storage: &StoolapKeyStorage, key_id: &str) -> Response<String> {
    // Generate new key
    let new_key_string = generate_key_string();
    let new_key_id = generate_key_id();
    let new_key_hash = crate::keys::compute_key_hash(&new_key_string);

    let new_api_key = ApiKey {
        key_id: new_key_id.clone(),
        key_hash: new_key_hash.to_vec(),
        key_prefix: new_key_string.chars().take(7).collect(),
        team_id: None,
        budget_limit: 1000,
        rpm_limit: Some(60),
        tpm_limit: Some(1000),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
        expires_at: None,
        revoked: false,
        revoked_at: None,
        revoked_by: None,
        revocation_reason: None,
        key_type: KeyType::Default,
        allowed_routes: None,
        auto_rotate: false,
        rotation_interval_days: None,
        description: None,
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

fn handle_create_team(_storage: &StoolapKeyStorage) -> Response<String> {
    // For team creation we need request body parsing, but we don't have a full HTTP body reader
    // For now, create a placeholder team - in full implementation this would parse JSON body
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .body("Team creation requires JSON body: {\"team_id\": ..., \"name\": ..., \"budget_limit\": ...}".to_string())
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

fn handle_update_team(_storage: &StoolapKeyStorage, _team_id: &str) -> Response<String> {
    // For team update we need request body parsing
    // For now, return error indicating full implementation needed
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .body("Team update requires JSON body: {\"name\": ..., \"budget_limit\": ...}".to_string())
        .unwrap()
}

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
    let key_hash = crate::keys::compute_key_hash(key_string);

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
