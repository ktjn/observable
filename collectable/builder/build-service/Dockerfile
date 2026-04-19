# Build stage — compile the build-service binary
FROM rust:bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY builder/build-service/Cargo.toml ./Cargo.toml
COPY builder/build-service/src ./src
RUN cargo build --release

# Runtime stage — full Rust toolchain kept for on-demand mediator compilation
FROM rust:bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    gcc-aarch64-linux-gnu \
    binutils-aarch64-linux-gnu \
    cmake pkg-config libssl-dev ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# Proper musl cross-compilers from musl.cc (Debian's musl-tools uses a hybrid
# gcc that defaults to PIE/dynamic and cannot produce working static binaries).
RUN curl -fsSL https://musl.cc/x86_64-linux-musl-cross.tgz \
        | tar xzf - -C /opt \
    && curl -fsSL https://musl.cc/aarch64-linux-musl-cross.tgz \
        | tar xzf - -C /opt

# Cross-compilation targets
RUN rustup target add \
    x86_64-unknown-linux-musl \
    aarch64-unknown-linux-musl \
    x86_64-unknown-linux-gnu \
    aarch64-unknown-linux-gnu

# Linker configuration for cross targets
# musl.cc toolchains produce fully static binaries with +crt-static
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/opt/x86_64-linux-musl-cross/bin/x86_64-linux-musl-gcc
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=/opt/aarch64-linux-musl-cross/bin/aarch64-linux-musl-gcc
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc

# Copy the build-service binary
COPY --from=builder /app/target/release/build-service /usr/local/bin/build-service

# Copy mediator library and templates (used by codegen at runtime)
COPY mediator /mediator

ENV MEDIATOR_TEMPLATES_DIR=/mediator/templates
ENV MEDIATOR_SRC_DIR=/mediator
ENV RUST_LOG=info

EXPOSE 8091
CMD ["/usr/local/bin/build-service"]
