# syntax=docker/dockerfile:1.24

# --- Rust Build ---
FROM lukemathwalker/cargo-chef:0.1.77-rust-1.95.0-bookworm AS chef
WORKDIR /app
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        cmake \
        libcurl4-openssl-dev \
        libssl-dev \
        pkg-config \
        zlib1g-dev \
    && rustup component add rustfmt clippy
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY libs libs
COPY services services
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS cacher
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,id=observable-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=observable-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    cargo chef cook --release --all-targets --recipe-path recipe.json

FROM cacher AS rust-ci
COPY Cargo.toml Cargo.lock ./
COPY libs libs
COPY services services
COPY proto proto
RUN --mount=type=cache,id=observable-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=observable-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=observable-cargo-target,target=/app/target,sharing=locked \
    cargo fmt --check \
    && cargo clippy --workspace --all-targets -- -D warnings \
    && cargo test --workspace --lib --bins

FROM rust-ci AS rust-builder
RUN --mount=type=cache,id=observable-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=observable-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=observable-cargo-target,target=/app/target,sharing=locked \
    cargo build --release --workspace \
    && mkdir -p /app/bin \
    && cp /app/target/release/auth-service \
          /app/target/release/storage-writer \
          /app/target/release/stream-processor \
          /app/target/release/ingest-gateway \
          /app/target/release/query-api \
          /app/target/release/alert-evaluator \
          /app/target/release/admin-service \
          /app/bin/

# --- grpcurl downloader ---
FROM debian:bookworm-slim AS grpcurl-downloader
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
    && curl -fsSL https://github.com/fullstorydev/grpcurl/releases/download/v1.9.3/grpcurl_1.9.3_linux_x86_64.tar.gz \
       | tar -xz -C /usr/local/bin grpcurl

# --- Final Runtime Image ---
FROM debian:bookworm-slim AS runtime
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 curl jq

COPY --from=grpcurl-downloader /usr/local/bin/grpcurl /usr/local/bin/grpcurl
COPY --from=rust-builder /app/bin/auth-service /usr/local/bin/auth-service
COPY --from=rust-builder /app/bin/storage-writer /usr/local/bin/storage-writer
COPY --from=rust-builder /app/bin/stream-processor /usr/local/bin/stream-processor
COPY --from=rust-builder /app/bin/ingest-gateway /usr/local/bin/ingest-gateway
COPY --from=rust-builder /app/bin/query-api /usr/local/bin/query-api
COPY --from=rust-builder /app/bin/alert-evaluator /usr/local/bin/alert-evaluator
COPY --from=rust-builder /app/bin/admin-service /usr/local/bin/admin-service
COPY proto/otlp /proto/otlp

USER 65532:65532
