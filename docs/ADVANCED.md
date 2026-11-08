# Developer and service reference

The normal desktop flow is the [control room](USER_GUIDE.md). This reference is for source builds, existing environment-based installations, and manual services.

## Source builds and checks

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
cargo run --release --locked -- --local
```

Use `--local --no-browser --data-dir PATH` for an isolated local profile. Desktop mode uses the selected user data folder and binds to `127.0.0.1:3000`. The packaged executable opens in desktop mode by default. Use `--service` to run it with environment configuration. Source builds and the Docker image default to service mode.

## Existing service configuration

Service mode reads process environment, `.env.local`, then `.env` for missing values. Saved SQLite settings override account/reply/automation values on restart. Listener address, database path, and admin password remain service settings.

| Variable | Purpose |
| --- | --- |
| `IG_ADMIN_PASSWORD` | Unique panel password, at least 32 characters. |
| `IG_REPLY_BIND_ADDR` | Listener; default `127.0.0.1:3000`. |
| `IG_DATABASE_PATH` | SQLite path; default `ig_reply_bot.sqlite3`. |
| `IG_AUTOMATION_ENABLED` | Initial sending state; default `false`. |
| `IG_POLL_ENABLED` | Initial polling setting; default `true`. |
| `IG_POLL_INTERVAL_SECONDS` | 30–3600 seconds; default `60`. |
| `IG_ACCESS_TOKEN` / `IG_USER_ID` | Account credentials, normally entered through the panel. |
| `IG_KEYWORD` | Whole-comment trigger; default `bot`. |
| `PUBLIC_REPLY_TEXT` / `PRIVATE_REPLY_TEXT` | Public reply and DM, normally saved in the panel. |
| `META_API_VERSION` | Graph API version, currently `v26.0`. |
| `META_APP_SECRET` | Signature verification for optional webhooks. |
| `META_WEBHOOK_VERIFY_TOKEN` | Operator-chosen webhook handshake token. |

`.env.example` is a credential-free template. Keep actual environment files and databases private. Existing service profiles are not silently imported into desktop local mode. To migrate account/reply settings, pause the old instance, enter them in the new control room, let polling establish its baseline, and leave a new test comment.

## Manual server operation

Build with `cargo build --release --locked`, create a dedicated service user, and adapt `deploy/ig-reply-bot.service` for your paths. Keep the environment file outside the repository with mode `0600`. The `deploy/` directory also contains the Docker build and Compose definition used by the UI installer.

Polling-only operation needs no public app port. Optional webhooks use `GET /webhook` for verification and signed `POST /webhook` events. Configure an existing HTTPS proxy to forward only that callback path; the Nginx/Caddy examples use `/ig/webhook`. Keep the admin routes private or access them through SSH forwarding.

`/healthz` checks process/database liveness. `/readyz` also requires a recent successful Meta poll when automation and polling are enabled. The authenticated `/api/admin/status` reports configuration, queue state, and polling health without returning secrets.

## Release packaging

`.github/workflows/release.yml` runs formatting, lint, and tests before building platform ZIPs. A manual run produces workflow artifacts. A version tag also prepares a draft GitHub release. Platform archives contain only the executable, launchers, README, and license. Source environment files and databases are never packaged. SHA256 checksums accompany the ZIPs.

Use the release candidate's clean-profile smoke check and the real local comment acceptance test before publishing the draft. A workflow build is not a substitute for confirming a public reply and DM on an actual Instagram account.
