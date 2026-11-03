FROM rust:1-bookworm AS builder

WORKDIR /build
ENV CARGO_BUILD_JOBS=2
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY static ./static
COPY deploy ./deploy
RUN cargo build --release --locked

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 igbot \
    && useradd --uid 10001 --gid 10001 --no-create-home --shell /usr/sbin/nologin igbot \
    && install -d -o 10001 -g 10001 /data

COPY --from=builder /build/target/release/ig-reply-bot /usr/local/bin/ig-reply-bot

ENV IG_REPLY_BIND_ADDR=0.0.0.0:3000 \
    IG_DATABASE_PATH=/data/ig_reply_bot.sqlite3

USER 10001:10001
VOLUME ["/data"]
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl --fail --silent http://127.0.0.1:3000/healthz || exit 1
ENTRYPOINT ["/usr/local/bin/ig-reply-bot"]
