# Operator guide

This project runs one Instagram Professional account at a time. It watches new comments on that account's media. When a comment consists only of the configured keyword, it posts a public reply and then sends one private reply to the commenter. Each operator supplies their own Meta app, Instagram access token, texts, and database.

Ordinary commenters do **not** need to be Meta app testers. App roles govern which professional account owners can authorize a Meta app. Serving professional accounts owned by people outside your app roles may require Meta App Review and Advanced Access. See Meta's current [Instagram API with Instagram Login documentation](https://developers.facebook.com/docs/instagram-platform/instagram-api-with-instagram-login/) for the current rules.

## Choose where it runs

| Setup | What stays on | Public IP / domain | Delivery speed |
| --- | --- | --- | --- |
| Laptop only | Your laptop and the bot process | Not needed with polling | Usually up to one poll interval, plus Meta/API processing |
| Linux server (Docker wizard) | The VPS and a separate Docker Compose project | Not needed with polling; HTTPS domain needed only for optional webhooks | Same polling behavior; a working webhook may deliver sooner |
| Linux server (manual) | The VPS and its systemd service | Not needed with polling; HTTPS domain needed only for optional webhooks | Same polling behavior; a working webhook may deliver sooner |

The default poll interval is 60 seconds. Keep the laptop awake and connected while using the laptop setup. A sleeping or powered-off laptop cannot process comments. Reducing the interval increases Meta API usage. The server setup is better for continuous operation.

## Before either setup: configure Meta

1. Use an Instagram Business or Creator account. Create your own Meta developer app with Instagram Login. Do not copy another operator's token or app secret.
2. Make sure the app/token can use `instagram_business_basic`, `instagram_business_manage_comments`, and `instagram_business_manage_messages` for that professional account.
3. Generate an access token for that account. Record its expiration date: this bot does **not** refresh tokens automatically. The control room checks the token and reads the account ID and username for you.
4. Meta's app secret and a separate webhook verify token are only needed if you enable webhooks. Polling works without them.

The built-in webhook test in Meta's dashboard only tests the callback. For an acceptance test, a **different ordinary Instagram account** should leave a new comment containing exactly your keyword under a post on the connected professional account.

## Laptop only: Windows PowerShell

