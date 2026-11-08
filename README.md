# IG Reply Bot

IG Reply Bot watches comments on one Instagram Business or Creator account. When a new comment matches your trigger, it posts your public reply and sends your DM. You set it up in a browser; it can run on your computer or on a VPS.

[Download the latest version](https://github.com/1295pro/ig-replybot-dev/releases/latest) · [Quick start](docs/QUICK_START.md) · [Full setup guide](docs/USER_GUIDE.md) · [MIT license](LICENSE)

![IG Reply Bot control room in Self-hosted mode](docs/images/control-room.jpg)


## Get started

1. Download the ZIP for your operating system from **Releases** and extract it.
2. On Windows, double-click **start-local.cmd**. On macOS, open **start-local.command**. On Linux, run **./start-local.sh**.
3. The control room opens in your browser and signs you in automatically.
4. Connect Instagram using an access token from your own Meta app.
5. Save your trigger word, public reply, and private message. Choose **Self-hosted** to keep the bot on this computer.
6. Enable the bot. Once polling is healthy, leave a **new** trigger-word comment from another account and check for both replies.

The computer needs to stay awake, online, and running the app. Comments are checked every minute by default. Polling works on a normal home connection; you do not need a static IP, public domain, or VPS.

For example, with the trigger `bot`, a new comment saying `BOT!` receives your public reply followed by one DM. `I want a bot` does not match. Existing comments form the first-run baseline and are not answered.

Windows builds are unsigned. Windows 11 Smart App Control can block them without offering a per-app exception. Check the [Windows launch limitation](docs/USER_GUIDE.md#windows-launch-limitation) before choosing the Windows download.

## What you need from Meta

Use an Instagram **Business or Creator** account and your own Meta developer app with Instagram Login. The token needs `instagram_business_basic`, `instagram_business_manage_comments`, and `instagram_business_manage_messages`. The account connection form links to Meta's dashboard and checks your token for you.

Use Meta's dashboard to create the app and get your token. Follow the [full setup guide](docs/USER_GUIDE.md#connect-instagram) if you are setting this up for the first time or Meta asks for a website or policy links. Test the public reply and DM with a new comment from an Instagram account outside your app's roles.

## Publish policy pages for Meta

Some Meta app setups ask for public Privacy Policy, Terms of Service, and Data Deletion pages. You can publish your own pages with Google Sites; a custom domain is not required. Start with the [Instagram Reply Bot policy template](https://sites.google.com/view/ig-replybot-1295-policies/), copy its page text into a site you control, then publish your own URLs and enter those links in your Meta app.

**IMPORTANT: EDIT THE TEMPLATE BEFORE SUBMITTING IT TO META.** Replace every placeholder with your own name or business, Instagram account, contact details, hosting provider, and accurate information about your bot’s data, retention, security, and deletion process. Review every page, remove all bracketed instructions, and publish only after the text accurately describes your installation. Do not submit the template unchanged. Meta may require additional app setup or review; publishing policy pages does not guarantee approval.

## Run continuously on a VPS

Start the app on your computer and save your account and reply settings. Choose **VPS**, enter your server details, check its SSH fingerprint, and select **Check server**. If it finds your existing bot, open its control room. For a new installation, continue with **Deploy and run checks**.

The installer supports Ubuntu/Debian on x64 or ARM64. It creates a separate Docker project, checks Meta polling, and opens a private connection to the server’s control room. Pause the laptop copy before deploying. The VPS keeps working after your laptop is closed. To manage an existing VPS bot, select **Open VPS control room**. The desktop launcher opens the copy on your computer at `http://127.0.0.1:3000/`.

[Full VPS walkthrough](docs/USER_GUIDE.md#move-to-a-vps)

## Your data

Settings survive app restarts in your user data folder. The app uses Meta's official API and keeps your token, reply settings, comment IDs, and delivery status on the computer or VPS running it. Tokens and saved secrets are not encrypted at rest. Keep that device and its backups private.

Meta controls account eligibility, token expiration, permissions, API limits, and message delivery. Tokens currently need manual renewal. Each instance handles one Instagram account. Failed or uncertain message deliveries are shown for review rather than automatically resent.

## Build from source

Install [Rust](https://rustup.rs/), clone this repository, and run:

```sh
cargo run --release --locked -- --local
```

Windows users can also double-click `start-local.cmd` in the source folder. Building on Windows requires the Microsoft C++ build tools requested by Rust. Ready-to-run release downloads include the compiled app.

[For contributors: project files](docs/USER_GUIDE.md#project-files) · [Advanced setup](docs/ADVANCED.md) · [How releases are tested](docs/TESTING.md) · [Architecture](docs/ARCHITECTURE.md)
