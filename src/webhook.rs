use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::Value;
use sha2::Sha256;
use tokio::sync::watch;

use crate::{
    config::Config,
    deploy::VpsTunnel,
    domain::keyword::is_exact_keyword,
    storage::{Database, DeploymentTarget},
};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct AppState {
    config: watch::Sender<Arc<Config>>,
    config_updates: watch::Receiver<Arc<Config>>,
    database: Arc<Database>,
    poll_last_success: Arc<AtomicU64>,
    vps_tunnel: Arc<Mutex<Option<(DeploymentTarget, VpsTunnel)>>>,
}

impl AppState {
    pub fn new(config: Arc<Config>, database: Arc<Database>) -> Self {
        let (config, config_updates) = watch::channel(config);
        Self {
            config,
            config_updates,
            database,
            poll_last_success: Arc::new(AtomicU64::new(0)),
            vps_tunnel: Arc::new(Mutex::new(None)),
        }
    }

    pub fn poll_health(&self) -> Arc<AtomicU64> {
        self.poll_last_success.clone()
    }

    pub fn last_poll_success(&self) -> u64 {
        self.poll_last_success.load(Ordering::Relaxed)
    }

    pub fn database(&self) -> &Arc<Database> {
        &self.database
    }

    pub fn config(&self) -> Arc<Config> {
        self.config_updates.borrow().clone()
    }

    pub fn config_updates(&self) -> watch::Receiver<Arc<Config>> {
        self.config_updates.clone()
    }

    pub fn update_config(&self, config: Config) {
        if self.config().ig_user_id != config.ig_user_id
            && let Ok(mut active) = self.vps_tunnel.lock()
        {
            active.take();
        }
        self.config.send_replace(Arc::new(config));
    }

    pub fn set_vps_tunnel(
        &self,
        target: DeploymentTarget,
        tunnel: VpsTunnel,
    ) -> Result<Option<String>, String> {
        let mut active = self
            .vps_tunnel
            .lock()
            .map_err(|_| "Could not save the VPS connection".to_owned())?;
        let url = tunnel.url();
        *active = Some((target, tunnel));
        Ok(url)
    }

    pub fn vps_panel_url(&self, target: &DeploymentTarget) -> Option<String> {
        let active = self.vps_tunnel.lock().ok()?;
        let (current, tunnel) = active.as_ref()?;
        (current == target).then(|| tunnel.url()).flatten()
    }
}

#[derive(Debug)]
pub struct MatchedComment {
    pub account_id: String,
    pub comment_id: String,
    pub media_id: String,
    pub commenter_id: String,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(ready))
        .route("/webhook", get(verify).post(receive))
        .merge(crate::admin::router())
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .with_state(state)
}

async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config();
    if state.database.health_check().is_err() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "status": "not_ready" })),
        );
    }
    if config.automation_enabled && config.poll_enabled {
        let last = state.poll_last_success.load(Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs());
        if last == 0 || now.saturating_sub(last) > config.poll_interval_seconds * 3 + 30 {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "status": "not_ready", "polling": "stale" })),
            );
        }
    }

    let automation = if config.automation_enabled {
        "enabled"
    } else {
        "disabled"
    };
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ready",
            "automation": automation,
            "polling": config.automation_enabled && config.poll_enabled
        })),
    )
}

async fn verify(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let config = state.config();
    let valid = query
        .get("hub.mode")
        .is_some_and(|mode| mode == "subscribe")
        && query.get("hub.verify_token").is_some_and(|token| {
            !config.webhook_verify_token.is_empty() && token == &config.webhook_verify_token
        });

    if valid {
        match query.get("hub.challenge") {
            Some(challenge) => (StatusCode::OK, challenge.clone()).into_response(),
            None => (StatusCode::BAD_REQUEST, "missing challenge").into_response(),
        }
    } else {
        (StatusCode::FORBIDDEN, "verification failed").into_response()
    }
}

async fn receive(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let config = state.config();
    if config.app_secret.is_empty() || !valid_signature(&headers, &body, &config.app_secret) {
        return (StatusCode::UNAUTHORIZED, "invalid signature").into_response();
    }

    // A signed webhook can still arrive while sending is disabled. Acknowledge
    // it without queuing work, so enabling automation later cannot replay old
    // comments unexpectedly.
    if !config.automation_enabled {
        return StatusCode::OK.into_response();
    }

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid JSON").into_response(),
    };

    let mut inserted = 0usize;
    let Some(expected_account_id) = config.ig_user_id.as_deref() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Instagram account is not configured",
        )
            .into_response();
    };
    let matches = matching_comments(
        &payload,
        &config.keyword,
        expected_account_id,
        &config.ig_username,
    );
    if matches.is_empty() {
        log_ignored_webhook(&payload, &config.keyword, expected_account_id);
    }
    for comment in matches {
        match state.database.insert_matched(&comment) {
            Ok(true) => inserted += 1,
            Ok(false) => {}
            Err(_) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, "could not persist event")
                    .into_response();
            }
        }
    }

    if inserted > 0 {
        eprintln!("stored {inserted} matching Instagram comment event(s)");
    }
    StatusCode::OK.into_response()
}

