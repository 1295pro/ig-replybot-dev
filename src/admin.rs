use axum::{
    Json, Router,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post, put},
};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::Sha256;
use std::{
    env,
    net::SocketAddr,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    config::Config,
    deploy::{self, ConnectionInput, InstallInput, PreflightInput},
    meta_api::MetaApi,
    storage::DeploymentTarget,
    webhook::AppState,
};

type HmacSha256 = Hmac<Sha256>;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/api/admin/access", get(access))
        .route("/api/admin/status", get(status))
        .route("/api/admin/connect", post(connect))
        .route("/api/admin/settings", put(save_settings))
        .route("/api/admin/automation", post(set_automation))
        .route("/api/admin/deploy/fingerprint", post(deploy_fingerprint))
        .route("/api/admin/deploy/preflight", post(deploy_preflight))
        .route("/api/admin/deploy/install", post(deploy_install))
}

async fn index() -> impl IntoResponse {
    let mut response = Html(include_str!("../static/admin.html")).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("valid cache header"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        "nosniff".parse().expect("valid nosniff header"),
    );
    response.headers_mut().insert(
        header::X_FRAME_OPTIONS,
        "DENY".parse().expect("valid frame header"),
    );
    response
}

async fn access(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config();
    Json(json!({
        "admin_password_configured": !config.admin_password.is_empty(),
        "vps_deploy_available": config.bind_addr.ip().is_loopback(),
    }))
}

#[derive(Deserialize)]
struct ConnectInput {
    access_token: String,
    api_version: String,
}

#[derive(Deserialize)]
struct SettingsInput {
    app_secret: String,
    webhook_verify_token: String,
    keyword: String,
    public_reply_text: String,
    private_reply_text: String,
    poll_interval_seconds: u64,
    poll_enabled: bool,
}

#[derive(Deserialize)]
struct AutomationInput {
    enabled: bool,
}

#[derive(Deserialize)]
struct FingerprintInput {
    connection: ConnectionInput,
}

async fn deploy_fingerprint(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(input): Json<FingerprintInput>,
) -> Response {
    if let Err(error) = authorize_local_deploy(&state, &headers, peer) {
        return error_response(StatusCode::FORBIDDEN, error);
    }
    match deploy::inspect_host(&input.connection).await {
        Ok(fingerprint) => {
            (StatusCode::OK, Json(json!({ "fingerprint": fingerprint }))).into_response()
        }
        Err(error) => error_response(StatusCode::BAD_GATEWAY, error),
    }
}

async fn deploy_preflight(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(input): Json<PreflightInput>,
) -> Response {
    if let Err(error) = authorize_local_deploy(&state, &headers, peer) {
        return error_response(StatusCode::FORBIDDEN, error);
    }
    let config = state.config();
    if let Err(error) = deploy::ensure_connected_account(&config) {
        return error_response(StatusCode::CONFLICT, error);
    }
    let target = deployment_target(&input.connection);
    match deploy::check_server(input, &config).await {
        Ok(info) => {
            if info.existing_bot.is_some() {
                if state.config().ig_user_id != config.ig_user_id {
                    return error_response(
                        StatusCode::CONFLICT,
                        "The local Instagram account changed. Run Check server again".to_owned(),
                    );
                }
                save_deployment_target(&state, &target);
            } else if !info.existing_bot_detected
                && info.port_available
                && let Err(error) = deploy::ensure_deployable_config(&state.config())
            {
                return error_response(StatusCode::CONFLICT, error);
            }
            (StatusCode::OK, Json(info)).into_response()
        }
        Err(error) => error_response(StatusCode::BAD_GATEWAY, error),
    }
}

async fn deploy_install(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(input): Json<InstallInput>,
) -> Response {
    if let Err(error) = authorize_local_deploy(&state, &headers, peer) {
        return error_response(StatusCode::FORBIDDEN, error);
    }
    let config = state.config();
    if let Err(error) = deploy::ensure_deployable_config(&config) {
        return error_response(StatusCode::CONFLICT, error);
    }
    let target = deployment_target(&input.connection);
    match deploy::deploy(input, &config).await {
        Ok(result) => {
            save_deployment_target(&state, &target);
            (StatusCode::OK, Json(result)).into_response()
        }
        Err(error) => error_response(StatusCode::BAD_GATEWAY, error),
    }
}

