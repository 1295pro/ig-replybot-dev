# IG Reply Bot

Connect your own Instagram Business or Creator account, set a trigger and two replies, then run the bot on your computer or on a VPS.

## What's new in 1.0.5

Open the downloaded Windows EXE to start the desktop control room. A new profile starts empty and paused. The desktop app reads its VPS settings from that profile, and the panel now makes it clearer which copy of the bot you are managing.

## What's new in 1.0.4

Select **Open VPS control room** after checking or installing a VPS bot. The desktop app opens the private SSH connection and remembers the server address and confirmed fingerprint for the next visit.

## What's new in 1.0.3

**Check server** now recognizes an existing Docker installation for the connected Instagram account. It shows the bot's status and how to open its control room through SSH, using the same private port and password.

## What's new in 1.0.2

The setup guide now uses the same ports as the app: 3000 for the desktop control room and 12950 for a VPS tunnel with the default deployment settings.

## What's new in 1.0.1

- Every platform ZIP now includes the updated setup guide and instructions for publishing your own Meta policy pages with Google Sites.
- The VPS screen distinguishes a saved server address from the live control room. The guide shows how to reopen the existing VPS panel through SSH while keeping the desktop panel on port 3000.

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

Meta controls account access, token validity, API limits, and message delivery. See the [quick start](https://github.com/1295pro/ig-replybot-dev/blob/v1.0.5/docs/QUICK_START.md), [full setup guide](https://github.com/1295pro/ig-replybot-dev/blob/v1.0.5/docs/USER_GUIDE.md), and [testing notes](https://github.com/1295pro/ig-replybot-dev/blob/v1.0.5/docs/TESTING.md) before deploying.
