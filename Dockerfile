# syntax=docker/dockerfile:1

# --- Rust Build ---
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
RUN cargo chef cook --release --all-targets --recipe-path recipe.json

FROM cacher AS rust-builder
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

FROM cacher AS rust-tester
COPY . .
RUN cargo test --release --workspace

FROM cacher AS rust-linter
COPY . .
RUN cargo clippy --release --all-targets --all-features -- -D warnings

FROM chef AS rust-formatter
COPY . .
RUN cargo fmt --check

# --- Frontend Build ---
FROM node:22-bookworm-slim AS frontend-builder
WORKDIR /app
# Copy root package files
COPY package*.json ./
# Copy frontend package files
COPY apps/frontend/package*.json ./apps/frontend/
RUN npm ci
COPY apps/frontend ./apps/frontend
RUN npm run build --workspace=apps/frontend

# --- Final Runtime ---
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 curl jq \
    && rm -rf /var/lib/apt/lists/*

# Copy backend binaries
COPY --from=rust-builder /app/bin/auth-service /usr/local/bin/auth-service
COPY --from=rust-builder /app/bin/storage-writer /usr/local/bin/storage-writer
COPY --from=rust-builder /app/bin/stream-processor /usr/local/bin/stream-processor
COPY --from=rust-builder /app/bin/ingest-gateway /usr/local/bin/ingest-gateway
COPY --from=rust-builder /app/bin/query-api /usr/local/bin/query-api
COPY --from=rust-builder /app/bin/alert-evaluator /usr/local/bin/alert-evaluator

# Copy frontend assets (dist/) - for reference/completeness, though in 
# production k8s they might be in a separate nginx container.
COPY --from=frontend-builder /app/apps/frontend/dist /usr/share/nginx/html

USER 65532:65532
