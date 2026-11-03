mod admin;
mod config;
mod deploy;
mod domain;
mod meta_api;
mod poller;
mod storage;
mod webhook;
mod worker;

use std::path::Path;
use std::sync::Arc;

use config::Config;
use meta_api::MetaApi;
use storage::Database;
use webhook::{AppState, router};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    #[cfg(unix)]
    unsafe {
        libc::umask(0o077);
    }
    // Keep process environment values authoritative, then use the local file
    // and fill any missing values from the conventional .env file.
    if Path::new(".env.local").exists() && dotenvy::from_filename(".env.local").is_err() {
        eprintln!(
            "Invalid .env.local: quote text values that contain spaces or special characters"
        );
        std::process::exit(2);
    }
    dotenvy::dotenv().ok();
    let mut config = Config::from_env().unwrap_or_else(|error| {
        eprintln!("configuration error: {error}");
        std::process::exit(2);
    });
    let database = Arc::new(
        Database::open(&config.database_path).unwrap_or_else(|error| {
            eprintln!("database error: {error}");
            std::process::exit(2);
        }),
    );
    let admin_password = config.admin_password.clone();
    if let Some(mut saved) = database.load_settings().unwrap_or_else(|error| {
        eprintln!("saved settings error: {error}");
        std::process::exit(2);
    }) {
        saved.bind_addr = config.bind_addr;
        saved.database_path = config.database_path.clone();
        saved.admin_password = admin_password;
        config = saved;
    }
    if let Some(token) = config
        .ig_access_token
        .as_deref()
        .filter(|_| config.automation_enabled || config.ig_username.is_empty())
    {
        match MetaApi::inspect_identity(token, &config.api_version).await {
            Ok(identity) => {
                config.ig_user_id = Some(identity.id);
                config.ig_username = identity.username;
            }
            Err(error) => {
                eprintln!("Instagram account check failed: {error}");
                if config.automation_enabled {
                    eprintln!("automation starts paused until the account token is fixed");
                    config.automation_enabled = false;
                }
            }
        }
    } else if config.automation_enabled {
        eprintln!("Instagram settings are incomplete; automation starts paused");
        config.automation_enabled = false;
    }
    if config.automation_enabled
        && !config.poll_enabled
        && (config.app_secret.is_empty() || config.webhook_verify_token.is_empty())
    {
        eprintln!("no comment source is configured; automation starts paused");
        config.automation_enabled = false;
    }
    let config = Arc::new(config);
    let address = config.bind_addr;

    let state = AppState::new(config.clone(), database.clone());
    tokio::spawn(worker::run(state.config_updates(), database.clone()));
    tokio::spawn(poller::run(
        state.config_updates(),
        database.clone(),
        state.poll_health(),
    ));
    eprintln!("IG Reply Bot admin panel available at http://127.0.0.1:3000/");

    let app = router(state);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .unwrap_or_else(|error| {
            eprintln!("failed to bind HTTP listener: {error}");
            std::process::exit(2);
        });

    eprintln!("listening on {address}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .expect("HTTP server failed");
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
