#!/bin/sh
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
if [ -x ./ig-reply-bot ]; then
    exec ./ig-reply-bot --local
fi
if ! command -v cargo >/dev/null 2>&1; then
    echo 'Download the ready-to-run app from https://github.com/1295pro/ig-replybot-dev/releases/latest'
    echo 'To build this source download, install Rust from https://rustup.rs/ first.'
    exit 1
fi
cargo build --release --locked
exec ./target/release/ig-reply-bot --local