fn log_ignored_webhook(payload: &Value, keyword: &str, expected_account_id: &str) {
    let is_instagram = payload.get("object").and_then(Value::as_str) == Some("instagram");
    let entries = payload.get("entry").and_then(Value::as_array);
    let entry_count = entries.map_or(0, Vec::len);
    let account_matches = entries.map_or(0, |entries| {
        entries
            .iter()
            .filter(|entry| entry.get("id").and_then(Value::as_str) == Some(expected_account_id))
            .count()
    });
    let mut comment_changes = 0;
    let mut keyword_matches = 0;
    if let Some(entries) = entries {
        for entry in entries {
            if let Some(changes) = entry.get("changes").and_then(Value::as_array) {
                for change in changes {
                    if change.get("field").and_then(Value::as_str) == Some("comments") {
                        comment_changes += 1;
                        if change
                            .get("value")
                            .and_then(|value| value.get("text"))
                            .and_then(Value::as_str)
                            .is_some_and(|text| is_exact_keyword(text, keyword))
                        {
                            keyword_matches += 1;
                        }
                    }
                }
            }
        }
    }
    eprintln!(
        "signed webhook ignored: instagram={is_instagram} entries={entry_count} matching_account_entries={account_matches} comment_changes={comment_changes} matching_keywords={keyword_matches}"
    );
}

fn valid_signature(headers: &HeaderMap, body: &[u8], app_secret: &str) -> bool {
    let Some(signature) = headers
        .get("x-hub-signature-256")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("sha256="))
        .and_then(|value| hex::decode(value).ok())
    else {
        return false;
    };

    let Ok(mut mac) = HmacSha256::new_from_slice(app_secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&signature).is_ok()
}

