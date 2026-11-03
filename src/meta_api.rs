use std::time::Duration;

use reqwest::{Client, StatusCode, redirect::Policy};
use serde_json::{Value, json};

use crate::{
    config::Config,
    storage::{DeliveryAction, DeliveryKind},
};

pub struct MetaApi {
    client: Client,
    account_id: String,
    access_token: String,
    api_version: String,
    public_reply_text: String,
    private_reply_text: String,
    origin: String,
}

pub struct InstagramIdentity {
    pub id: String,
    pub username: String,
}

pub struct ApiResponse {
    pub status: StatusCode,
    pub error: Option<ApiErrorSummary>,
}

pub struct ApiErrorSummary {
    code: Option<i64>,
    subcode: Option<i64>,
    trace_id: Option<String>,
    message: Option<String>,
}

impl ApiErrorSummary {
    pub fn log_summary(&self) -> String {
        let mut details = Vec::new();
        if let Some(code) = self.code {
            details.push(format!("code {code}"));
        }
        if let Some(subcode) = self.subcode {
            details.push(format!("subcode {subcode}"));
        }
        if let Some(trace_id) = &self.trace_id {
            details.push(format!("trace {trace_id}"));
        }
        if let Some(message) = &self.message {
            details.push(message.clone());
        }

        if details.is_empty() {
            "Meta returned an error response".to_owned()
        } else {
            format!("Meta error: {}", details.join("; "))
        }
    }
}

