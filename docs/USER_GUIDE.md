# Setup and usage

## Choose where the bot runs

| Mode | What stays online | Incoming public connection |
| --- | --- | --- |
| Self-hosted on a laptop or home server | Your computer and the app | Not needed with polling |
| VPS | The VPS and its Docker container | SSH for installation; a public app port is not needed with polling |

Both modes use the same control room and reply rules. The **VPS / Self-hosted** switch shows your server information or the laptop instructions. The controls below always manage the instance opened in this browser tab. A saved VPS address in the laptop panel is an installation record, not a live connection to the remote bot.

## Download and open

Download and extract your operating system's ZIP from [Releases](https://github.com/1295pro/ig-replybot-dev/releases/latest).

- **Windows x64:** double-click `start-local.cmd`.
- **macOS 12+:** open `start-local.command` from the ZIP matching your Mac (Apple Silicon or Intel).
- **Linux x64 (glibc 2.39+, such as Ubuntu 24.04+):** run `./start-local.sh` in the extracted folder.

The packaged apps include the executable. The first launch creates a private panel password and opens `http://127.0.0.1:3000/` with automatic sign-in. New installations start paused. Subsequent launches reuse your settings; opening the launcher again while the app is running reopens its panel.

The builds are unsigned. macOS may require approval before opening them; Windows Smart App Control can block them entirely. Download only from this repository's releases. Use the included SHA256 checksums to verify a download if needed.

A source-code ZIP is different from a platform release ZIP. For a source download, install Rust (and its requested Windows C++ build tools), then use the same launcher or `cargo run --release --locked -- --local`.

## Windows launch limitation

