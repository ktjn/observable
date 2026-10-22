# syntax=docker/dockerfile:1.28@sha256:bb22d9815c728170f72750f4e5b0d672e06176142e1d602c7e66c050100b7e5b

# --- Rust Build ---
FROM lukemathwalker/cargo-chef:0.1.77-rust-1.97.0-bookworm@sha256:974d873357408b6ececac8729285204664e23f5aab21a34e353ae5bf9bb899a2 AS chef
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

FROM cacher AS rust-source
COPY Cargo.toml Cargo.lock ./
COPY libs libs
COPY services services
COPY proto proto
COPY contracts contracts
FROM rust-source AS rust-ci
RUN --mount=type=cache,id=observable-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=observable-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=observable-cargo-target,target=/app/target,sharing=locked \
    cargo fmt --check \
    && cargo clippy --workspace --all-targets -- -D warnings \
    && cargo test --workspace --lib --bins

FROM rust-source AS rust-builder
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

# --- Per-component images (Phase 7, docs/component-decomposition.md) ---
# One image per deployable component. `SERVICE` is the Cargo package name
# (auth-service, query-api, ...); the component-builder builds only that
# package, so a source change to one component does not rebuild another
# component's binary. The existing shared `observable-services` image above is
# unchanged during the migration.
#
# The /app/target mount is a BuildKit cache: it exists only for the duration of
# the RUN that declares it. Copy the built binary to a real path under /app
# inside the same RUN (mirroring rust-builder) so the stage actually carries it
# forward; otherwise component-runtime copies the cargo-chef `cook` dummy
# binary baked into the cacher layer, and every component image exits
# immediately on start.
FROM cacher AS component-builder
ARG SERVICE
COPY Cargo.toml Cargo.lock ./
COPY libs libs
COPY services services
COPY proto proto
RUN --mount=type=cache,id=observable-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=observable-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=observable-cargo-target,target=/app/target,sharing=locked \
    cargo build --release -p "${SERVICE}" \
    && mkdir -p /app/bin \
    && cp "/app/target/release/${SERVICE}" "/app/bin/${SERVICE}"

FROM debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8 AS component-runtime
ARG SERVICE
ENV SERVICE=${SERVICE}
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=component-builder /app/bin/${SERVICE} /usr/local/bin/${SERVICE}
COPY proto/otlp /proto/otlp
USER 65532:65532
ENTRYPOINT ["/bin/sh", "-c", "exec /usr/local/bin/$SERVICE"]

# --- grpcurl downloader ---
FROM debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8 AS grpcurl-downloader
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
    && curl -fsSL https://github.com/fullstorydev/grpcurl/releases/download/v1.9.3/grpcurl_1.9.3_linux_x86_64.tar.gz \
       | tar -xz -C /usr/local/bin grpcurl

# --- Verification tooling (Compose smoke/perf/soak runners; no service binaries) ---
FROM debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8 AS verification-tools
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl jq \
    && rm -rf /var/lib/apt/lists/*
COPY --from=grpcurl-downloader /usr/local/bin/grpcurl /usr/local/bin/grpcurl
COPY proto/otlp /proto/otlp
USER 65532:65532

# --- Final Runtime Image (CI variant: binaries built natively on the runner via sccache) ---
FROM debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8 AS runtime-ci
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 curl jq

COPY --from=grpcurl-downloader /usr/local/bin/grpcurl /usr/local/bin/grpcurl
COPY bin/auth-service /usr/local/bin/auth-service
COPY bin/storage-writer /usr/local/bin/storage-writer
COPY bin/stream-processor /usr/local/bin/stream-processor
COPY bin/ingest-gateway /usr/local/bin/ingest-gateway
COPY bin/query-api /usr/local/bin/query-api
COPY bin/alert-evaluator /usr/local/bin/alert-evaluator
COPY bin/admin-service /usr/local/bin/admin-service
COPY proto/otlp /proto/otlp

USER 65532:65532

# --- Per-component runtime, CI variant (prebuilt binaries staged in bin/) ---
FROM debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8 AS component-runtime-ci
ARG SERVICE
ENV SERVICE=${SERVICE}
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 curl \
    && rm -rf /var/lib/apt/lists/*
COPY bin/${SERVICE} /usr/local/bin/${SERVICE}
COPY proto/otlp /proto/otlp
USER 65532:65532
ENTRYPOINT ["/bin/sh", "-c", "exec /usr/local/bin/$SERVICE"]

# --- Final Runtime Image ---
FROM debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8 AS runtime
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
