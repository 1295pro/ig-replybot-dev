use std::{
    net::Ipv4Addr,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use russh::{
    ChannelMsg, Disconnect,
    client::{self, Handle},
    keys::{HashAlg, PublicKeyOrCertificate},
};
use russh_sftp::{client::SftpSession, protocol::OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use zeroize::Zeroizing;

use crate::config::Config;

const DEPLOY_FILES: &[(&str, &[u8])] = &[
    ("Cargo.toml", include_bytes!("../Cargo.toml")),
    ("Cargo.lock", include_bytes!("../Cargo.lock")),
    ("Dockerfile", include_bytes!("../deploy/Dockerfile")),
    (".dockerignore", include_bytes!("../deploy/.dockerignore")),
    ("compose.yaml", include_bytes!("../deploy/compose.yaml")),
    ("deploy/Dockerfile", include_bytes!("../deploy/Dockerfile")),
    (
        "deploy/.dockerignore",
        include_bytes!("../deploy/.dockerignore"),
    ),
    (
        "deploy/compose.yaml",
        include_bytes!("../deploy/compose.yaml"),
    ),
    ("src/main.rs", include_bytes!("main.rs")),
    ("src/admin.rs", include_bytes!("admin.rs")),
    ("src/config.rs", include_bytes!("config.rs")),
    ("src/deploy.rs", include_bytes!("deploy.rs")),
    ("src/meta_api.rs", include_bytes!("meta_api.rs")),
    ("src/poller.rs", include_bytes!("poller.rs")),
    ("src/storage.rs", include_bytes!("storage.rs")),
    ("src/webhook.rs", include_bytes!("webhook.rs")),
    ("src/worker.rs", include_bytes!("worker.rs")),
    ("src/domain/mod.rs", include_bytes!("domain/mod.rs")),
    ("src/domain/keyword.rs", include_bytes!("domain/keyword.rs")),
    ("static/admin.html", include_bytes!("../static/admin.html")),
];

#[derive(Clone, Debug, Deserialize)]
pub struct ConnectionInput {
    pub host: String,
    pub ssh_port: u16,
    pub service_port: u16,
    pub username: String,
}

#[derive(Debug, Deserialize)]
pub struct PreflightInput {
    pub connection: ConnectionInput,
    pub host_fingerprint: String,
    pub ssh_password: String,
}

#[derive(Debug, Deserialize)]
pub struct InstallInput {
    pub connection: ConnectionInput,
    pub host_fingerprint: String,
    pub ssh_password: String,
    pub remote_admin_password: String,
}

#[derive(Debug, Serialize)]
pub struct ServerInfo {
    pub os_name: String,
    pub os_id: String,
    pub os_version: String,
    pub architecture: String,
    pub docker_installed: bool,
    pub docker_running: bool,
    pub compose_installed: bool,
    pub conflicting_packages: Vec<String>,
    pub existing_bot_detected: bool,
    pub port_available: bool,
    pub install_docker_supported: bool,
}

#[derive(Debug, Serialize)]
pub struct DeploymentResult {
    pub deployment_id: String,
    pub remote_directory: String,
    pub service_port: u16,
    pub tunnel_command: String,
    pub account_username: String,
    pub keyword: String,
    pub automated_checks_passed: bool,
    pub live_test_instruction: String,
}

#[derive(Clone)]
struct SshHandler {
    expected_fingerprint: Option<String>,
    observed_fingerprint: Arc<Mutex<Option<String>>>,
    allow_unpinned: bool,
}

impl client::Handler for SshHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let fingerprint = server_public_key
            .public_key()
            .fingerprint(HashAlg::Sha256)
            .to_string();
        if let Ok(mut observed) = self.observed_fingerprint.lock() {
            *observed = Some(fingerprint.clone());
        }
        Ok(self.allow_unpinned
            || self.expected_fingerprint.as_deref() == Some(fingerprint.as_str()))
    }
}

struct CommandOutput {
    code: u32,
    stdout: String,
    stderr: String,
}

