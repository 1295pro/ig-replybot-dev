# Instagram comment-to-DM bot

A small self-hosted Rust service with a browser control room. It checks comments on your Instagram Professional account and accepts signed comment webhooks. When a new comment is exactly the configured keyword, it posts a public reply and sends one private reply through Meta's official Instagram API.

Open `http://127.0.0.1:3000/` after starting the service to connect an Instagram account, set reply rules, pause or enable automation, and check queue health. The control room can also build and install a separate Docker deployment on a compatible VPS over SSH. Start with the [operator guide](docs/USER_GUIDE.md) for laptop-only, UI-assisted VPS, or manual Linux-server setup. See [the runtime architecture](docs/ARCHITECTURE.md) for delivery and security details.

## What it does

- Matches the whole comment, ignoring case, punctuation, whitespace, and emoji around the keyword. With the default keyword, `🤖 BOT!` matches; `I want a bot` and `bots` do not.
- Ignores comment edits and comments written by the connected Instagram account.
- Deduplicates polled comments and webhook events by comment ID and sends the public reply before the private reply.
- Polls your account every 60 seconds by default. It scans current comments once on first startup as a baseline, without replying to old comments. Later scans queue new matching comments. A webhook can queue the same comment sooner.
- Keeps automation off by default. While it is off, signed webhook requests are acknowledged but not queued.
- Stores comment, media, account, and commenter IDs plus delivery state in a local SQLite database. It does not store comment text or full webhook payloads. Rows are currently retained until the database is removed.
- Uses only official Meta API calls. It does not ask for Instagram passwords or browser cookies.

Meta limits private replies to one message per comment and generally requires sending it within seven days of the comment. For Live comments, the reply must be sent during the broadcast. Check Meta's current rules before deployment.

## Requirements

- Rust stable, including Cargo.
- An Instagram Professional account (Business or Creator).
- Your own Meta developer app configured for Instagram Login and the Instagram permissions listed below. The `comments` webhook field is optional when polling is enabled.
- A public HTTPS URL if you want Meta to deliver webhooks. Polling works without an incoming webhook connection. The local development server binds to `127.0.0.1` and is not directly reachable by Meta.

Meta changes its dashboard labels and API requirements over time. Follow the current Instagram Platform setup in Meta's dashboard and documentation. In Development mode, app roles govern who can authorize a professional account; ordinary commenters do not need an app role. Use App Review and the required access level before offering one app to other professional account owners.

## Get it from GitHub

Replace the repository address with the GitHub URL where this project is published.

```powershell
git clone https://github.com/YOUR-NAME/ig-reply-bot.git
cd ig-reply-bot
```

