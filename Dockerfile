# syntax=docker/dockerfile:1
FROM lukemathwalker/cargo-chef:latest-rust-1.89-bookworm AS chef
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        cmake \
        libssl-dev \
        pkg-config \
        zlib1g-dev \
    && rm -rf /var/lib/apt/lists/*

RUN rustup component add clippy rustfmt

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS cacher
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

FROM cacher AS builder
COPY . .
RUN cargo build --release --workspace \
    && mkdir -p /app/bin \
    && cp /app/target/release/auth-service \
          /app/target/release/storage-writer \
          /app/target/release/stream-processor \
          /app/target/release/ingest-gateway \
          /app/target/release/query-api \
          /app/target/release/alert-evaluator \
          /app/bin/

FROM cacher AS tester
COPY . .
RUN cargo test --release --workspace

FROM cacher AS linter
COPY . .
RUN cargo clippy --release --all-targets --all-features -- -D warnings

FROM chef AS formatter
COPY . .
RUN cargo fmt --check

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 curl jq \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/bin/auth-service /usr/local/bin/auth-service
COPY --from=builder /app/bin/storage-writer /usr/local/bin/storage-writer
COPY --from=builder /app/bin/stream-processor /usr/local/bin/stream-processor
COPY --from=builder /app/bin/ingest-gateway /usr/local/bin/ingest-gateway
COPY --from=builder /app/bin/query-api /usr/local/bin/query-api
COPY --from=builder /app/bin/alert-evaluator /usr/local/bin/alert-evaluator

USER 65532:65532