pub async fn inspect_host(connection: &ConnectionInput) -> Result<String, String> {
    validate_connection(connection)?;
    let observed = Arc::new(Mutex::new(None));
    let session = open_session(connection, None, observed.clone()).await?;
    let fingerprint = observed
        .lock()
        .map_err(|_| "Could not read the SSH host fingerprint".to_owned())?
        .clone()
        .ok_or_else(|| "The SSH server did not provide a host key".to_owned())?;
    let _ = session
        .disconnect(Disconnect::ByApplication, "", "en")
        .await;
    Ok(fingerprint)
}

pub async fn check_server(input: PreflightInput) -> Result<ServerInfo, String> {
    let PreflightInput {
        connection,
        host_fingerprint,
        ssh_password,
    } = input;
    validate_connection(&connection)?;
    validate_fingerprint(&host_fingerprint)?;
    let ssh_password = Zeroizing::new(ssh_password);
    let mut session = authenticated_session(&connection, &host_fingerprint, &ssh_password).await?;
    let info = inspect_server(&mut session, &connection).await;
    let _ = session
        .disconnect(Disconnect::ByApplication, "", "en")
        .await;
    info
}

pub async fn deploy(input: InstallInput, config: &Config) -> Result<DeploymentResult, String> {
    let InstallInput {
        connection,
        host_fingerprint,
        ssh_password,
        remote_admin_password,
    } = input;
    validate_connection(&connection)?;
    validate_fingerprint(&host_fingerprint)?;
    if remote_admin_password.chars().count() < 32 {
        return Err("The VPS control-room password must contain at least 32 characters".to_owned());
    }
    ensure_deployable_config(config)?;

    let ssh_password = Zeroizing::new(ssh_password);
    let remote_admin_password = Zeroizing::new(remote_admin_password);
    let mut session = authenticated_session(&connection, &host_fingerprint, &ssh_password).await?;
    let result = deploy_authenticated(
        &mut session,
        &connection,
        &host_fingerprint,
        &remote_admin_password,
        config,
    )
    .await;
    let _ = session
        .disconnect(Disconnect::ByApplication, "", "en")
        .await;
    result
}

pub fn ensure_deployable_config(config: &Config) -> Result<(), String> {
    if config.automation_enabled {
        return Err(
            "Pause automation on this computer before deploying, to avoid duplicate replies"
                .to_owned(),
        );
    }
    if !config.poll_enabled {
        return Err("VPS deployment currently requires polling for the automatic Meta test. Enable polling first".to_owned());
    }
    if config.ig_access_token.is_none()
        || config.ig_user_id.is_none()
        || config.ig_username.is_empty()
    {
        return Err(
            "Connect and verify an Instagram Professional account before deployment".to_owned(),
        );
    }
    Ok(())
}

fn validate_connection(connection: &ConnectionInput) -> Result<(), String> {
    let host = connection.host.trim();
    if host.is_empty() || host.len() > 253 || host.contains(':') || host.contains('/') {
        return Err("Enter an IPv4 address or hostname (IPv6 is not supported yet)".to_owned());
    }
    if host.parse::<Ipv4Addr>().is_err() {
        let valid = host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        });
        if !valid {
            return Err("Enter a valid IPv4 address or hostname".to_owned());
        }
    }
    if connection.username != "root" {
        return Err("This installer currently supports the root SSH account only".to_owned());
    }
    if connection.ssh_port == 0 {
        return Err("SSH port must be between 1 and 65535".to_owned());
    }
    if !(1024..=65535).contains(&connection.service_port) {
        return Err("The private control-room port must be between 1024 and 65535".to_owned());
    }
    Ok(())
}

fn validate_fingerprint(fingerprint: &str) -> Result<(), String> {
    if !fingerprint.starts_with("SHA256:") || fingerprint.len() < 20 || fingerprint.len() > 100 {
        return Err("Confirm the SSH fingerprint before continuing".to_owned());
    }
    Ok(())
}