On macOS or Linux, the same commands work in Terminal. Install Rust from [rustup.rs](https://rustup.rs/) first if Cargo is not available.

## Configure your own Meta app

Each person running the bot must use their own Meta app and Instagram access token. Do not use the maintainer's credentials.

1. Convert the Instagram account to a Business or Creator account if it is not already Professional.
2. Create a Meta developer app and add the current Instagram Login/API setup offered in the dashboard.
3. Request the permissions required by this code: `instagram_business_basic`, `instagram_business_manage_comments`, and `instagram_business_manage_messages`.
4. Create a long-lived access token for the Instagram Professional account with those permissions. This service does not refresh tokens automatically; monitor expiration and renew the token using Meta's current flow.
5. Paste the token into the control room's account connection form. The bot verifies the token and reads the account ID and username for you.
6. Webhooks are optional. For faster delivery, configure the `comments` webhook field and a public HTTPS callback. The route is `/webhook`; with the included reverse proxy snippets, use `https://YOUR-DOMAIN/ig/webhook`. Add the Meta app secret and a separate random verify token in the control room. Polling works without a public callback.

Meta's official guides: [Instagram API with Instagram Login](https://developers.facebook.com/docs/instagram-platform/instagram-api-with-instagram-login/) and [Private Replies](https://developers.facebook.com/docs/instagram-platform/private-replies/).

## Configure and run locally

Copy the sample settings file and set a unique admin password. The panel handles the Instagram account and reply settings. `.env.local` is ignored by Git.

```powershell
Copy-Item .env.example .env.local
notepad .env.local
```

On macOS or Linux:

```sh
cp .env.example .env.local
chmod 600 .env.local
nano .env.local
```

Set `IG_ADMIN_PASSWORD` to at least 32 random characters. Leave automation disabled until you connect an account in the control room. The remaining defaults are:

```dotenv
IG_AUTOMATION_ENABLED=false
IG_POLL_ENABLED=true
IG_POLL_INTERVAL_SECONDS=60
META_API_VERSION=v26.0
IG_KEYWORD=bot
PUBLIC_REPLY_TEXT=Thanks for commenting!
PRIVATE_REPLY_TEXT=Here is the information you asked for.
IG_REPLY_BIND_ADDR=127.0.0.1:3000
IG_DATABASE_PATH=ig_reply_bot.sqlite3
```

Keep automation set to `false` while configuring the app. Run the local checks and server:

```sh
cargo test --locked
cargo run --release
```

Open `http://127.0.0.1:3000/` and sign in with `IG_ADMIN_PASSWORD`. In the control room, connect an Instagram Professional account with a token from your own Meta app, set the trigger and replies, then enable automation when ready. `/healthz` and `/readyz` remain available for service checks. The webhook routes are `GET /webhook` for Meta's verification handshake and `POST /webhook` for signed events. Meta cannot reach this local address; use a public HTTPS reverse proxy only for the webhook callback. Do not expose the admin panel to the internet.

If using webhooks, enter the same verify token and callback URL in Meta's dashboard, then subscribe the account to `comments`. Keep polling enabled as a fallback. Enable automation from the control room; the first complete poll records existing comments without answering them. After that, write a new exact-keyword comment from another account and check for the public reply and DM. Comments by the connected account itself are ignored.

The dashboard's sample “send to server” event checks callback delivery only. It does not represent a real keyword comment and will not make the bot send a public reply or DM.

## Deploy a separate VPS instance from the control room

Start the control room on your own computer, connect the Instagram account, save the trigger/reply settings, and pause automation on that computer. Open **Deploy to VPS**. The wizard supports a root SSH login to Ubuntu or Debian on `amd64` or `arm64`; it checks the SSH host fingerprint before sending the password, then checks Docker, Compose, port availability, and known running IG Reply Bot instances. If Docker is missing, it installs Docker Engine and Compose from Docker's official Ubuntu/Debian repository. Review the server changes and explicitly confirm before installation.

The wizard uploads an allowlisted source bundle and the bot's current settings to a new private folder under `/opt`. It builds a dedicated Compose project and waits for the first successful Meta poll before showing the live test instruction. The admin port is bound to VPS loopback only. Use the generated SSH tunnel command to open the VPS control room from your computer, then make a **new** trigger-word comment from another Instagram account and confirm both the public reply and DM.

The SSH password is used for the wizard session and not saved. The Instagram token, Meta settings, and new VPS control-room password are saved in the VPS's root-owned `runtime.env` file with mode `0600`; they are not encrypted at rest and remain accessible to VPS root and the Docker daemon. Keep a separate record of the VPS control-room password. The wizard does not edit a firewall, proxy, DNS, systemd service, or existing container. If it detects a known running IG Reply Bot instance, stop that copy yourself before deploying to avoid duplicate replies. A later code update is a separate operation; this wizard currently creates a fresh deployment and does not upgrade or remove one.

## Settings

| Variable | Purpose |
| --- | --- |
| `META_WEBHOOK_VERIFY_TOKEN` | Secret string chosen by the operator for Meta's webhook handshake. |
| `META_APP_SECRET` | App Secret from the operator's Meta app; used to verify webhook signatures. |
| `IG_AUTOMATION_ENABLED` | Initial state, `false` by default. After setup, use the control room to pause or enable replies. |
| `IG_ADMIN_PASSWORD` | Required to use the browser control room; use a unique random value with at least 32 characters. |
| `IG_POLL_ENABLED` | `true` by default; polls account media and comments when automation is enabled. |
| `IG_POLL_INTERVAL_SECONDS` | Time between polls, 30–3600 seconds; defaults to 60. Changed media are rechecked for three more polls, and every tenth completed poll includes a full comment scan. |
| `IG_USER_ID` | Instagram Professional account ID. |
| `IG_ACCESS_TOKEN` | Token for that account and Meta app. |
| `META_API_VERSION` | Graph API version; update it when Meta retires the configured version. |
| `IG_KEYWORD` | Exact keyword to match. Defaults to `бот`. |
| `PUBLIC_REPLY_TEXT` | Text posted under the matching comment. |
| `PRIVATE_REPLY_TEXT` | One private reply sent to the commenter. |
| `IG_REPLY_BIND_ADDR` | Listener address; defaults to `127.0.0.1:3000`. |
| `IG_DATABASE_PATH` | SQLite queue path; defaults to `ig_reply_bot.sqlite3`. |

Initial configuration uses process environment, then `.env.local`, then `.env` for any values still missing. After settings are saved in the control room, they are stored in the private SQLite database and take precedence over environment values on restart. `IG_ADMIN_PASSWORD` always comes from the environment. Never commit environment files or paste their contents into public issues or chat.

## Production deployment

Keep the service bound to `127.0.0.1:3000`. Polling-only operation needs no public address or reverse proxy. If you want webhooks, put the service behind an existing HTTPS reverse proxy. The included `deploy/` folder has example Nginx and Caddy routes that forward only `/ig/webhook` to the local `/webhook` route. Edit the examples for your domain and existing web server; do not create a second server block that conflicts with your site.

For a new VPS, the control room's **Deploy to VPS** wizard can create an isolated Docker deployment over SSH; see the operator guide for supported operating systems and the exact steps. The systemd example remains a manual alternative. It runs as a dedicated unprivileged user and restricts filesystem writes to the database directory. Store the production environment file outside the repository with permissions limited to the service administrator. Start with automation disabled and check the control room and health routes. If you enable webhooks, verify the callback handshake too. Then enable automation and perform a controlled test.

## Security and delivery behavior

- POST bodies are capped at 1 MiB. The `X-Hub-Signature-256` HMAC is checked before parsing.
- API requests use HTTPS, bounded timeouts, and disabled redirects.
- The panel checks account ID and username using the configured access token. This also prevents replies to comments written by the connected account when Instagram uses a different commenter ID for it.
- `.env*` files (except `.env.example`), SQLite databases, and Rust build output are excluded by `.gitignore`.
- Do not expose port 3000 directly. Keep the app secret and access token private, and rotate them if exposed.
- A Meta HTTP rejection is recorded as failed. If a network error happens after a request may have reached Meta, its outcome is marked uncertain and it is not automatically retried, to reduce duplicate replies.
- Polling remembers seen comment IDs in SQLite. The first complete scan is a baseline, so enabling the bot does not answer every old keyword comment. Keep this database across restarts or older comments may be treated as a new baseline.
- The SQLite database contains account, media, commenter, and comment IDs, delivery state, and settings saved in the control room, including unencrypted access tokens and app secrets. On Linux, the service uses private file permissions and a restrictive umask. Keep the database, backups, and containing directory private. It has no automatic retention policy yet; removing the database deletes settings, queue, and deduplication history.
- Meta controls account eligibility, app review, rate limits, token validity, and message delivery. A successful webhook response alone does not guarantee that Meta accepted a reply.

## Checks

```sh
cargo test --locked
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
```

To check locked dependencies against current Rust security advisories, install `cargo-audit` and run `cargo audit`. Review any advisory before deploying; do not suppress findings without understanding their impact.

## License

MIT. See [LICENSE](LICENSE).