fn matching_comments(
    payload: &Value,
    keyword: &str,
    expected_account_id: &str,
    expected_username: &str,
) -> Vec<MatchedComment> {
    if payload.get("object").and_then(Value::as_str) != Some("instagram") {
        return Vec::new();
    }

    let Some(entries) = payload.get("entry").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut matches = Vec::new();

    for entry in entries {
        let Some(account_id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        if account_id != expected_account_id {
            continue;
        }
        let Some(changes) = entry.get("changes").and_then(Value::as_array) else {
            continue;
        };

        for change in changes {
            if change.get("field").and_then(Value::as_str) != Some("comments") {
                continue;
            }
            let Some(value) = change.get("value") else {
                continue;
            };
            // Explicit edit/remove events must never initiate an automation.
            if value
                .get("verb")
                .and_then(Value::as_str)
                .is_some_and(|verb| verb != "add")
            {
                continue;
            }
            let Some(comment_id) = value.get("id").and_then(Value::as_str) else {
                continue;
            };
            // This ID is interpolated into a Graph API path. Accept Meta's
            // numeric IDs only, with a generous bound to reject malformed
            // webhook data before constructing a URL.
            if !valid_graph_id(comment_id) {
                continue;
            }
            let Some(text) = value.get("text").and_then(Value::as_str) else {
                continue;
            };
            let Some(commenter_id) = value
                .get("from")
                .and_then(|from| from.get("id"))
                .and_then(Value::as_str)
            else {
                continue;
            };
            let commenter_username = value
                .get("from")
                .and_then(|from| from.get("username"))
                .and_then(Value::as_str);
            if commenter_id == account_id
                || (commenter_username == Some(expected_username) && !expected_username.is_empty())
                || !is_exact_keyword(text, keyword)
            {
                continue;
            }
            let media_id = value.get("media_id").and_then(Value::as_str).or_else(|| {
                value
                    .get("media")
                    .and_then(|media| media.get("id"))
                    .and_then(Value::as_str)
            });
            let Some(media_id) = media_id else {
                continue;
            };

            matches.push(MatchedComment {
                account_id: account_id.to_owned(),
                comment_id: comment_id.to_owned(),
                media_id: media_id.to_owned(),
                commenter_id: commenter_id.to_owned(),
            });
        }
    }

    matches
}

fn valid_graph_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::{AppState, matching_comments, ready, receive, valid_signature};
    use crate::{config::Config, storage::Database};
    use axum::{
        body::Bytes,
        extract::State,
        http::{HeaderMap, HeaderValue},
        response::IntoResponse,
    };
    use hmac::{Hmac, KeyInit, Mac};
    use serde_json::json;
    use sha2::Sha256;
    use std::{
        path::PathBuf,
        sync::{Arc, atomic::Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    type HmacSha256 = Hmac<Sha256>;

    fn signature(secret: &str, body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn accepts_only_a_valid_sha256_webhook_signature() {
        let body = br#"{"object":"instagram"}"#;
        let valid = signature("test-secret", body);
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-hub-signature-256",
            HeaderValue::from_bytes(valid.as_bytes()).unwrap(),
        );
        assert!(valid_signature(&headers, body, "test-secret"));
        assert!(!valid_signature(&headers, body, "different-secret"));

        headers.insert(
            "x-hub-signature-256",
            HeaderValue::from_static("sha256=not-hex"),
        );
        assert!(!valid_signature(&headers, body, "test-secret"));
        headers.remove("x-hub-signature-256");
        assert!(!valid_signature(&headers, body, "test-secret"));
    }

    #[tokio::test]
    async fn signed_events_are_acknowledged_but_not_queued_when_automation_is_disabled() {
        let secret = "test-secret";
        let body = Bytes::from(
            r#"{"object":"instagram","entry":[{"id":"123","changes":[{"field":"comments","value":{"id":"456","verb":"add","text":"бот","from":{"id":"789"},"media":{"id":"999"}}}]}]}"#,
        );
        let config = Arc::new(Config {
            bind_addr: "127.0.0.1:3000".parse().unwrap(),
            webhook_verify_token: "verify-token".into(),
            app_secret: secret.into(),
            admin_password: "test-admin-password-long-enough-for-tests".into(),
            database_path: PathBuf::from(":memory:"),
            keyword: "бот".into(),
            automation_enabled: false,
            poll_enabled: false,
            poll_interval_seconds: 60,
            ig_user_id: Some("123".into()),
            ig_username: String::new(),
            ig_access_token: None,
            api_version: "v26.0".into(),
            public_reply_text: "reply".into(),
            private_reply_text: "message".into(),
        });
        let database = Arc::new(Database::open(&config.database_path).unwrap());
        let state = AppState::new(config, database.clone());
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-hub-signature-256",
            HeaderValue::from_bytes(signature(secret, &body).as_bytes()).unwrap(),
        );

        let response = receive(State(state), headers, body).await.into_response();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert!(database.claim_next().unwrap().is_none());
    }

    #[tokio::test]
    async fn readiness_fails_when_comment_polling_is_stale() {
        let config = Arc::new(Config {
            bind_addr: "127.0.0.1:3000".parse().unwrap(),
            webhook_verify_token: "verify-token".into(),
            app_secret: "test-secret".into(),
            admin_password: "test-admin-password-long-enough-for-tests".into(),
            database_path: PathBuf::from(":memory:"),
            keyword: "бот".into(),
            automation_enabled: true,
            poll_enabled: true,
            poll_interval_seconds: 60,
            ig_user_id: Some("123".into()),
            ig_username: "owner".into(),
            ig_access_token: Some("test-token".into()),
            api_version: "v26.0".into(),
            public_reply_text: "reply".into(),
            private_reply_text: "message".into(),
        });
        let database = Arc::new(Database::open(&config.database_path).unwrap());
        let state = AppState::new(config, database);
        let response = ready(State(state.clone())).await.into_response();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        );

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        state.poll_health().store(now, Ordering::Relaxed);
        let response = ready(State(state)).await.into_response();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    #[test]
    fn queues_only_new_exact_keyword_comments_from_other_users() {
        let payload = json!({
            "object": "instagram",
            "entry": [{
                "id": "account-1",
                "changes": [
                    {"field": "comments", "value": {
                        "id": "111", "verb": "add", "text": "🤖 БОТ!",
                        "from": {"id": "person-1"}, "media": {"id": "media-1"}
                    }},
                    {"field": "comments", "value": {
                        "id": "456?access_token=unexpected", "verb": "add", "text": "бот",
                        "from": {"id": "person-4"}, "media_id": "media-1"
                    }},
                    {"field": "comments", "value": {
                        "id": "phrase", "verb": "add", "text": "хочу такого бота",
                        "from": {"id": "person-2"}, "media_id": "media-1"
                    }},
                    {"field": "comments", "value": {
                        "id": "edit", "verb": "edited", "text": "бот",
                        "from": {"id": "person-3"}, "media_id": "media-1"
                    }},
                    {"field": "comments", "value": {
                        "id": "own-comment", "verb": "add", "text": "бот",
                        "from": {"id": "account-1"}, "media_id": "media-1"
                    }},
                    {"field": "comments", "value": {
                        "id": "222", "verb": "add", "text": "бот",
                        "from": {"id": "different-owner-id", "username": "owner"}, "media_id": "media-1"
                    }}
                ]
            }]
        });

        let matches = matching_comments(&payload, "бот", "account-1", "owner");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].comment_id, "111");
        assert_eq!(matches[0].commenter_id, "person-1");
    }
}
