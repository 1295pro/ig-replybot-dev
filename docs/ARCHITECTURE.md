# Runtime architecture

## Event sources

1. The poller reads media and comments for the configured professional account through Meta's Graph API. It scans all current comments on first startup to establish a durable baseline, without replying to them.
2. Later polls fetch comments on media whose comment count changed, then recheck those media on the next three polls because Meta can update a count before a comment appears in the list. Every 10 completed polls, it scans all media again to catch additions that coincided with deletions or stale counts. A failed or incomplete poll does not advance the baseline or count cache.
3. New comment IDs are stored in SQLite. Exact keyword comments from another account enter the same delivery queue as webhook events. The database does not keep comment text.

## Webhook path

1. Meta verifies the callback with `GET /webhook`; the service returns the challenge only when the mode and verify token match.
2. For `POST /webhook`, the service checks `X-Hub-Signature-256` against the exact request bytes before parsing or acting on the event. The body limit is 1 MiB.
3. The event filter accepts only the configured Instagram account, new comment events, comments from another account, valid Meta IDs, and an exact configured keyword.
4. A matching event is stored in SQLite with a unique comment ID. This makes webhook redelivery and overlap with polling idempotent. The database stores IDs, timestamps, and delivery state; it does not store comment text or the full webhook body.
5. A background sender posts the public comment reply. Only after Meta accepts that step does it attempt one private reply to the comment.

## Failure behavior

- Invalid signatures and malformed events never enter the queue. A polling failure is logged and retried at the next interval.
- Database errors return a non-success response so Meta can redeliver; the unique comment ID prevents duplicate queue rows.
- A rejected API request is recorded as failed. A timeout or interrupted request is recorded as uncertain. Neither state is automatically retried because Meta may already have applied the action.
- Sender configuration is checked before automation is enabled; invalid tokens keep the bot paused and are shown in the control room.
- At startup the service resolves the token's canonical account ID and username from Meta. It uses both to avoid treating the connected account's own comments as customer comments.
- `/healthz` reports process liveness; `/readyz` reports whether required runtime dependencies are usable. With polling enabled, readiness also requires a recent successful poll. Automation state is visible without exposing credentials.

## Trust boundaries and secrets

- The service accepts webhook events only when their HMAC signature matches the configured Meta app secret.
- The browser control room and its JSON endpoints require `IG_ADMIN_PASSWORD`. The page stores it in browser session storage only. Use the panel locally or through an SSH tunnel.
- Access tokens, app secrets, and webhook settings saved in the control room are stored unencrypted in the SQLite database. On Unix, the database file is set to mode `0600`; the process and systemd unit use umask `0077`. Keep database backups private. The admin API never returns secrets or writes them to logs.
- The VPS installer is available only from a loopback-bound control room and accepts loopback peers. It first reads the SSH host key without sending the password; authenticated SSH and SFTP connections require that exact SHA-256 fingerprint. It runs fixed installer commands and uploads an allowlisted source manifest, never the local environment file, database, or Git directory.
- The installer keeps the root SSH password in memory for the request and does not persist it. It writes the Instagram token, Meta settings, and new VPS admin password into a root-owned `runtime.env` file with mode `0600`; these values are not encrypted at rest and remain visible to VPS root and the Docker daemon. The admin listener is published on VPS loopback and should be reached over SSH forwarding.
- The VPS deployment uses a distinct Compose project and named SQLite volume. It does not alter firewall, reverse proxy, DNS, systemd, or existing containers. Preflight checks for a known running IG Reply Bot process/service/container and refuses to add a second instance. Docker is installed only on Ubuntu/Debian when missing and no known conflicting packages are installed.
- The HTTP listener binds to loopback by default. Public deployments terminate HTTPS at a reverse proxy and forward only the exact `/webhook` callback path. Do not proxy `/` or `/api/admin/*` to the public internet.
- The Meta client uses a fixed HTTPS Graph API host, bearer-token authentication, bounded timeouts, and no redirects.

## Control room

The panel is served by the same process at `/`. It connects an account by checking its token against Meta's `/me` endpoint, saves settings to the local SQLite store, and broadcasts updates to the webhook handler, sender, and poller without restarting the process. Automation starts paused by default. The public webhook route remains separate from the authenticated admin API.

The **Deploy to VPS** flow takes the saved account and reply configuration from this panel, requires the local bot to be paused, and starts a new private Docker deployment over pinned SSH. It polls the remote status endpoint until the container identifies the expected Instagram account and reports a healthy Meta poll. This proves service startup and account polling only; the operator still needs to leave a fresh trigger-word comment from another account and confirm the public reply and DM. Failed automated checks best-effort pause the remote bot and leave its files in place for diagnosis.

## Configuration precedence

Process environment values take precedence over `.env.local`; `.env.local` takes precedence over `.env`. Missing values in one file may be supplied by the next source. The repository template is `.env.example` and contains placeholders only.

## Release checks

The release gate is formatting, linting, unit tests, and an optimized build. Local API-contract tests use a loopback fake Graph API and never contact Instagram or send real replies. A real acceptance test checks a new comment on the connected account, its queue state, and Meta's public reply and private message responses. App Review may still be required before a single Meta app can serve other people's professional accounts.
