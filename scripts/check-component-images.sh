#!/usr/bin/env bash
set -euo pipefail

# Git Bash converts POSIX-looking arguments (e.g. the container binary path
# passed to `docker run`) into Windows paths; disable that for this script.
export MSYS_NO_PATHCONV=1

# check-component-images.sh
#
# Verifies the per-component Compose images actually carry their service
# binary. Regression guard for the build-layer bug that shipped a 438 KB
# cargo-chef `cook` stub in every component image: the `component-builder`
# stage built into a BuildKit cache mount at /app/target (present only for the
# RUN) and then component-runtime copied /app/target/release/${SERVICE},
# which resolved to the dummy binary baked into the cacher layer. A real
# service binary is tens of megabytes; a stub exits immediately.
#
# Companion to `docker compose --profile verification build` in
# scripts/local-ci.sh. The static Dockerfile pattern is pinned by
# scripts/tests/test_component_builds.py; this is the behavioral check.
#
# Usage:
#   bash scripts/check-component-images.sh
#
# Environment:
#   TAG  image tag (default: local)

TAG=${TAG:-local}

# Minimum plausible size of a real component binary. Go/Rust release binaries
# for these services are ~19 MB; the stub is ~0.4 MB. Use a floor well above
# the stub and well below the real binary.
MIN_BYTES=$((1024 * 1024))

# component:package pairs. Keep in sync with scripts/ci_changed_components.py
# and the `image:` entries in docker-compose.yml (see
# scripts/tests/test_component_builds.py).
COMPONENTS=(
  auth:auth-service
  control:admin-service
  ingest:ingest-gateway
  process:stream-processor
  store-clickhouse:storage-writer
  query:query-api
  alerting:alert-evaluator
)

failures=0

for entry in "${COMPONENTS[@]}"; do
  component="${entry%%:*}"
  package="${entry#*:}"
  image="observable-${component}:${TAG}"
  binary="/usr/local/bin/${package}"

  if ! docker image inspect "$image" >/dev/null 2>&1; then
    echo "MISSING image ${image} (build it with: docker compose --profile verification build)"
    (( ++failures ))
    continue
  fi

  size="$(docker run --rm --entrypoint /usr/bin/stat "$image" -c %s "$binary" 2>/dev/null || true)"
  if [[ -z "$size" ]]; then
    echo "MISSING ${binary} in ${image}"
    (( ++failures ))
    continue
  fi
  if (( size < MIN_BYTES )); then
    echo "STUB ${binary} in ${image} is ${size} bytes (< 1 MiB); the component build did not copy the real binary out of the cache-mounted target"
    (( ++failures ))
    continue
  fi
  echo "OK  ${image} -> ${binary} (${size} bytes)"
done

if (( failures > 0 )); then
  echo "FAIL ${failures} component image(s)" >&2
  exit 1
fi

echo "OK  all component images carry a real service binary"