fn deployment_target(connection: &ConnectionInput) -> DeploymentTarget {
    DeploymentTarget {
        host: connection.host.trim().to_owned(),
        ssh_port: connection.ssh_port,
        service_port: connection.service_port,
    }
}

fn save_deployment_target(state: &AppState, target: &DeploymentTarget) {
    if let Err(error) = state.database().save_deployment_target(target) {
        eprintln!("VPS check succeeded, but its address could not be saved locally: {error}");
    }
}

fn authorize_local_deploy(
    state: &AppState,
    headers: &HeaderMap,
    peer: SocketAddr,
) -> Result<(), String> {
    authorize(state, headers)?;
    if !state.config().bind_addr.ip().is_loopback() {
        return Err(
            "VPS deployment is enabled only when this control room listens on localhost".to_owned(),
        );
    }
    if !peer.ip().is_loopback() {
        return Err(
            "VPS deployment is available only from this computer or an SSH tunnel".to_owned(),
        );
    }
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            "Open the control room at a localhost address before deploying".to_owned()
        })?;
    if !is_local_deploy_origin(origin, state.config().bind_addr.port()) {
        return Err(
            "VPS deployment is allowed only from the localhost control room, not through a public proxy".to_owned(),
        );
    }
    Ok(())
}

fn is_local_deploy_origin(origin: &str, port: u16) -> bool {
    let loopback_hosts = ["127.0.0.1", "localhost", "[::1]"];
    loopback_hosts.iter().any(|host| {
        let with_port = format!("http://{host}:{port}");
        origin.eq_ignore_ascii_case(&with_port)
            || (port == 80 && origin.eq_ignore_ascii_case(&format!("http://{host}")))
    })
}

