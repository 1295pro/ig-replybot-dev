# Release validation — 2026-11-07

Status: real laptop-to-Instagram acceptance passed; release prepared for publication.

## Verified

[Desktop build run 42324927177](https://github.com/1295pro/ig-replybot-dev/actions/runs/42324927177) completed successfully for Windows x64, Linux x64, macOS Apple Silicon, and macOS Intel. The application source is commit `1cf9a35`.

Each platform passed formatting, Clippy with warnings denied, 25 unit/API tests, an optimized build, and the clean-profile desktop lifecycle check. The lifecycle check uses the real compiled executable and its HTTP API to verify:

- First launch starts paused, with English defaults and no connected account.
- Local mode works without a valid environment file and ignores service environment variables.
- The panel rejects missing/wrong credentials.
- Reply settings and the generated password survive a process restart.
- A second launch reuses the running instance.
- A conflicting profile fails before opening another SQLite database.
- Automation cannot be enabled before connecting an account.

These checks use no live Instagram account and send no real comments or DMs. Windows release builds link the C runtime statically. Platform ZIPs contain executable code, launchers, documentation, and the license; SHA256 checksums accompany them.

The existing VPS was updated to the same application version. Its container is healthy, identifies the configured Instagram account, reports healthy polling, and has no queued or attention-required deliveries. Its earlier completed delivery is not counted as a laptop acceptance test.

## Browser interface check

The current HTML was also exercised in a browser against a loopback fixture with sample data. Automatic fragment sign-in removed the fragment from the address bar, the VPS installer appeared only in VPS mode, unsaved trigger text survived a real 20-second background-refresh interval, and saving updated the visible trigger. The README screenshot uses this sample profile and contains no real Instagram credentials or account data. This is a frontend check, not a real Meta delivery test.

[UI-only rebuild 42326627016](https://github.com/1295pro/ig-replybot-dev/actions/runs/42326627016) successfully built and smoke-tested all four platform packages from `e76d86b`. Formatting and the full Linux CI also passed for that source. The Rust delivery logic is unchanged from the full four-platform test run.

## Physical Windows laptop limitation

The downloaded Windows artifact's checksum was verified. Starting it on the owner's laptop was blocked by Windows Application Control (error 4551; Code Integrity event 3077). Smart App Control is enabled. Local compiler build helpers were blocked by the same mechanism. Windows Subsystem for Linux is not installed.

The app is currently unsigned. See the [documented launch limitation](USER_GUIDE.md#windows-launch-limitation) and [Microsoft's guidance](https://support.microsoft.com/en-us/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions). After explicitly authorizing the change, the owner turned Smart App Control off through Windows Security. Its state was read back as Off, and the downloaded executable then ran successfully. The launcher itself does not change security settings.

## Physical laptop startup verified

The latest Windows package from `e76d86b` starts on the laptop, opens the real browser, signs in automatically, and displays the connected account. The clean profile initially had no account and automation was paused. The token and reply settings were saved through the same authenticated HTTP API used by the UI; no environment file was created or edited. Polling without a webhook completed its initial baseline of 81 comments and reported healthy.

For the real delivery test, the VPS container was explicitly stopped and verified exited before enabling the local bot. The local queue initially had zero completed deliveries. A fresh trigger comment from a different account was then processed successfully.

## Real delivery acceptance passed

The owner left a fresh exact-trigger comment from another Instagram account and reported receiving the reply. During that delivery, the VPS bot container was verified `exited`. The local instance had no VPS deployment and no webhook configured; it used outbound polling only.

The local SQLite queue changed from zero to one completed job. Its public reply and private reply both had status `sent`, the control-room API reported `live_test_passed=true`, and there were no pending or attention-required deliveries. A read through Meta's API independently confirmed exactly one public reply with the configured text.

After the test, local automation was paused and its process closed. The successful delivery record and seen-comment history were reconciled into the stopped VPS database after a private SQLite backup, preventing polling or webhook retries from answering the same test comment again. The server was restarted and enabled; it reports healthy polling, zero pending deliveries, and zero jobs needing attention. Its two completed records include the earlier server delivery and the transferred local test record. Meta still showed exactly one matching public reply after restoration.

The local profile retains its account, reply rules, test result, and paused automation state for the next launch. Browser automation stopped before capturing a final Passed screenshot because it could not reliably identify the Windows browser URL; the result was verified through the actual API, SQLite state, Meta, and the owner's response instead.

## Publication

The code, English UI documentation, ready-to-run ZIPs, checksums, and draft release are prepared. Publishing the release and making the currently private repository public await the owner's approval. Windows packages remain unsigned; the Smart App Control limitation is documented for other users.
