# Dockerfile Clippy Cache Optimization

## Context

The main `Dockerfile` builds 6 Rust services via a multi-stage pipeline using `cargo-chef`. The `rust-ci` stage runs `cargo fmt`, `cargo clippy`, and `cargo test`. Both the `planner` and `rust-ci` stages use `COPY . .`, which copies the entire repo into the build context. Any file change — including scripts, Helm charts, testbench code, or docs — busts the layer cache and forces clippy to re-run from scratch, even though those files have no effect on Rust compilation.

This spec covers four targeted changes to `Dockerfile` that ensure clippy only runs when Rust source files actually change, and add an incremental compilation cache for local builds.

## Goal

- **Clippy skipped entirely** when only non-Rust files change (scripts, charts, testbench, docs, etc.)
- **Clippy fast** (incremental) when `.rs` files change — only changed files are re-checked
- **No CI changes required** — GitHub Actions `cache-to: type=gha,mode=max` already handles everything

## Scope

One file: `Dockerfile`.

No changes to:
- `.github/workflows/build.yml`
- `scripts/kind-test.sh` / `scripts/testbench.sh`
- Any other Dockerfile

## Changes

### 1. `planner` stage — selective COPY

`cargo chef prepare` only reads `Cargo.toml`/`Cargo.lock` files to generate `recipe.json`. It does not need `.rs` source files or non-Rust directories.

**Before:**
```dockerfile
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json
```

**After:**
```dockerfile
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY libs libs
COPY services services
RUN cargo chef prepare --recipe-path recipe.json
```

When scripts, charts, or testbench files change, the planner layer stays valid and the `recipe.json` content is unchanged, so the `cacher` layer also stays valid.

### 2. `rust-ci` stage — selective COPY

Only Rust-relevant files affect compilation and linting.

**Before:**
```dockerfile
FROM cacher AS rust-ci
COPY . .
RUN cargo fmt --check \
    && cargo clippy --workspace --all-targets -- -D warnings \
    && cargo test --workspace --lib --bins
```

**After:**
```dockerfile
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
```

`proto/` is included because tonic services use `build.rs` to generate Rust code from proto files at compile time.

### 3. `rust-ci` RUN — BuildKit cache mounts

Three cache mounts are added to the `rust-ci` RUN:

| Mount | Cache ID | Purpose |
|---|---|---|
| `/usr/local/cargo/registry` | `observable-cargo-registry` | Already in `cacher`; added here for consistency when clippy runs |
| `/usr/local/cargo/git` | `observable-cargo-git` | Same |
| `/app/target` | `observable-cargo-target` | Persists incremental compilation state between builds |

**Target cache behavior:**

Cache mounts overlay on top of the image layer — they do not replace it. The pre-built dependencies from the `cacher` stage remain visible through the mount. On first use (cold cache), the image layer provides the deps. On subsequent local builds, the cache stores the source compilation artifacts, making clippy incremental.

In CI (GitHub Actions ephemeral runners), the mount is always cold. The image layer's pre-built deps are still visible, so CI performance is unchanged from today.

### 4. `rust-builder` stage — BuildKit cache mounts

Same registry, git, and target cache mounts added to `rust-builder` for incremental local release builds.

**Before:**
```dockerfile
FROM rust-ci AS rust-builder
RUN cargo build --release --workspace \
    && mkdir -p /app/bin \
    && cp /app/target/release/auth-service \
          /app/target/release/storage-writer \
          /app/target/release/stream-processor \
          /app/target/release/ingest-gateway \
          /app/target/release/query-api \
          /app/target/release/alert-evaluator \
          /app/bin/
```

**After:**
```dockerfile
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
          /app/bin/
```

The final `cp` into `/app/bin/` runs inside the same RUN command while the cache mount is active, so the release binaries are correctly copied into the image layer.

## Cache Hit Table

| What changed | `planner` | `cacher` | `rust-ci` | Outcome |
|---|---|---|---|---|
| scripts, charts, testbench, docs | ✅ hit | ✅ hit | ✅ hit | Clippy **skipped** |
| `.rs` source only | ✅ hit† | ✅ hit† | ❌ miss → incremental | Clippy **fast** |
| `Cargo.toml` / `Cargo.lock` | ❌ miss | ❌ miss | ❌ miss | Full rebuild |
| Nothing | ✅ hit | ✅ hit | ✅ hit | Clippy **skipped** |

†`recipe.json` content is unchanged when only source changes, so `cacher`'s `COPY --from=planner` layer stays valid.

## Verification

1. **Smoke test — non-Rust change:**
   ```bash
   touch scripts/testbench.sh
   docker build . --target rust-ci
   # Expected: "CACHED" for planner, cacher, and rust-ci layers — clippy not re-run
   ```

2. **Smoke test — Rust source change:**
   ```bash
   touch services/query-api/src/main.rs
   docker build . --target rust-ci
   # Expected: rust-ci layer rebuilt, but fast (incremental) on second run
   ```

3. **Smoke test — full build:**
   ```bash
   docker build . --tag observable-services:local
   # Expected: final image builds successfully
   ```

4. **CI:** Push branch, verify GHA `Build and load final image` step shows cache hits on unchanged Rust files.
