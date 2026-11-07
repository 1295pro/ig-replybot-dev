# Testing and releases

For a version tag, the release workflow builds ready-to-run packages for Windows x64, Linux x64, macOS Intel, and Apple Silicon. Each build checks formatting, runs Clippy and the automated tests, builds the app, and checks startup with a clean profile before packaging. The workflow adds a SHA256 checksum beside each ZIP. A manual UI or documentation rebuild can skip Clippy and tests only after a full run has passed.

A live acceptance check passed on the local build: a new trigger comment received the configured public reply and DM while the VPS copy was stopped and the laptop used polling. That confirms this setup worked during the test; it does not guarantee the same result for every Meta app or account. Meta controls eligibility, token validity, permissions, limits, and delivery.

## Run checks from source

Install Rust, then run these commands from the project folder:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --release
```

The platform builds and desktop startup checks run in GitHub Actions: [CI workflow](../.github/workflows/ci.yml) · [release workflow](../.github/workflows/release.yml).
