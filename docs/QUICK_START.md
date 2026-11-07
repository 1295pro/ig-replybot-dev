# Quick start

Use this short route if you already have an Instagram access token. If you are creating a Meta app or need a website for its policy links, start with the [full setup guide](USER_GUIDE.md).

1. [Download the release ZIP](https://github.com/1295pro/ig-replybot-dev/releases/latest) for your computer and extract it.
2. Open `start-local.cmd` on Windows, `start-local.command` on macOS, or `./start-local.sh` on Linux.
3. The browser opens the control room and signs you in. New installations start paused.
4. Open **Connect Instagram**, paste your token, and select **Verify and connect**.
5. Choose your trigger word and write the public reply and DM. Select **Save reply rules**.
6. For laptop operation, choose **Self-hosted** and keep polling enabled. Pause other copies for the same Instagram account, then select **Enable bot**.
7. Wait for healthy polling. Leave a new trigger-word comment from another Instagram account that is not added to your Meta app's roles, then check that both the public reply and DM arrive. The first scan skips existing comments.

Keep the app window open and the computer awake. Closing the browser tab does not stop the bot. Use **Pause bot** to stop replies, or press Ctrl+C in the app window to exit.

To run continuously on a VPS, choose **VPS** and follow the installer instead. See the [full guide](USER_GUIDE.md) for Meta setup, saved data, server deployment, and troubleshooting.
