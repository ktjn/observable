# Dockerfile Clippy Cache Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `COPY . .` in the `planner` and `rust-ci` Dockerfile stages with selective copies so clippy is skipped when non-Rust files change, and add BuildKit target-dir cache mounts for incremental local builds.

**Architecture:** Four edits to `Dockerfile` only. The `planner` stage copies only Cargo manifests; `rust-ci` copies only Rust-relevant files (`Cargo.toml`, `Cargo.lock`, `libs/`, `services/`, `proto/`); both `rust-ci` and `rust-builder` gain `--mount=type=cache` for registry, git, and the target directory. No CI or script changes required.

**Tech Stack:** Docker BuildKit, cargo-chef, Rust

---

## File Map

| File | Action |
|------|--------|
| `Dockerfile` | Modify — 4 targeted edits |

---

## Task 1: Patch `Dockerfile`

**Files:**
- Modify: `Dockerfile`

Current file for reference (all line numbers below reference the file as it exists today):

```
Line 17: FROM chef AS planner
Line 18: COPY . .
Line 19: RUN cargo chef prepare --recipe-path recipe.json
...
Line 27: FROM cacher AS rust-ci
Line 28: COPY . .
Line 29: RUN cargo fmt --check \
Line 30:     && cargo clippy --workspace --all-targets -- -D warnings \
Line 31:     && cargo test --workspace --lib --bins
Line 32: (blank)
Line 33: FROM rust-ci AS rust-builder
Line 34: RUN cargo build --release --workspace \
...
```

---

- [ ] **Step 1: Fix `planner` COPY**

Replace the `planner` stage (lines 17–19):

```dockerfile
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json
```

With:

```dockerfile
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY libs libs
COPY services services
RUN cargo chef prepare --recipe-path recipe.json
```

`cargo chef prepare` only reads Cargo manifests — `.rs` source files are not needed. This prevents changes to scripts, charts, testbench, or docs from invalidating the planner layer.

---

- [ ] **Step 2: Fix `rust-ci` COPY and add cache mounts to its RUN**

Replace the `rust-ci` stage (lines 27–31):

```dockerfile
FROM cacher AS rust-ci
COPY . .
RUN cargo fmt --check \
    && cargo clippy --workspace --all-targets -- -D warnings \
    && cargo test --workspace --lib --bins
```

With:

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

`proto/` is required because tonic services generate Rust code from proto files via `build.rs` at compile time. The target cache mount (`observable-cargo-target`) persists incremental compilation state locally; in CI it starts cold but the image layer's pre-built deps from the `cacher` stage remain visible through the mount overlay.

---

- [ ] **Step 3: Add cache mounts to `rust-builder` RUN**

Replace the `rust-builder` RUN (lines 34–42):

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

With:

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

The `cp` into `/app/bin/` runs inside the same RUN command while the cache mount is active, so the release binaries are written into the image layer correctly.

---

- [ ] **Step 4: Verify the full Dockerfile is well-formed**

Run:
```bash
docker build --dry-run . 2>&1 | head -30
```

If `--dry-run` is not available (older BuildKit), run a syntax-only parse:
```bash
docker buildx build --no-cache --target planner --output type=cacheonly . 2>&1 | tail -10
```

Expected: no parse errors, stages resolved correctly.

---

- [ ] **Step 5: Smoke-test — non-Rust file change does not bust rust-ci cache**

```bash
# First build (warms the cache)
docker build --progress=plain . --target rust-ci 2>&1 | grep -E "CACHED|RUN cargo"

# Touch a non-Rust file
touch scripts/testbench.sh

# Second build — rust-ci must be CACHED
docker build --progress=plain . --target rust-ci 2>&1 | grep -E "CACHED|RUN cargo"
```

Expected on second build: all three stages (`planner`, `cacher`, `rust-ci`) show `CACHED` — no `RUN cargo` output.

---

- [ ] **Step 6: Smoke-test — Rust source change does bust rust-ci cache**

```bash
# Touch a Rust source file
touch services/query-api/src/main.rs

# Build — rust-ci must NOT be cached
docker build --progress=plain . --target rust-ci 2>&1 | grep -E "CACHED|RUN cargo"
```

Expected: `planner` and `cacher` show `CACHED`; `rust-ci` shows `RUN cargo` executing.

---

- [ ] **Step 7: Smoke-test — full build produces a valid image**

```bash
docker build . --tag observable-services:local
docker run --rm observable-services:local auth-service --version 2>&1 || \
  docker run --rm --entrypoint auth-service observable-services:local --help 2>&1 | head -5
```

Expected: image builds to completion, binary is executable.

---

- [ ] **Step 8: Commit**

```bash
git add Dockerfile
git commit -m "perf(docker): skip clippy on non-Rust changes; add incremental target cache"
```

---

## Verification

| Scenario | Expected |
|---|---|
| `touch scripts/testbench.sh && docker build --target rust-ci .` | All layers CACHED |
| `touch services/query-api/src/main.rs && docker build --target rust-ci .` | `planner`+`cacher` CACHED, `rust-ci` rebuilt |
| `docker build . --tag observable-services:local` | Builds to completion |
| Push to GitHub → GHA `Build and load final image` | Cache hits on unchanged Rust files |
