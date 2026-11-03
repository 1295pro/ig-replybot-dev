# Self-hosted deployment example

The bot can run on a server with polling alone: it makes outbound requests to Meta and needs no public URL, open inbound port, or reverse proxy. The examples below add an optional HTTPS webhook callback for faster delivery. They assume the bot listens on `127.0.0.1:3000` and expose only the exact webhook path through your existing reverse proxy. Do not expose the admin panel or plain HTTP listener directly.

## Automatic Docker setup from the control room

For a new VPS deployment, start the control room on your computer, connect Instagram, save the trigger/reply settings, pause local automation, and use **Deploy to VPS**. The installer supports root SSH on Ubuntu/Debian `amd64` and `arm64`. It verifies the VPS SSH fingerprint before sending the password, checks for a known running bot and port conflicts, and creates a distinct Compose project under `/opt/ig-reply-bot-<id>`. It installs Docker from Docker's official Ubuntu/Debian repository only when Docker is missing and no known conflicting Docker packages are present. The host admin port stays on loopback; use the generated SSH tunnel command to open the dashboard.

The wizard's success check confirms that the container started, the expected Instagram account was loaded, and a Meta poll succeeded. It does not send a test comment or prove a public reply/DM. After success, leave a new trigger-word comment from a different Instagram account and verify the public reply and DM. The wizard does not update/remove deployments or change any firewall, proxy, DNS, systemd service, or existing container. See [the operator guide](../docs/USER_GUIDE.md#deploy-a-separate-docker-instance-from-the-control-room) for the full steps, diagnostics, and stop/cleanup instructions.

The installer transfers the Meta token, settings, and VPS panel password to a root-owned `runtime.env` file with mode `0600`. They are unencrypted at rest and visible to VPS root and the Docker daemon. The SSH password is not saved. Keep the service directory and its Docker volume private.

## Public endpoints

- Meta callback: `https://YOUR-DOMAIN/ig/webhook`
The proxy removes `/ig` for the callback so Axum receives `/webhook`. Add the matching Caddy or Nginx snippet to the existing HTTPS site configuration; avoid creating a second site block for the same hostname.

## Example service layout

- Binary: `/opt/ig-reply-bot/ig-reply-bot`
- Private environment file: `/etc/ig-reply-bot/ig-reply-bot.env`
- Persistent SQLite queue: `/var/lib/ig-reply-bot/queue.sqlite3`
- systemd unit: `ig-reply-bot.service`

Create a dedicated unprivileged `ig-reply-bot` system user. Install the binary and unit, set a unique random `IG_ADMIN_PASSWORD` in the private environment file, and restrict that file to the administrator. The control room saves Meta credentials and bot settings to the private SQLite database. Connect to the panel with an SSH tunnel (`ssh -L 3000:127.0.0.1:3000 USER@YOUR-SERVER`) and open `http://127.0.0.1:3000/`. Enable automation only after the account and replies are configured. Polling needs no webhook setup.

The required environment variable names are listed in the repository's `.env.example`. Do not copy a local `.env.local` into GitHub or send credentials in chat. The Instagram access token and Meta app secret belong only on the host running the bot.

## Deployment checklist

1. Install the service and check `http://127.0.0.1:3000/healthz` from the server itself.
2. Open the control room through SSH, connect the token, save reply settings, and enable automation. Wait for the first successful poll.
3. Write a new exact-keyword comment from another Instagram account and confirm both the public reply and DM. That commenter does not need an app role.
4. Optional webhook: confirm the existing HTTPS website remains healthy, add one proxy snippet, set Meta's callback to `https://YOUR-DOMAIN/ig/webhook`, enter the matching verify token and app secret in the panel, then subscribe the connected account to `comments`.

Rollback consists of stopping and disabling the systemd unit and removing only the `/ig/` proxy route. Preserve the rest of the site's proxy and DNS configuration.
