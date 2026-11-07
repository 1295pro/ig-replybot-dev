Draft: automated desktop checks pass on all four platforms. Real startup, account connection, and polling work on the test laptop after the owner disabled Smart App Control. The fresh-comment acceptance test is awaiting the operator. See docs/RELEASE_VALIDATION.md.

IG Reply Bot brings comment-to-DM automation to a browser control room.

- Connect your own Instagram Business or Creator account using a Meta access token.
- Set a trigger word, public reply, and DM in the interface.
- Run on a laptop with polling, or deploy a separate Docker instance to an Ubuntu/Debian VPS through the SSH wizard.
- Local launch creates panel access automatically and keeps settings in your user data folder between updates.
- Check polling, setup progress, and individual delivery results from the control room.

Download the platform ZIP, extract it, and open its launcher. Ready-to-run packages include the compiled app. Source downloads require Rust. All packages are unsigned; SHA256 checksums are included. Windows 11 Smart App Control can block unsigned apps without a per-app exception; a trusted signed distribution is needed for that mode.

Use your own Meta app and token. Meta controls permission approval and token validity. Keep a laptop awake for local operation. Tokens require manual renewal, saved credentials are not encrypted at rest, and failed/uncertain deliveries are not retried automatically. See the setup guide for the complete operating limits.