async fn open_session(
    connection: &ConnectionInput,
    expected_fingerprint: Option<String>,
    observed_fingerprint: Arc<Mutex<Option<String>>>,
) -> Result<Handle<SshHandler>, String> {
    let handler = SshHandler {
        allow_unpinned: expected_fingerprint.is_none(),
        expected_fingerprint,
        observed_fingerprint,
    };
    let config = client::Config {
        inactivity_timeout: Some(Duration::from_secs(600)),
        ..Default::default()
    };
    tokio::time::timeout(
        Duration::from_secs(20),
        client::connect(
            Arc::new(config),
            (connection.host.trim(), connection.ssh_port),
            handler,
        ),
    )
    .await
    .map_err(|_| "SSH connection timed out".to_owned())?
    .map_err(|_| "Could not connect to SSH. Check the address, port, and firewall".to_owned())
}

async fn authenticated_session(
    connection: &ConnectionInput,
    fingerprint: &str,
    password: &str,
) -> Result<Handle<SshHandler>, String> {
    let observed = Arc::new(Mutex::new(None));
    let mut session =
        match open_session(connection, Some(fingerprint.to_owned()), observed.clone()).await {
            Ok(session) => session,
            Err(error) => {
                if observed
                    .lock()
                    .ok()
                    .and_then(|value| value.clone())
                    .is_some_and(|actual| actual != fingerprint)
                {
                    return Err(
                    "The SSH host fingerprint changed. Stop and verify it with your VPS provider"
                        .to_owned(),
                );
                }
                return Err(error);
            }
        };
    let result = tokio::time::timeout(
        Duration::from_secs(20),
        session.authenticate_password(connection.username.clone(), password),
    )
    .await
    .map_err(|_| "SSH authentication timed out".to_owned())?
    .map_err(|_| {
        "SSH authentication failed; check the root password and SSH settings".to_owned()
    })?;
    if !result.success() {
        return Err("SSH login was rejected. Check the root password and SSH settings".to_owned());
    }
    Ok(session)
}

async fn inspect_server(
    session: &mut Handle<SshHandler>,
    connection: &ConnectionInput,
) -> Result<ServerInfo, String> {
    let command = format!(
        "set -eu; printf 'uid=%s\\n' \"$(id -u)\"; . /etc/os-release; printf 'os_id=%s\\nos_version=%s\\nos_name=%s\\n' \"$ID\" \"${{VERSION_ID:-}}\" \"${{PRETTY_NAME:-$ID}}\"; printf 'architecture=%s\\n' \"$(dpkg --print-architecture 2>/dev/null || uname -m)\"; if command -v docker >/dev/null 2>&1; then printf 'docker_installed=1\\n'; if docker info >/dev/null 2>&1; then printf 'docker_running=1\\n'; else printf 'docker_running=0\\n'; fi; if docker compose version >/dev/null 2>&1; then printf 'compose_installed=1\\n'; else printf 'compose_installed=0\\n'; fi; else printf 'docker_installed=0\\ndocker_running=0\\ncompose_installed=0\\n'; fi; printf 'conflicts='; dpkg-query -W -f='${{Package}} ' docker.io docker-compose docker-compose-v2 docker-doc docker-buildx podman-docker containerd runc 2>/dev/null || true; printf '\\n'; if systemctl is-active --quiet ig-reply-bot.service 2>/dev/null || (command -v pgrep >/dev/null 2>&1 && pgrep -x ig-reply-bot >/dev/null 2>&1) || (command -v docker >/dev/null 2>&1 && docker ps --format '{{{{.Names}}}}' | grep -Eiq '(^|[-_])ig[-_]?reply[-_]?bot([-_]|$)'); then printf 'existing_bot=1\\n'; else printf 'existing_bot=0\\n'; fi; if command -v ss >/dev/null 2>&1 && ss -H -ltn 'sport = :{}' 2>/dev/null | grep -q .; then printf 'port_available=0\\n'; else printf 'port_available=1\\n'; fi",
        connection.service_port
    );
    let output = run_command(session, &command, None).await?;
    if output.code != 0 {
        return Err(
            "Server checks failed. Confirm this is a Linux VPS and the SSH account is root"
                .to_owned(),
        );
    }
    parse_server_info(&output.stdout)
}

