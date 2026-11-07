# Release validation — 2026-11-07

Status: candidate prepared; real laptop-to-Instagram acceptance remains pending.

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

UI-only refinements following the full platform run are rebuilt separately; the Rust delivery logic is unchanged.

## Physical Windows laptop limitation

The downloaded Windows artifact's checksum was verified. Starting it on the owner's laptop was blocked by Windows Application Control (error 4551; Code Integrity event 3077). Smart App Control is enabled. Local compiler build helpers were blocked by the same mechanism. Windows Subsystem for Linux is not installed.

The app is currently unsigned. See the [documented launch limitation](USER_GUIDE.md#windows-launch-limitation) and [Microsoft's guidance](https://support.microsoft.com/en-us/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions). No Windows protection setting was changed. The VPS bot remains running.

## Remaining acceptance gate

Run the candidate in a permitted local environment. Pause/stop the VPS bot, connect the account through the local control room, save the replies, and enable local polling. After the baseline succeeds, a different Instagram account must leave a new trigger comment. Verify a completed public reply plus private reply in the local queue and confirm the received DM. Pause the local copy and restore the server afterward.

Publishing the release and opening repository access should follow this acceptance step. The repository is currently private.