async fn status(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(error) = authorize(&state, &headers) {
        return error_response(StatusCode::UNAUTHORIZED, error);
    }
    let config = state.config();
    let queue = match state.database().queue_summary() {
        Ok(queue) => queue,
        Err(error) => return error_response(StatusCode::SERVICE_UNAVAILABLE, error),
    };
    let recent_activity = match state.database().recent_activity(8) {
        Ok(activity) => activity,
        Err(error) => return error_response(StatusCode::SERVICE_UNAVAILABLE, error),
    };
    let settings_saved = match state.database().has_saved_settings() {
        Ok(saved) => saved,
        Err(error) => return error_response(StatusCode::SERVICE_UNAVAILABLE, error),
    };
    let deployment = match state.database().load_deployment_target() {
        Ok(target) => target,
        Err(error) => return error_response(StatusCode::SERVICE_UNAVAILABLE, error),
    };
    let last_poll = state.last_poll_success();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let polling_state = if !config.automation_enabled || !config.poll_enabled {
        "off"
    } else if last_poll == 0 {
        "starting"
    } else if now.saturating_sub(last_poll) > config.poll_interval_seconds * 3 + 30 {
        "stale"
    } else {
        "healthy"
    };
    let connected = config.ig_access_token.is_some() && !config.ig_username.is_empty();
    let live_test_passed = match config.ig_user_id.as_deref() {
        Some(account_id) => match state
            .database()
            .has_completed_delivery_for_account(account_id)
        {
            Ok(passed) => passed,
            Err(error) => return error_response(StatusCode::SERVICE_UNAVAILABLE, error),
        },
        None => false,
    };
    let rules_configured = rules_configured(&config, connected, settings_saved, live_test_passed);
    let deployment = env::var("IG_DEPLOY_HOST")
        .ok()
        .filter(|host| !host.trim().is_empty())
        .map(|host| DeploymentTarget {
            host,
            ssh_port: env::var("IG_DEPLOY_SSH_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(22),
            service_port: env::var("IG_DEPLOY_SERVICE_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(12950),
        })
        .or(deployment);
    let configured = json!({
        "connected": connected,
        "app_secret": !config.app_secret.is_empty(),
        "webhook": !config.app_secret.is_empty() && !config.webhook_verify_token.is_empty(),
    });
    let body = json!({
        "status": if config.automation_enabled { "active" } else { "paused" },
        "automation_enabled": config.automation_enabled,
        "poll_enabled": config.poll_enabled,
        "poll_interval_seconds": config.poll_interval_seconds,
        "polling_state": polling_state,
        "last_poll_success": if last_poll == 0 { Value::Null } else { json!(last_poll) },
        "account_id": config.ig_user_id,
        "account_username": if config.ig_username.is_empty() { Value::Null } else { json!(config.ig_username) },
        "configured": configured,
        "keyword": config.keyword,
        "public_reply_text": config.public_reply_text,
        "private_reply_text": config.private_reply_text,
        "queue": queue,
        "recent_activity": recent_activity,
        "settings_saved": settings_saved,
        "rules_configured": rules_configured,
        "live_test_passed": live_test_passed,
        "deployment": deployment,
    });
    (StatusCode::OK, Json(body)).into_response()
}

fn rules_configured(
    config: &Config,
    connected: bool,
    settings_saved: bool,
    live_test_passed: bool,
) -> bool {
    settings_saved
        || (connected
            && (config.automation_enabled || live_test_passed)
            && !config.keyword.trim().is_empty()
            && !config.public_reply_text.trim().is_empty()
            && !config.private_reply_text.trim().is_empty())
}

async fn connect(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ConnectInput>,
) -> Response {
    if let Err(error) = authorize(&state, &headers) {
        return error_response(StatusCode::UNAUTHORIZED, error);
    }
    let version = normalize_version(&input.api_version);
    let identity = match MetaApi::inspect_identity(&input.access_token, &version).await {
        Ok(identity) => identity,
        Err(error) => return error_response(StatusCode::BAD_GATEWAY, error),
    };

    let mut config = (*state.config()).clone();
    config.ig_access_token = Some(input.access_token);
    config.ig_user_id = Some(identity.id);
    config.ig_username = identity.username.clone();
    config.api_version = version;
    if let Err(error) = save_and_update(&state, config) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    (
        StatusCode::OK,
        Json(json!({ "connected": true, "username": identity.username })),
    )
        .into_response()
}

async fn save_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SettingsInput>,
) -> Response {
    if let Err(error) = authorize(&state, &headers) {
        return error_response(StatusCode::UNAUTHORIZED, error);
    }
    let keyword = input.keyword.trim();
    let public_reply_text = input.public_reply_text.trim();
    let private_reply_text = input.private_reply_text.trim();
    if keyword.is_empty() || keyword.chars().count() > 80 {
        return error_response(
            StatusCode::BAD_REQUEST,
            "Keyword must contain 1 to 80 characters".to_owned(),
        );
    }
    if public_reply_text.is_empty()
        || private_reply_text.is_empty()
        || public_reply_text.chars().count() > 1000
        || private_reply_text.chars().count() > 1000
    {
        return error_response(
            StatusCode::BAD_REQUEST,
            "Replies must contain 1 to 1000 characters".to_owned(),
        );
    }
    if !(30..=3600).contains(&input.poll_interval_seconds) {
        return error_response(
            StatusCode::BAD_REQUEST,
            "Polling interval must be between 30 and 3600 seconds".to_owned(),
        );
    }

    let mut config = (*state.config()).clone();
    if !input.app_secret.trim().is_empty() {
        config.app_secret = input.app_secret.trim().to_owned();
    }
    if !input.webhook_verify_token.trim().is_empty() {
        config.webhook_verify_token = input.webhook_verify_token.trim().to_owned();
    }
    config.keyword = keyword.to_owned();
    config.public_reply_text = public_reply_text.to_owned();
    config.private_reply_text = private_reply_text.to_owned();
    config.poll_interval_seconds = input.poll_interval_seconds;
    config.poll_enabled = input.poll_enabled;
    if config.automation_enabled
        && !config.poll_enabled
        && (config.app_secret.is_empty() || config.webhook_verify_token.is_empty())
    {
        return error_response(
            StatusCode::CONFLICT,
            "Enable polling or configure a signed Meta webhook before saving this change"
                .to_owned(),
        );
    }
    if let Err(error) = save_and_update(&state, config) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    (StatusCode::OK, Json(json!({ "saved": true }))).into_response()
}

async fn set_automation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AutomationInput>,
) -> Response {
    if let Err(error) = authorize(&state, &headers) {
        return error_response(StatusCode::UNAUTHORIZED, error);
    }
    let mut config = (*state.config()).clone();
    if input.enabled {
        if config.ig_access_token.is_none() || config.ig_user_id.is_none() {
            return error_response(
                StatusCode::CONFLICT,
                "Connect an Instagram Professional account first".to_owned(),
            );
        }
        if !config.poll_enabled
            && (config.app_secret.is_empty() || config.webhook_verify_token.is_empty())
        {
            return error_response(
                StatusCode::CONFLICT,
                "Enable polling or configure a signed Meta webhook before enabling automation"
                    .to_owned(),
            );
        }
        let api = match MetaApi::new(&config) {
            Ok(api) => api,
            Err(error) => return error_response(StatusCode::BAD_REQUEST, error),
        };
        match api.identity().await {
            Ok(identity) => {
                config.ig_user_id = Some(identity.id);
                config.ig_username = identity.username;
            }
            Err(error) => return error_response(StatusCode::BAD_GATEWAY, error),
        }
    }
    config.automation_enabled = input.enabled;
    if let Err(error) = save_and_update(&state, config) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    (
        StatusCode::OK,
        Json(json!({ "automation_enabled": input.enabled })),
    )
        .into_response()
}