On Windows 11 with Smart App Control enabled, the unsigned executable may be blocked, including helper executables built from source. The app cannot grant itself an exception. Microsoft currently provides no per-app allow list for Smart App Control; a trusted signing certificate is the distribution solution. See [Microsoft's Smart App Control FAQ](https://support.microsoft.com/en-us/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions).

If Windows blocks the app, use a permitted environment or wait for a trusted signed distribution. The launcher does not change Windows security settings. This limitation is distinct from a browser sign-in or Instagram permission error.

## Connect Instagram

1. Use an Instagram **Business or Creator** account.
2. Open the [Meta App Dashboard](https://developers.facebook.com/apps/) and create your own app with Instagram Login. Follow Meta's current setup for the Instagram API.
3. Configure access to `instagram_business_basic`, `instagram_business_manage_comments`, and `instagram_business_manage_messages`, then generate an access token for your account. Record the expiration date; the bot does not renew tokens automatically.
4. In the control room, paste the token under **Connect Instagram** and select **Verify and connect**. The bot checks it with Meta and displays your username.

Each operator uses their own Meta app and credentials. Accounts outside an app's roles may require App Review and Advanced Access. Ordinary people leaving comments do not need to be Meta app testers. Meta controls permission eligibility and approval. See [Instagram API with Instagram Login](https://developers.facebook.com/docs/instagram-platform/instagram-api-with-instagram-login/).

If Meta asks for Privacy Policy, Terms of Service, or data-deletion URLs, host public pages that describe your own deployment, data retention, and contact/deletion process. Another operator's pages do not describe your service automatically. See [policy page notes](../deploy/LEGAL-PAGES.md).

The token is enough for polling. **Meta app secret** and **Webhook verify token** are used only for optional webhooks. A webhook requires a public HTTPS callback, so laptop operation normally uses polling.

## Set your replies

Under **Reply rules**, enter the exact trigger word, public reply, and DM. Select **Save reply rules**. Changes apply immediately.

The trigger matches the whole comment, ignoring case and surrounding punctuation, whitespace, and emoji. With `bot` as the trigger, `🤖 BOT!` matches; `I want a bot` and `bots` do not. The connected account's own comments and comment edits are ignored.

The bot sends the public reply first, then one private reply for that comment. The same comment is deduplicated across polling and webhooks.

## Run on your laptop

1. Choose **Self-hosted**, keep polling enabled, and pause any other bot copy connected to the same account.
2. Select **Enable bot** and confirm your reply rule.
3. Wait for polling to become healthy. The first full scan establishes a baseline; old comments are skipped.
4. From a different Instagram account, leave a **new** comment containing the trigger under a post on the connected account.
5. Confirm the public reply and DM. **Recent activity** shows each delivery; **Setup progress** shows **Passed** after a completed public reply and DM for this account.

The default interval is one minute, plus Meta's processing time. Keep the app running, your laptop awake, and internet connected. A sleeping or powered-off laptop cannot answer comments. Closing the browser tab keeps the bot running. **Pause bot** stops new automated replies; Ctrl+C in the app window exits the process. Pause before intentionally shutting down during a delivery.

## Move to a VPS

The installer runs from the local control room on your computer. It supports root SSH access to Ubuntu/Debian on x64 or ARM64.

1. Connect Instagram and save your replies locally, then **pause the local bot**.
2. Choose **VPS** and open the deployment form. Enter your server address, SSH port, and private panel port.
3. Select **Read SSH server fingerprint**, compare it with the provider's console, and confirm it.
4. Enter the root SSH password and a separate VPS panel password of at least 32 characters. Keep the panel password for future sign-in.
5. Select **Check server**. Review the detected OS, Docker/Compose, port availability, and existing bot check.
6. Confirm the installation plan, then select **Deploy and run checks**. Docker is installed if needed. The installer builds a separate container and checks the first successful Meta poll.
7. Use the displayed SSH tunnel command and open the private admin URL. Leave a new trigger comment from another account and confirm both replies.

After deployment, the server keeps running when your laptop is closed. VPS mode shows the saved address and panel port. **Change VPS** leads to the local installer; pause the old copy before starting a new one. A new deployment has a separate database and does not migrate history.

The wizard creates a new `/opt/ig-reply-bot-<id>` folder and Compose project. It does not upgrade/remove an existing bot or edit your firewall, DNS, proxy, or existing containers. The root SSH password is used for the request and not stored. The VPS `runtime.env` contains the bot credentials and panel password, readable by root/Docker. Its panel port binds to VPS loopback and is opened through the supplied SSH tunnel.

## Saved settings and backups

Local app data stays outside the download folder:

| System | App data folder |
| --- | --- |
| Windows | `%LOCALAPPDATA%\IG Reply Bot` |
| macOS | `~/Library/Application Support/IG Reply Bot` |
| Linux | `$XDG_DATA_HOME/ig-reply-bot`, or `~/.local/share/ig-reply-bot` |

`bot.sqlite3` contains the connected account token, reply rules, other saved secrets, seen comment IDs, and delivery status. `admin-password.txt` contains your generated panel password. These files are not encrypted at rest. The browser receives the password in a URL fragment, stores it for the tab's session, and immediately removes the fragment from the address bar; it is not sent in an HTTP request.

To back up or move a laptop installation, stop the app and copy the entire app data folder. Preserve both files and any SQLite sidecar files. To update, stop the app, replace the extracted release files, and start it again. App data remains in the same user folder. A source/service installation using `.env.local` is a separate mode; follow [the advanced reference](ADVANCED.md) when migrating it.

## Troubleshooting

- **Nothing opens:** keep the app window open and read its error. A source download needs Rust; a platform release ZIP includes the compiled app.
- **Port 3000 in use:** close the other app using it. A second launch with the same local profile reopens the existing bot instead of starting a second worker.
- **Sign-in screen:** launch the app again for automatic sign-in, or use `admin-password.txt` from the app data folder.
- **Account rejected:** check the token's expiration, permissions, app access level, and Business/Creator account status in Meta.
- **No reply:** wait for healthy polling, then leave a new exact trigger from another account. The initial scan skips older comments. Check recent activity and the app log for Meta errors.
- **App works only with assigned accounts:** check Meta App Review and the access level for the permissions your app uses.
- **Failed/uncertain delivery:** inspect the corresponding Instagram comment/DM before acting. The bot does not automatically retry these outcomes because Meta may already have received a request.

The bot handles one account per process. Tokens need manual renewal. Stored rows have no automatic retention cleanup. Each Meta collection is limited to 20 pages of up to 100 items; exceeding that fails the poll rather than silently omitting comments. A successful webhook handshake only checks callback reachability; use a real new comment to check replies.

## File map

| File | Purpose |
| --- | --- |
| `README.md` | App overview, download links, and UI setup. |
| `start-local.cmd` and `scripts/start-local.ps1` | Windows launcher for the packaged app or source build. |
| `docs/QUICK_START.md` | Short UI setup and first comment test. |
| `docs/USER_GUIDE.md` | Laptop and server setup, operations, and this file map. |
| `docs/ARCHITECTURE.md` | Event flow, trust boundaries, failure behavior, and release criteria. |
| `.env.example` | Safe settings template. Copy it; do not put real secrets here. |
| `.gitignore` | Keeps local secrets, SQLite files, and compiled output out of Git. |
| `start-local.sh` | macOS/Linux local launcher. Packaged macOS builds also include `start-local.command`. |
| `docs/ADVANCED.md` | Developer checks, environment reference, and manual service setup. |
| `.github/workflows/release.yml` | Builds and tests platform ZIPs; prepares draft release assets for version tags. |
| `.github/workflows/ci.yml` | GitHub Actions formatting, lint, test, and release-build checks. |
| `Cargo.toml` | Rust package metadata and dependencies. |
| `Cargo.lock` | Exact dependency versions used for reproducible builds. |
| `LICENSE` | MIT license. |
| `src/main.rs` | Loads settings, verifies account identity, starts the worker, poller, and HTTP server. |
| `src/local.rs` | Local user-data folder, generated panel password, browser opening, and relaunch handling. |
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
