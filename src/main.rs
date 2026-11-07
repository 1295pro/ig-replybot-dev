mod admin;
mod config;
mod deploy;
mod domain;
mod local;
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
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--help"] {
        println!(
            "IG Reply Bot\n\n--local                 Open the local control room; no environment file needed\n--data-dir PATH         Local data folder (requires --local)\n--no-browser            Start locally without opening a browser\n--version               Show the version\n\nNo arguments: run as a service using environment configuration."
        );
        return;
    }
    if args == ["--version"] {
        println!("IG Reply Bot {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let mut local_mode = false;
    let mut open_browser = true;
    let mut directory = None;
    let mut arguments = args.iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--local" => local_mode = true,
            "--no-browser" => open_browser = false,
            "--data-dir" => {
                let Some(path) = arguments.next().filter(|path| !path.starts_with("--")) else {
                    eprintln!("--data-dir requires a folder path");
                    std::process::exit(2);
                };
                directory = Some(std::path::PathBuf::from(path));
            }
            _ => {
                eprintln!("Unknown argument: {argument}. Use --help for launch options.");
                std::process::exit(2);
            }
        }
    }
    if !local_mode && !args.is_empty() {
        eprintln!("--data-dir and --no-browser require --local");
        std::process::exit(2);
    }
    let local_launch = local_mode.then(|| {
        local::LocalLaunch::prepare(directory, open_browser).unwrap_or_else(|error| {
            eprintln!("local startup error: {error}");
            std::process::exit(2);
        })
    });
    // Service mode supports existing environment-based installations.
    // and fill any missing values from the conventional .env file.
    if !local_mode
        && Path::new(".env.local").exists()
        && dotenvy::from_filename(".env.local").is_err()
    {
        eprintln!(
            "Invalid .env.local: quote text values that contain spaces or special characters"
        );
        std::process::exit(2);
    }
    if !local_mode {
        dotenvy::dotenv().ok();
    }
    let mut config = local_launch
        .as_ref()
        .map_or_else(Config::from_env, |local| Ok(local.config()))
        .unwrap_or_else(|error| {
            eprintln!("configuration error: {error}");
            std::process::exit(2);
        });
    // Bind before touching SQLite: a second launch must not recover the active
    // instance's delivery queue or alter its persisted settings.
    let address = config.bind_addr;
    let listener = match tokio::net::TcpListener::bind(address).await {
        Ok(listener) => listener,
        Err(error) => {
            if let Some(local) = &local_launch
                && local.already_running().await
            {
                local.open();
                eprintln!("IG Reply Bot is already running. Opened its control room.");
                return;
            }
            eprintln!(
                "Cannot open {address}: {error}. Close the program using that port and try again."
            );
            std::process::exit(2);
        }
    };
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

    let state = AppState::new(config.clone(), database.clone());
    tokio::spawn(worker::run(state.config_updates(), database.clone()));
    tokio::spawn(poller::run(
        state.config_updates(),
        database.clone(),
        state.poll_health(),
    ));
    eprintln!("IG Reply Bot admin panel available at http://127.0.0.1:3000/");

    let app = router(state);
    if let Some(local) = &local_launch {
        eprintln!("Local app data: {}", local.directory.display());
        local.open();
    }

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
