FROM rust:1.89-bookworm AS builder

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        cmake \
        libssl-dev \
        pkg-config \
        zlib1g-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY libs ./libs
COPY services ./services

RUN cargo build --release --workspace

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 curl jq \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/auth-service /usr/local/bin/auth-service
COPY --from=builder /app/target/release/storage-writer /usr/local/bin/storage-writer
COPY --from=builder /app/target/release/stream-processor /usr/local/bin/stream-processor
COPY --from=builder /app/target/release/ingest-gateway /usr/local/bin/ingest-gateway
COPY --from=builder /app/target/release/query-api /usr/local/bin/query-api

USER 65532:65532
