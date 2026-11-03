use serde::{Deserialize, Serialize};
use std::{env, net::SocketAddr, path::PathBuf};

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(skip, default = "default_bind_addr")]
    pub bind_addr: SocketAddr,
    pub webhook_verify_token: String,
    pub app_secret: String,
    #[serde(skip)]
    pub admin_password: String,
    #[serde(skip)]
    pub database_path: PathBuf,
    pub keyword: String,
    pub automation_enabled: bool,
    pub poll_enabled: bool,
    pub poll_interval_seconds: u64,
    pub ig_user_id: Option<String>,
    pub ig_username: String,
    pub ig_access_token: Option<String>,
    pub api_version: String,
    pub public_reply_text: String,
    pub private_reply_text: String,
}

fn default_bind_addr() -> SocketAddr {
    "127.0.0.1:3000"
        .parse()
        .expect("valid default listener address")
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let bind_addr_value =
            env::var("IG_REPLY_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_owned());
        let bind_addr = bind_addr_value.parse::<SocketAddr>().map_err(|_| {
            format!("IG_REPLY_BIND_ADDR value {bind_addr_value:?} is not a valid socket address")
        })?;
        let webhook_verify_token = env::var("META_WEBHOOK_VERIFY_TOKEN").unwrap_or_default();
        let app_secret = env::var("META_APP_SECRET").unwrap_or_default();
        let admin_password = env::var("IG_ADMIN_PASSWORD").unwrap_or_default();
        if !admin_password.is_empty() && admin_password.chars().count() < 32 {
            return Err("IG_ADMIN_PASSWORD must contain at least 32 characters".to_owned());
        }
        let database_path = env::var_os("IG_DATABASE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("ig_reply_bot.sqlite3"));
        let keyword = env::var("IG_KEYWORD").unwrap_or_else(|_| "бот".to_owned());
        if keyword.trim().is_empty() {
            return Err("IG_KEYWORD must not be empty".to_owned());
        }

        let automation_enabled = env::var("IG_AUTOMATION_ENABLED")
            .unwrap_or_else(|_| "false".to_owned())
            .parse::<bool>()
            .map_err(|_| "IG_AUTOMATION_ENABLED must be true or false".to_owned())?;
        let ig_user_id = optional("IG_USER_ID");
        let ig_access_token = optional("IG_ACCESS_TOKEN");
        let poll_enabled = env::var("IG_POLL_ENABLED")
            .unwrap_or_else(|_| "true".to_owned())
            .parse::<bool>()
            .map_err(|_| "IG_POLL_ENABLED must be true or false".to_owned())?;
        let poll_interval_seconds = env::var("IG_POLL_INTERVAL_SECONDS")
            .unwrap_or_else(|_| "60".to_owned())
            .parse::<u64>()
            .map_err(|_| "IG_POLL_INTERVAL_SECONDS must be an integer".to_owned())?;
        if !(30..=3600).contains(&poll_interval_seconds) {
            return Err("IG_POLL_INTERVAL_SECONDS must be between 30 and 3600".to_owned());
        }

        Ok(Self {
            bind_addr,
            webhook_verify_token,
            app_secret,
            admin_password,
            database_path,
            keyword,
            automation_enabled,
            poll_enabled,
            poll_interval_seconds,
            ig_user_id,
            ig_username: String::new(),
            ig_access_token,
            api_version: env::var("META_API_VERSION").unwrap_or_else(|_| "v26.0".to_owned()),
            public_reply_text: env::var("PUBLIC_REPLY_TEXT")
                .unwrap_or_else(|_| "send it".to_owned()),
            private_reply_text: env::var("PRIVATE_REPLY_TEXT")
                .unwrap_or_else(|_| "dm worked!".to_owned()),
        })
    }
}

fn optional(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}