fn save_and_update(state: &AppState, config: Config) -> Result<(), String> {
    state.database().save_settings(&config)?;
    state.update_config(config);
    Ok(())
}

fn authorize(state: &AppState, headers: &HeaderMap) -> Result<(), String> {
    let config = state.config();
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return Err("Enter the admin password to continue".to_owned());
    };
    let Some(password) = value.strip_prefix("Bearer ") else {
        return Err("Admin access denied".to_owned());
    };
    if config.admin_password.is_empty() || !password_matches(password, &config.admin_password) {
        return Err("Admin access denied".to_owned());
    }
    Ok(())
}

fn password_matches(candidate: &str, expected: &str) -> bool {
    let Ok(mut expected_mac) = HmacSha256::new_from_slice(b"ig-reply-bot-admin-v1") else {
        return false;
    };
    expected_mac.update(expected.as_bytes());
    let digest = expected_mac.finalize().into_bytes();
    let Ok(mut candidate_mac) = HmacSha256::new_from_slice(b"ig-reply-bot-admin-v1") else {
        return false;
    };
    candidate_mac.update(candidate.as_bytes());
    candidate_mac.verify_slice(&digest).is_ok()
}

fn normalize_version(value: &str) -> String {
    let version = value.trim();
    if version.starts_with('v') {
        version.to_owned()
    } else {
        format!("v{version}")
    }
}

fn error_response(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

#[cfg(test)]
mod deploy_access_tests {
    use super::is_local_deploy_origin;

    #[test]
    fn deployment_origin_accepts_only_localhost_and_the_bound_port() {
        assert!(is_local_deploy_origin("http://127.0.0.1:3000", 3000));
        assert!(is_local_deploy_origin("http://localhost:3000", 3000));
        assert!(is_local_deploy_origin("http://[::1]:3000", 3000));
        assert!(!is_local_deploy_origin("https://bot.example.com", 3000));
        assert!(!is_local_deploy_origin("http://127.0.0.1:3001", 3000));
    }

    #[test]
    fn deployment_origin_handles_the_default_http_port() {
        assert!(is_local_deploy_origin("http://localhost", 80));
        assert!(is_local_deploy_origin("http://127.0.0.1:80", 80));
        assert!(!is_local_deploy_origin("http://localhost:81", 80));
    }
}