Install [Git](https://git-scm.com/downloads) and [Rust](https://rustup.rs/) first. The default Windows Rust toolchain may also ask for Microsoft C++ Build Tools; follow the [Rust installation instructions](https://rust-lang.org/tools/install/?platform_override=win) if the linker is missing. Open a new PowerShell window after installing Rust so `cargo` is on your PATH. The repository is currently private, so GitHub access is required:

```powershell
git clone https://github.com/1295pro/ig-replybot-dev.git
Set-Location ig-replybot-dev
Copy-Item .env.example .env.local
notepad .env.local
```

In `.env.local`, set `IG_ADMIN_PASSWORD` to a unique random value of at least 32 characters. Keep `IG_REPLY_BIND_ADDR=127.0.0.1:3000` and `IG_AUTOMATION_ENABLED=false`. The Meta token and reply settings can be entered in the browser panel after startup.

On Windows, double-click `start-local.cmd` to build, start, and open the local control room. The equivalent terminal command is:

```powershell
cargo run --release --locked
```

Open `http://127.0.0.1:3000/` in your browser and sign in with `IG_ADMIN_PASSWORD`. Connect the Instagram account, set the trigger and replies, then enable automation from the panel when you're ready.

Use the panel's **Enable bot** button when the account and replies are ready. The first complete poll establishes a baseline and does **not** answer old comments. Now post a **new** exact-keyword comment from another Instagram account. Check for both the public reply and the DM. Keep this terminal running and the laptop awake.

On macOS or Linux, use `cp .env.example .env.local`, edit the file, and run the same Cargo command. Keep `.env.local` private (`chmod 600 .env.local` on Unix systems).

## Deploy a separate Docker instance from the control room

The UI-assisted installer runs from the control room on your laptop. It connects to a VPS over SSH and supports root login on Ubuntu or Debian with `amd64` or `arm64`. A public IP or hostname and an SSH-reachable port are required; polling does not need a public app port, domain, Nginx, or webhook.

1. On your laptop, start the bot and open `http://127.0.0.1:3000/`. Sign in, connect your Instagram Professional account, and save the trigger word, public reply, and DM.
2. Keep polling enabled and **pause automation on the laptop**. The installer refuses to deploy while the local copy is active so two copies do not answer the same comments.
3. Open **Deploy to VPS**. Enter the VPS address and SSH port, then choose **Read SSH server fingerprint**. Compare the shown fingerprint with the VPS provider's console. Do not continue if the values differ.
4. After confirming the fingerprint, enter the VPS root SSH password and a new VPS control-room password of at least 32 characters. The SSH password is used for the checks and installation session and is then cleared from the page; it is not stored. Keep a separate record of the new VPS panel password.
5. Select **Check server** and review the operating system, existing bot check, Docker/Compose status, and private control-room port. If an existing IG Reply Bot service, process, or container is detected, pause it yourself before installing. The wizard will not stop or replace it.
6. Review the installation plan, select the confirmation checkbox, and choose **Deploy and run checks**. On Ubuntu/Debian without Docker, the wizard installs Docker Engine and Compose from Docker's official repository. It creates a new root-owned `/opt/ig-reply-bot-<id>` directory and separate Compose project, uploads only the required source files and runtime settings, builds a non-root container, and waits for a successful first poll of the connected account.
7. When the automated checks pass, copy the displayed SSH tunnel command into a second PowerShell/Terminal window. Keep it running and open the displayed `http://127.0.0.1:<port>/` URL. Sign in with the **new VPS control-room password**.
8. From another Instagram account, leave a **new** comment that matches the configured trigger on a post belonging to the connected account. Confirm both the public reply and DM. The first complete poll creates a baseline and does not answer old comments; allow up to the configured poll interval plus Meta processing time for a new comment to be found.

The admin port is published only on VPS loopback and is reachable through the SSH tunnel. The wizard does not change firewall, Nginx, DNS, systemd, or existing containers. The access token, Meta settings, and new VPS panel password are stored unencrypted in `/opt/ig-reply-bot-<id>/runtime.env` with mode `0600`; VPS root and the Docker daemon can access them. The SQLite database is stored in the project's named Docker volume. The setup wizard currently creates a fresh deployment; it does not update or remove existing deployments. Keep the deployment ID from the success message so you can operate the right Compose project.

To inspect a deployment, use the project name `igreplybot<ID>` shown by that deployment ID:

```sh
cd /opt/ig-reply-bot-<ID>
docker compose --project-name igreplybot<ID> -f compose.yaml ps
docker compose --project-name igreplybot<ID> -f compose.yaml logs --tail=100 bot
```

To stop its container without deleting its saved settings/history, run `docker compose --project-name igreplybot<ID> -f compose.yaml down`. Adding `--volumes` also deletes the bot's SQLite settings and deduplication history.

## Linux server: polling-only installation

The example below targets an Ubuntu server with SSH and `systemd`. Install Rust and Git, clone the repository, and configure `.env.local` as above. For the server, set `IG_DATABASE_PATH=/var/lib/ig-reply-bot/queue.sqlite3` and `IG_REPLY_BIND_ADDR=127.0.0.1:3000`. Build the binary:

```sh
cargo build --release --locked
```

Create a dedicated service account and install the files. Review the paths before running these commands on your server:

```sh
sudo useradd --system --user-group --home-dir /var/lib/ig-reply-bot --shell /usr/sbin/nologin ig-reply-bot
sudo install -d -m 0755 /opt/ig-reply-bot
sudo install -d -m 0700 /etc/ig-reply-bot
sudo install -m 0755 target/release/ig-reply-bot /opt/ig-reply-bot/ig-reply-bot
sudo install -m 0600 .env.local /etc/ig-reply-bot/ig-reply-bot.env
sudo install -m 0644 deploy/ig-reply-bot.service /etc/systemd/system/ig-reply-bot.service
sudo systemctl daemon-reload
sudo systemctl enable --now ig-reply-bot
```

The installed environment file must contain the unique random `IG_ADMIN_PASSWORD` you set in `.env.local`. Open an SSH tunnel from your computer with `ssh -L 3000:127.0.0.1:3000 USER@YOUR-SERVER`, then visit `http://127.0.0.1:3000/` and configure the account and replies in the control room. The panel saves settings immediately; it does not require a service restart.

The unit creates and owns `/var/lib/ig-reply-bot` for the unprivileged service user. It binds only to loopback, so no firewall or DNS changes are required for polling. While automation is disabled, check:

```sh
curl http://127.0.0.1:3000/healthz
curl http://127.0.0.1:3000/readyz
sudo journalctl -u ig-reply-bot -n 50 --no-pager
```

Use the control room's **Enable bot** button, wait for the first successful poll, then submit a **new** keyword comment from another account and confirm the public reply and DM. If you later replace the binary, restart the service. Keep a backup of the SQLite database before maintenance; it contains the saved settings and history used to avoid duplicate replies.

### Optional webhook and HTTPS

Polling alone needs no incoming connection. For faster webhook delivery, use an HTTPS domain and add either [the Nginx snippet](../deploy/nginx-ig-location.conf) or [the Caddy snippet](../deploy/Caddyfile.ig-snippet) to your **existing** site. They forward only `/ig/webhook` to the local `/webhook` route. Configure Meta's callback to `https://YOUR-DOMAIN/ig/webhook`, enter the same verify token, and subscribe the professional account to `comments`. Keep polling enabled as a fallback. Webhook events are accepted only with Meta's valid HMAC signature. Do not route the admin panel through your public proxy; reach it through the SSH tunnel.

## What the settings mean

The full variable list is in [README.md](../README.md#settings) and the safe template is [`.env.example`](../.env.example). In particular:

- Automation starts paused. Use the control room's **Enable bot** button after the token and reply texts are correct; **Pause bot** stops new delivery.
- `IG_POLL_ENABLED=true` reads your own account's comments through Meta. It needs outbound internet access, not a public address.
- `IG_KEYWORD` matches the **entire** comment, ignoring case and surrounding punctuation/emoji. With `бот`, `🤖 БОТ!` matches, but `хочу бота` does not.
- `PUBLIC_REPLY_TEXT` is posted beneath the comment. `PRIVATE_REPLY_TEXT` is sent as a private reply after Meta accepts the public one.
- `IG_DATABASE_PATH` is a persistent SQLite file. Do not delete it casually: it stores the baseline, seen comment IDs, and delivery states.

## Health, failures, and limits

- `/healthz` reports that the HTTP process answers. `/readyz` checks the local database and, when polling is enabled, requires a recent successful poll. Read its `automation` field: `ready` with automation disabled means **no replies**. Even `ready` with automation enabled does not prove that Meta will accept the next public reply or DM; inspect delivery logs and periodically make a controlled comment.
- The logs record a public reply and a private reply separately. A failed public reply stops the private step. Meta can reject a request because of permissions, token expiration, account eligibility, or its current rules.
- A definite API rejection is marked `failed`. A network interruption after a request is marked `uncertain`; neither state is retried automatically because the message may already have been delivered. There is no operator UI for reviewing or retrying those rows yet.
- The first complete poll records existing comments without replying. A comment hidden or delayed by Meta during that baseline may appear later as new. The poller rechecks changed media for three polls and scans all media every ten completed polls.
- The token is not renewed automatically. The database has no automatic retention or cleanup policy. The bot handles one connected account per process and has no user-facing dashboard.
- Each Meta collection is limited to 20 pages of up to 100 items. If an account has more than 2,000 media items or a post has more than 2,000 top-level comments, a poll fails rather than silently skipping the rest. This version is intended for a small self-hosted account, not a high-volume multi-account service.
- Keep the repository, `.env.local`, service environment file, and database private as appropriate. `.env*` (except `.env.example`), database files, and build output are ignored by Git. Before publishing, inspect `git status` for unexpected files.

## File map

| File | Purpose |
| --- | --- |
| `README.md` | Project overview, requirements, configuration table, and release checks. |
| `start-local.cmd` and `scripts/start-local.ps1` | Windows one-click local build, launch, and browser opening. |
| `docs/QUICK_START_RU.md` | Short Russian instructions for local setup and checks. |
| `docs/USER_GUIDE.md` | Laptop and server setup, operations, and this file map. |
| `docs/ARCHITECTURE.md` | Event flow, trust boundaries, failure behavior, and release criteria. |
| `.env.example` | Safe settings template. Copy it; do not put real secrets here. |
| `.gitignore` | Keeps local secrets, SQLite files, and compiled output out of Git. |
| `.github/workflows/ci.yml` | GitHub Actions formatting, lint, test, and release-build checks. |
| `Cargo.toml` | Rust package metadata and dependencies. |
| `Cargo.lock` | Exact dependency versions used for reproducible builds. |
| `LICENSE` | MIT license. |
| `src/main.rs` | Loads settings, verifies account identity, starts the worker, poller, and HTTP server. |
| `src/config.rs` | Reads and validates settings from the environment. |
| `src/domain/mod.rs` | Registers the domain logic module. |
| `src/domain/keyword.rs` | Normalizes and matches the whole comment against the keyword. |
| `src/poller.rs` | Reads account media/comments from Meta, handles pagination and rescan scheduling. |
| `src/admin.rs` | Password-protected control-room API for accounts, replies, automation, and VPS deployment. |
| `src/deploy.rs` | SSH fingerprint pinning, VPS preflight, allowlisted SFTP upload, Docker Compose install, and automated Meta-poll checks. |
| `static/admin.html` | Self-hosted control-room interface; contains no external scripts or fonts. |
| `src/webhook.rs` | Webhook verification, HMAC signature checks, event filtering, live settings, and health routes. |
| `src/storage.rs` | SQLite schema, baseline/deduplication, queue state, and migrations. |
| `src/worker.rs` | Takes queued jobs and sends public then private replies. |
| `src/meta_api.rs` | HTTP requests to Meta for identity, replies, and DM; summarizes API errors. |
| `deploy/README.md` | Deployment layout and optional webhook proxy checklist. |
| `deploy/Dockerfile` | Multi-stage Docker build; the final bot runs as a non-root user. |
| `deploy/compose.yaml` | Isolated bot service, persistent database volume, loopback-only admin port, and container healthcheck. |
| `deploy/.dockerignore` | Keeps secrets, databases, build output, and Git metadata out of the Docker build context. |
| `deploy/ig-reply-bot.service` | Example `systemd` service and filesystem restrictions. |
| `deploy/nginx-ig-location.conf` | Optional Nginx route for `/ig/`. |
| `deploy/Caddyfile.ig-snippet` | Optional Caddy route for `/ig/`. |
| `deploy/LEGAL-PAGES.md` | Notes about operator-owned Meta policy/data-deletion pages. |
| `deploy/nginx-legal-locations.conf` | Optional Nginx routes for those operator-owned pages. |

Files created at runtime, such as `.env.local`, `target/`, and `queue.sqlite3`, are **not** part of the source release.