fn parse_server_info(output: &str) -> Result<ServerInfo, String> {
    let fields = output
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect::<std::collections::HashMap<_, _>>();
    let get = |key: &str| fields.get(key).copied().unwrap_or("").trim();
    if get("uid") != "0" {
        return Err("SSH connected, but the account is not root".to_owned());
    }
    let os_id = get("os_id").to_ascii_lowercase();
    let architecture = get("architecture").to_ascii_lowercase();
    if !matches!(
        architecture.as_str(),
        "amd64" | "arm64" | "x86_64" | "aarch64"
    ) {
        return Err("This VPS architecture is not supported yet; use amd64 or arm64".to_owned());
    }
    let conflicting_packages = get("conflicts")
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let docker_installed = get("docker_installed") == "1";
    let existing_bot_detected = get("existing_bot") == "1";
    let install_docker_supported =
        matches!(os_id.as_str(), "ubuntu" | "debian") && conflicting_packages.is_empty();
    if !docker_installed && !install_docker_supported {
        return Err(if !conflicting_packages.is_empty() {
            format!(
                "Docker installation stopped: conflicting packages are installed ({}). Remove or reconcile them, then try again",
                conflicting_packages.join(", ")
            )
        } else {
            "Automatic Docker installation currently supports Ubuntu and Debian. Install Docker manually on this VPS, then retry".to_owned()
        });
    }
    Ok(ServerInfo {
        os_name: get("os_name").to_owned(),
        os_id,
        os_version: get("os_version").to_owned(),
        architecture,
        docker_installed,
        docker_running: get("docker_running") == "1",
        compose_installed: get("compose_installed") == "1",
        conflicting_packages,
        existing_bot_detected,
        port_available: get("port_available") == "1",
        install_docker_supported,
    })
}

