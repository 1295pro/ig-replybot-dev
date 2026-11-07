# IG Reply Bot

Connect your own Instagram Business or Creator account, set a trigger and two replies, then run the bot on your computer or on a VPS.

## Included

- A browser control room for your account, trigger, public reply, and DM.
- Local polling for a laptop or home server. No public IP, domain, or port forwarding is needed for this mode.
- An installer that creates a separate Docker service on Ubuntu or Debian VPS hosts.
- Separate status for polling, public replies, and private replies.

## Before you start

- Create your own Meta app and connect an Instagram access token with the required permissions.
- Keep a local computer awake and online while the bot is running, or use the VPS installer for continuous operation.
- Tokens and saved settings are not encrypted at rest. Protect the computer, VPS, and backups that contain the app data.
- Tokens need manual renewal. The bot handles one Instagram account per process.
- Failed or uncertain deliveries are shown for review and are not automatically retried.
- The ready-to-run packages are unsigned. Windows 11 Smart App Control may block the Windows build without offering a per-app exception.

Meta controls account access, token validity, API limits, and message delivery. See the [quick start](docs/QUICK_START.md), [full setup guide](docs/USER_GUIDE.md), and [testing notes](docs/TESTING.md) before deploying.