impl MetaApi {
    pub async fn inspect_identity(
        access_token: &str,
        api_version: &str,
    ) -> Result<InstagramIdentity, String> {
        if access_token.trim().is_empty() {
            return Err("Instagram access token must not be empty".to_owned());
        }
        if !valid_api_version(api_version) {
            return Err("API version must look like v26.0 or 26.0".to_owned());
        }
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .redirect(Policy::none())
            .user_agent(concat!("ig-reply-bot/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| format!("create Meta HTTP client: {error}"))?;
        Self::fetch_identity(
            &client,
            "https://graph.instagram.com",
            api_version,
            access_token,
        )
        .await
    }

    pub fn new(config: &Config) -> Result<Self, String> {
        Self::with_origin(config, "https://graph.instagram.com")
    }

    fn with_origin(config: &Config, origin: &str) -> Result<Self, String> {
        let account_id = config
            .ig_user_id
            .as_ref()
            .ok_or_else(|| "IG_USER_ID is required when automation is enabled".to_owned())?;
        if !valid_graph_id(account_id) {
            return Err("IG_USER_ID must contain 1 to 64 ASCII digits".to_owned());
        }
        let access_token = config
            .ig_access_token
            .clone()
            .ok_or_else(|| "IG_ACCESS_TOKEN is required when automation is enabled".to_owned())?;
        if access_token.trim().is_empty() {
            return Err("IG_ACCESS_TOKEN must not be empty".to_owned());
        }
        if !valid_api_version(&config.api_version) {
            return Err("META_API_VERSION must look like v26.0 or 26.0".to_owned());
        }

        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .redirect(Policy::none())
            .user_agent(concat!("ig-reply-bot/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| format!("create Meta HTTP client: {error}"))?;

        Ok(Self {
            client,
            account_id: account_id.clone(),
            access_token,
            api_version: config
                .api_version
                .strip_prefix('v')
                .unwrap_or(&config.api_version)
                .to_owned(),
            public_reply_text: config.public_reply_text.clone(),
            private_reply_text: config.private_reply_text.clone(),
            origin: origin.trim_end_matches('/').to_owned(),
        })
    }

    pub async fn identity(&self) -> Result<InstagramIdentity, String> {
        Self::fetch_identity(
            &self.client,
            &self.origin,
            &format!("v{}", self.api_version),
            &self.access_token,
        )
        .await
    }

    async fn fetch_identity(
        client: &Client,
        origin: &str,
        api_version: &str,
        access_token: &str,
    ) -> Result<InstagramIdentity, String> {
        let version = api_version.strip_prefix('v').unwrap_or(api_version);
        let url = format!(
            "{}/v{version}/me?fields=id,username",
            origin.trim_end_matches('/')
        );
        let response = client
            .get(url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|error| format!("Meta identity request failed: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "Meta rejected the account token with HTTP {}",
                response.status()
            ));
        }
        let body: Value = response
            .json()
            .await
            .map_err(|_| "Meta identity response is invalid JSON".to_owned())?;
        let id = body.get("id").and_then(Value::as_str).unwrap_or_default();
        let username = body
            .get("username")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !valid_graph_id(id) || username.is_empty() {
            return Err("Meta identity response lacks a valid ID or username".to_owned());
        }
        Ok(InstagramIdentity {
            id: id.to_owned(),
            username: username.to_owned(),
        })
    }

    pub async fn send(&self, action: &DeliveryAction) -> Result<ApiResponse, reqwest::Error> {
        let request = match action.kind {
            DeliveryKind::PublicReply => {
                let url = format!(
                    "{}/v{}/{}/replies",
                    self.origin, self.api_version, action.comment_id
                );
                self.client
                    .post(url.clone())
                    .bearer_auth(&self.access_token)
                    .form(&[("message", self.public_reply_text.as_str())])
            }
            DeliveryKind::PrivateReply => {
                let url = format!(
                    "{}/v{}/{}/messages",
                    self.origin, self.api_version, self.account_id
                );
                let body = json!({
                    "recipient": { "comment_id": action.comment_id },
                    "message": { "text": self.private_reply_text }
                });
                self.client
                    .post(url)
                    .bearer_auth(&self.access_token)
                    .json(&body)
            }
        };

        let http_response = self.client.execute(request.build()?).await?;

        let status = http_response.status();
        let error = if status.is_success() {
            None
        } else {
            let body = http_response.json::<Value>().await.unwrap_or(Value::Null);
            summarize_api_error(&body)
        };
        Ok(ApiResponse { status, error })
    }
}

fn valid_graph_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_api_version(value: &str) -> bool {
    let version = value.strip_prefix('v').unwrap_or(value);
    let mut parts = version.split('.');
    let Some(major) = parts.next() else {
        return false;
    };
    let Some(minor) = parts.next() else {
        return false;
    };
    !major.is_empty()
        && !minor.is_empty()
        && major.bytes().all(|byte| byte.is_ascii_digit())
        && minor.bytes().all(|byte| byte.is_ascii_digit())
        && parts.next().is_none()
}

fn summarize_api_error(body: &Value) -> Option<ApiErrorSummary> {
    let error = body.get("error")?;
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .map(|message| safe_log_text(message, 240));
    let trace_id = error
        .get("fbtrace_id")
        .and_then(Value::as_str)
        .map(|trace| safe_log_text(trace, 80));

    Some(ApiErrorSummary {
        code: error.get("code").and_then(Value::as_i64),
        subcode: error.get("error_subcode").and_then(Value::as_i64),
        trace_id,
        message,
    })
}

fn safe_log_text(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(max_chars)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::MetaApi;
    use crate::{
        config::Config,
        storage::{DeliveryAction, DeliveryKind},
    };
    use axum::{
        Json, Router,
        body::Bytes,
        extract::State,
        http::{HeaderMap, Method, Uri},
        response::{IntoResponse, Response},
        routing::{any, get},
    };
    use serde_json::{Value, json};
    use std::{
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    #[derive(Clone, Default)]
    struct CapturedRequests(Arc<Mutex<Vec<CapturedRequest>>>);

    #[derive(Debug)]
    struct CapturedRequest {
        method: Method,
        path: String,
        authorization: Option<String>,
        body: Vec<u8>,
    }

    fn config() -> Config {
        Config {
            bind_addr: "127.0.0.1:3000".parse().unwrap(),
            webhook_verify_token: "verify-token".into(),
            app_secret: "app-secret".into(),
            admin_password: "test-admin-password-long-enough-for-tests".into(),
            database_path: PathBuf::from(":memory:"),
            keyword: "bot".into(),
            automation_enabled: true,
            poll_enabled: false,
            poll_interval_seconds: 60,
            ig_user_id: Some("123456".into()),
            ig_username: "owner".into(),
            ig_access_token: Some("test-access-token".into()),
            api_version: "v26.0".into(),
            public_reply_text: "public reply".into(),
            private_reply_text: "private reply".into(),
        }
    }

    async fn capture_request(
        State(captured): State<CapturedRequests>,
        method: Method,
        uri: Uri,
        headers: HeaderMap,
        body: Bytes,
    ) -> Response {
        captured.0.lock().unwrap().push(CapturedRequest {
            method,
            path: uri.path().to_owned(),
            authorization: headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned),
            body: body.to_vec(),
        });

        if uri.path().ends_with("/messages") {
            (
                axum::http::StatusCode::FORBIDDEN,
                Json(json!({
                    "error": {
                        "message": "Missing instagram_business_manage_messages permission",
                        "type": "OAuthException",
                        "code": 10,
                        "error_subcode": 200,
                        "fbtrace_id": "test-trace-id"
                    }
                })),
            )
                .into_response()
        } else {
            (
                axum::http::StatusCode::OK,
                Json(json!({ "id": "test-reply-id" })),
            )
                .into_response()
        }
    }

    #[tokio::test]
    async fn sends_expected_public_and_private_reply_requests() {
        let captured = CapturedRequests::default();
        let app = Router::new()
            .route("/{version}/{id}/{edge}", any(capture_request))
            .with_state(captured.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let api = MetaApi::with_origin(&config(), &format!("http://{address}")).unwrap();
        let public = api
            .send(&DeliveryAction {
                comment_id: "987654".into(),
                kind: DeliveryKind::PublicReply,
            })
            .await
            .unwrap();
        let private = api
            .send(&DeliveryAction {
                comment_id: "987654".into(),
                kind: DeliveryKind::PrivateReply,
            })
            .await
            .unwrap();
        server.abort();

        assert_eq!(public.status, axum::http::StatusCode::OK);
        assert!(public.error.is_none());
        assert_eq!(private.status, axum::http::StatusCode::FORBIDDEN);
        let api_error = private.error.unwrap();
        assert!(api_error.log_summary().contains("code 10"));
        assert!(
            api_error
                .log_summary()
                .contains("Missing instagram_business_manage_messages permission")
        );

        let requests = captured.0.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].method, Method::POST);
        assert_eq!(requests[0].path, "/v26.0/987654/replies");
        assert_eq!(
            requests[0].authorization.as_deref(),
            Some("Bearer test-access-token")
        );
        assert!(String::from_utf8_lossy(&requests[0].body).contains("message=public+reply"));

        assert_eq!(requests[1].path, "/v26.0/123456/messages");
        assert_eq!(
            requests[1].authorization.as_deref(),
            Some("Bearer test-access-token")
        );
        let private_body: Value = serde_json::from_slice(&requests[1].body).unwrap();
        assert_eq!(
            private_body,
            json!({
                "recipient": { "comment_id": "987654" },
                "message": { "text": "private reply" }
            })
        );
    }

    #[test]
    fn rejects_invalid_account_ids_and_api_versions_at_startup() {
        let mut invalid_account = config();
        invalid_account.ig_user_id = Some("123/456".into());
        assert!(MetaApi::new(&invalid_account).is_err());

        let mut invalid_version = config();
        invalid_version.api_version = "v26.0/anything".into();
        assert!(MetaApi::new(&invalid_version).is_err());
    }

    #[tokio::test]
    async fn reads_identity_from_the_same_token_used_for_replies() {
        let app = Router::new().route(
            "/v26.0/me",
            get(|| async { Json(json!({ "id": "123456", "username": "owner" })) }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let api = MetaApi::with_origin(&config(), &format!("http://{address}")).unwrap();
        let identity = api.identity().await.unwrap();
        server.abort();
        assert_eq!(identity.id, "123456");
        assert_eq!(identity.username, "owner");
    }
}