async fn deploy_authenticated(
    session: &mut Handle<SshHandler>,
    connection: &ConnectionInput,
    _fingerprint: &str,
    remote_admin_password: &str,
    config: &Config,
) -> Result<DeploymentResult, String> {
    let info = inspect_server(session, connection).await?;
    if info.existing_bot_detected {
        return Err(
            "An existing IG Reply Bot process or container is already running on this VPS. Pause it before creating a second instance for the same Instagram account".to_owned(),
        );
    }
    if !info.port_available {
        return Err(format!(
            "Port {} is already in use on the VPS; choose another local control-room port",
            connection.service_port
        ));
    }
    if !info.docker_installed && !info.install_docker_supported {
        return Err(
            "Docker cannot be installed automatically on this Linux distribution".to_owned(),
        );
    }
    if !info.docker_installed {
        run_checked(
            session,
            DOCKER_INSTALL_SCRIPT,
            "install Docker from the official Ubuntu/Debian repository",
        )
        .await?;
    } else if !info.docker_running {
        run_checked(
            session,
            "systemctl enable --now docker",
            "start the existing Docker service",
        )
        .await?;
    }
    run_checked(
        session,
        "i=0; until docker info >/dev/null 2>&1 && docker compose version >/dev/null 2>&1; do i=$((i + 1)); if [ \"$i\" -ge 30 ]; then echo 'docker_or_compose_not_ready'; exit 1; fi; sleep 2; done",
        "verify Docker Engine and Compose",
    )
    .await?;

    let deployment_id = new_deployment_id();
    let project = format!("igreplybot{deployment_id}");
    let remote_directory = format!("/opt/ig-reply-bot-{deployment_id}");
    let prepare = format!(
        "set -eu; test ! -e '{remote_directory}' || {{ echo 'deployment_path_exists'; exit 17; }}; install -d -o root -g root -m 700 '{remote_directory}'"
    );
    run_checked(
        session,
        &prepare,
        "create an isolated private deployment folder",
    )
    .await?;

    let connection_result = upload_bundle(
        session,
        &remote_directory,
        remote_admin_password,
        config,
        connection,
    )
    .await;
    if let Err(error) = connection_result {
        return Err(format!(
            "Could not upload the clean application bundle: {error}. Private files, if any, are in {remote_directory}"
        ));
    }

    let compose = format!(
        "cd '{remote_directory}' && docker compose --project-name '{project}' -f compose.yaml config --quiet && docker compose --project-name '{project}' -f compose.yaml up -d --build"
    );
    run_checked(
        session,
        &compose,
        "build and start the dedicated Docker container",
    )
    .await?;

    let status_command = format!(
        "cd '{remote_directory}' && docker compose --project-name '{project}' -f compose.yaml exec -T bot sh -c 'curl -fsS -H \"Authorization: Bearer $IG_ADMIN_PASSWORD\" http://127.0.0.1:3000/api/admin/status'"
    );
    let mut last_error = String::new();
    let mut passed = false;
    for _ in 0..36 {
        match run_command(session, &status_command, None).await {
            Ok(output) if output.code == 0 => {
                if let Ok(status) = serde_json::from_str::<Value>(&output.stdout) {
                    let correct_account = status
                        .get("account_username")
                        .and_then(Value::as_str)
                        .is_some_and(|username| username == config.ig_username);
                    let active =
                        status.get("automation_enabled").and_then(Value::as_bool) == Some(true);
                    let polling =
                        status.get("polling_state").and_then(Value::as_str) == Some("healthy");
                    if correct_account && active && polling {
                        passed = true;
                        break;
                    }
                    last_error =
                        "The container is running, but the Meta account poll has not passed yet"
                            .to_owned();
                    if !active {
                        last_error = "The bot started paused because Meta rejected the account token. Recheck the token and permissions".to_owned();
                        break;
                    }
                } else {
                    last_error = "The bot returned an invalid status response".to_owned();
                }
            }
            Ok(output) => last_error = limited_output(&output.stderr, 1200),
            Err(error) => last_error = error,
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    if !passed {
        let pause = format!(
            "cd '{remote_directory}' && docker compose --project-name '{project}' -f compose.yaml exec -T bot sh -c 'curl -fsS -X POST -H \"Authorization: Bearer $IG_ADMIN_PASSWORD\" -H \"Content-Type: application/json\" -d \"{{\\\"enabled\\\":false}}\" http://127.0.0.1:3000/api/admin/automation >/dev/null'"
        );
        let _ = run_command(session, &pause, None).await;
        return Err(format!(
            "The Docker container was installed, but the automated Meta check did not pass. The bot has been paused. {last_error}. Deployment files remain at {remote_directory}"
        ));
    }

    let tunnel_command = format!(
        "ssh -p {2} -L {0}:127.0.0.1:{0} root@{1}",
        connection.service_port, connection.host, connection.ssh_port
    );
    Ok(DeploymentResult {
        deployment_id,
        remote_directory,
        service_port: connection.service_port,
        tunnel_command,
        account_username: config.ig_username.clone(),
        keyword: config.keyword.clone(),
        automated_checks_passed: true,
        live_test_instruction: format!(
            "The VPS container is healthy and Meta polling succeeded. Leave a new comment containing '{}' under a post from another Instagram account, then check that the public reply and DM arrive.",
            config.keyword
        ),
    })
}

async fn upload_bundle(
    session: &mut Handle<SshHandler>,
    remote_directory: &str,
    remote_admin_password: &str,
    config: &Config,
    connection: &ConnectionInput,
) -> Result<(), String> {
    let channel = session
        .channel_open_session()
        .await
        .map_err(|_| "Could not open the private upload channel".to_owned())?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|_| "SFTP is not available on this VPS".to_owned())?;
    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(|_| "Could not start SFTP for the upload".to_owned())?;

    for directory in ["src", "src/domain", "static", "deploy"] {
        sftp.create_dir(format!("{remote_directory}/{directory}"))
            .await
            .map_err(|_| "Could not create the application source folders".to_owned())?;
    }
    let compose_file = render_compose(connection.service_port)?;
    for (relative_path, contents) in DEPLOY_FILES {
        let rendered = if *relative_path == "compose.yaml" {
            compose_file.as_bytes()
        } else {
            contents
        };
        sftp_create_file(
            &sftp,
            &format!("{remote_directory}/{relative_path}"),
            rendered,
        )
        .await?;
    }

    let env_file = Zeroizing::new(build_runtime_env(
        config,
        remote_admin_password,
        connection,
    )?);
    sftp_create_file(
        &sftp,
        &format!("{remote_directory}/runtime.env"),
        env_file.as_bytes(),
    )
    .await?;
    sftp.close()
        .await
        .map_err(|_| "Could not close the secure upload channel".to_owned())?;
    let permissions = format!(
        "chmod 600 '{remote_directory}/runtime.env' && chmod 644 '{remote_directory}/compose.yaml'"
    );
    let output = run_command(session, &permissions, None).await?;
    if output.code != 0 {
        return Err("Could not set private file permissions on the VPS".to_owned());
    }
    Ok(())
}

async fn sftp_create_file(
    sftp: &SftpSession,
    remote_path: &str,
    contents: &[u8],
) -> Result<(), String> {
    let mut file = sftp
        .open_with_flags(
            remote_path,
            OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
        )
        .await
        .map_err(|_| "Could not create an application file on the VPS".to_owned())?;
    file.write_all(contents)
        .await
        .map_err(|_| "Could not upload an application file to the VPS".to_owned())?;
    file.close()
        .await
        .map_err(|_| "Could not finish uploading an application file".to_owned())
}

fn build_runtime_env(
    config: &Config,
    admin_password: &str,
    connection: &ConnectionInput,
) -> Result<String, String> {
    let access_token = config
        .ig_access_token
        .as_deref()
        .ok_or_else(|| "Instagram access token is missing".to_owned())?;
    let user_id = config
        .ig_user_id
        .as_deref()
        .ok_or_else(|| "Instagram account ID is missing".to_owned())?;
    let fields = [
        ("IG_ADMIN_PASSWORD", admin_password),
        ("IG_REPLY_BIND_ADDR", "0.0.0.0:3000"),
        ("IG_DATABASE_PATH", "/data/ig_reply_bot.sqlite3"),
        ("IG_AUTOMATION_ENABLED", "true"),
        ("IG_POLL_ENABLED", "true"),
        (
            "IG_POLL_INTERVAL_SECONDS",
            &config.poll_interval_seconds.to_string(),
        ),
        ("IG_KEYWORD", &config.keyword),
        ("IG_USER_ID", user_id),
        ("IG_ACCESS_TOKEN", access_token),
        ("META_API_VERSION", &config.api_version),
        ("PUBLIC_REPLY_TEXT", &config.public_reply_text),
        ("PRIVATE_REPLY_TEXT", &config.private_reply_text),
        ("META_APP_SECRET", &config.app_secret),
        ("META_WEBHOOK_VERIFY_TOKEN", &config.webhook_verify_token),
        ("IG_DEPLOY_HOST", &connection.host),
        ("IG_DEPLOY_SSH_PORT", &connection.ssh_port.to_string()),
        (
            "IG_DEPLOY_SERVICE_PORT",
            &connection.service_port.to_string(),
        ),
    ];
    Ok(fields
        .into_iter()
        .map(|(key, value)| format!("{key}='{}'", value.replace('\'', "\\'")))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n")
}

fn render_compose(service_port: u16) -> Result<String, String> {
    if !(1024..=65535).contains(&service_port) {
        return Err("The control-room port must be between 1024 and 65535".to_owned());
    }
    let template = include_str!("../deploy/compose.yaml");
    let default_port = "127.0.0.1:12950:3000";
    if template.matches(default_port).count() != 1 {
        return Err("The Docker Compose template has an unexpected port mapping".to_owned());
    }
    Ok(template.replace(default_port, &format!("127.0.0.1:{service_port}:3000")))
}

async fn run_checked(
    session: &mut Handle<SshHandler>,
    command: &str,
    stage: &str,
) -> Result<(), String> {
    let output = tokio::time::timeout(
        Duration::from_secs(900),
        run_command(session, command, None),
    )
    .await
    .map_err(|_| format!("Timed out while trying to {stage}"))??;
    if output.code == 0 {
        Ok(())
    } else {
        let details = limited_output(&output.stderr, 1600);
        Err(if details.is_empty() {
            format!(
                "Could not {stage}; server command exited with code {}",
                output.code
            )
        } else {
            format!("Could not {stage}: {details}")
        })
    }
}

async fn run_command(
    session: &mut Handle<SshHandler>,
    command: &str,
    _stdin: Option<&[u8]>,
) -> Result<CommandOutput, String> {
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|_| "Could not open an SSH command channel".to_owned())?;
    channel
        .exec(true, command)
        .await
        .map_err(|_| "The VPS rejected an installer command".to_owned())?;
    channel
        .eof()
        .await
        .map_err(|_| "Could not finish sending the installer command".to_owned())?;
    let mut code = None;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    while let Some(message) = channel.wait().await {
        match message {
            ChannelMsg::Data { data } => append_bounded(&mut stdout, &data, 16 * 1024),
            ChannelMsg::ExtendedData { data, .. } => append_bounded(&mut stderr, &data, 16 * 1024),
            ChannelMsg::ExitStatus { exit_status } => code = Some(exit_status),
            ChannelMsg::Close => break,
            _ => {}
        }
    }
    Ok(CommandOutput {
        code: code.unwrap_or(255),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

fn append_bounded(destination: &mut Vec<u8>, source: &[u8], max_len: usize) {
    if source.len() >= max_len {
        destination.clear();
        destination.extend_from_slice(&source[source.len() - max_len..]);
        return;
    }
    let overflow = destination
        .len()
        .saturating_add(source.len())
        .saturating_sub(max_len);
    if overflow > 0 {
        destination.drain(..overflow.min(destination.len()));
    }
    destination.extend_from_slice(source);
}

fn limited_output(output: &str, max_chars: usize) -> String {
    output
        .chars()
        .rev()
        .take(max_chars)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>()
        .trim()
        .to_owned()
}

fn new_deployment_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    format!("{timestamp:x}{:x}", std::process::id())
}

const DOCKER_INSTALL_SCRIPT: &str = r#"set -eu
. /etc/os-release
case "$ID" in ubuntu|debian) ;; *) echo 'unsupported_linux_distribution'; exit 20 ;; esac
if dpkg-query -W -f='${Package}\n' docker.io docker-compose docker-compose-v2 docker-doc docker-buildx podman-docker containerd runc 2>/dev/null | grep -q .; then
  echo 'conflicting_docker_packages'
  exit 21
fi
apt-get update
DEBIAN_FRONTEND=noninteractive apt-get install -y ca-certificates curl
install -m 0755 -d /etc/apt/keyrings
curl -fsSL "https://download.docker.com/linux/$ID/gpg" -o /etc/apt/keyrings/docker.asc
chmod a+r /etc/apt/keyrings/docker.asc
ARCH="$(dpkg --print-architecture)"
CODENAME="${UBUNTU_CODENAME:-${VERSION_CODENAME:-}}"
test -n "$CODENAME"
printf 'deb [arch=%s signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/%s %s stable\n' "$ARCH" "$ID" "$CODENAME" > /etc/apt/sources.list.d/docker.list
apt-get update
DEBIAN_FRONTEND=noninteractive apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
systemctl enable --now docker
"#;

#[cfg(test)]
mod tests {
    use super::{
        ConnectionInput, DEPLOY_FILES, build_runtime_env, parse_server_info, render_compose,
        validate_connection,
    };
    use crate::config::Config;
    use std::{net::SocketAddr, path::PathBuf};

    fn sample_config() -> Config {
        Config {
            bind_addr: "127.0.0.1:3000".parse::<SocketAddr>().unwrap(),
            webhook_verify_token: String::new(),
            app_secret: String::new(),
            admin_password: "local-admin-password-is-not-forwarded".to_owned(),
            database_path: PathBuf::from("local.sqlite3"),
            keyword: "launch".to_owned(),
            automation_enabled: false,
            poll_enabled: true,
            poll_interval_seconds: 60,
            ig_user_id: Some("123456789".to_owned()),
            ig_username: "sample_account".to_owned(),
            ig_access_token: Some("secret-token".to_owned()),
            api_version: "v26.0".to_owned(),
            public_reply_text: "Thanks for commenting".to_owned(),
            private_reply_text: "Here is the link".to_owned(),
        }
    }

    #[test]
    fn accepts_only_safe_root_ssh_connection_values() {
        let connection = ConnectionInput {
            host: "bot.example.com".to_owned(),
            ssh_port: 22,
            service_port: 12950,
            username: "root".to_owned(),
        };
        assert!(validate_connection(&connection).is_ok());

        let mut unsafe_connection = connection.clone();
        unsafe_connection.host = "host.example; id".to_owned();
        assert!(validate_connection(&unsafe_connection).is_err());
        unsafe_connection.host = "bot.example.com".to_owned();
        unsafe_connection.username = "admin".to_owned();
        assert!(validate_connection(&unsafe_connection).is_err());
    }

    #[test]
    fn parses_server_support_and_existing_bot_status() {
        let output = "uid=0\nos_id=ubuntu\nos_version=24.04\nos_name=Ubuntu 24.04\narchitecture=amd64\ndocker_installed=1\ndocker_running=1\ncompose_installed=1\nconflicts=\nexisting_bot=0\nport_available=1\n";
        let info = parse_server_info(output).unwrap();
        assert_eq!(info.os_id, "ubuntu");
        assert!(info.docker_running);
        assert!(info.install_docker_supported);
        assert!(!info.existing_bot_detected);
        assert!(info.port_available);

        let running_bot = output.replace("existing_bot=0", "existing_bot=1");
        assert!(
            parse_server_info(&running_bot)
                .unwrap()
                .existing_bot_detected
        );
    }

    #[test]
    fn rejects_unprivileged_or_unsupported_servers() {
        assert!(parse_server_info("uid=1000\n").is_err());
        let unsupported_arch = "uid=0\nos_id=ubuntu\narchitecture=i386\n";
        assert!(parse_server_info(unsupported_arch).is_err());
        let conflicting =
            "uid=0\nos_id=ubuntu\narchitecture=amd64\ndocker_installed=0\nconflicts=docker.io\n";
        assert!(parse_server_info(conflicting).is_err());
    }

    #[test]
    fn runtime_env_quotes_values_without_forwarding_local_admin_password() {
        let mut config = sample_config();
        config.public_reply_text = "We're ready\n$HOME #literal".to_owned();
        let connection = ConnectionInput {
            host: "bot.example.com".to_owned(),
            ssh_port: 22,
            service_port: 12950,
            username: "root".to_owned(),
        };
        let rendered = build_runtime_env(
            &config,
            "remote-password-0123456789-0123456789",
            &connection,
        )
        .unwrap();
        assert!(rendered.contains("PUBLIC_REPLY_TEXT='We\\'re ready\n$HOME #literal'"));
        assert!(rendered.contains("IG_ADMIN_PASSWORD='remote-password-0123456789-0123456789'"));
        assert!(!rendered.contains(&config.admin_password));
        assert!(rendered.contains("IG_AUTOMATION_ENABLED='true'"));
        assert!(rendered.contains("IG_POLL_ENABLED='true'"));
        assert!(rendered.contains("IG_DEPLOY_HOST='bot.example.com'"));
    }

    #[test]
    fn compose_port_stays_loopback_only_and_changes_only_once() {
        let rendered = render_compose(15432).unwrap();
        assert!(rendered.contains("127.0.0.1:15432:3000"));
        assert!(!rendered.contains("127.0.0.1:12950:3000"));
        assert!(rendered.contains("bot-data:/data"));
        assert!(render_compose(0).is_err());
    }

    #[test]
    fn deployment_manifest_excludes_local_secret_and_database_files() {
        let paths = DEPLOY_FILES
            .iter()
            .map(|(path, _)| *path)
            .collect::<Vec<_>>();
        assert!(!paths.iter().any(|path| path.starts_with(".env")));
        assert!(!paths.iter().any(|path| path.contains("sqlite")));
        assert!(!paths.contains(&"runtime.env"));
        assert!(paths.contains(&"deploy/Dockerfile"));
        assert!(paths.contains(&"deploy/.dockerignore"));
        assert!(paths.contains(&"deploy/compose.yaml"));
        assert!(paths.contains(&"static/admin.html"));
    }
}
