#!/usr/bin/env bash
set -euo pipefail

# pinned-install.sh
#
# Installs the distribution's pinned component set through Compose and runs the
# end-to-end smoke test against it (docs/component-decomposition.md Phase 7
# sub-step 5). It builds each Rust component image tagged with its manifest
# version, then brings the stack up with distribution/compose.pinned.yaml so the
# exact pinned versions run, not the `local` developer tags.
#
# Usage:
#   bash scripts/pinned-install.sh
#
# Environment:
#   COMPOSE_PROJECT_NAME  isolated project name (recommended)

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

command -v docker >/dev/null 2>&1 || { echo "FAIL: docker not found" >&2; exit 1; }

# The override must match the manifest before anything is built or started.
bash "$REPO_ROOT/scripts/render-pinned-compose.sh" --check

echo "==> Building component images tagged with their manifest versions"
TAG=manifest bash "$REPO_ROOT/scripts/build-component-images.sh" \
  auth control ingest process store-clickhouse query alerting

echo "==> Bringing up the pinned component set and running the smoke test"
status=0
docker compose -f docker-compose.yml -f distribution/compose.pinned.yaml \
  --profile verification up smoke-test --abort-on-container-exit || status=$?

docker compose -f docker-compose.yml -f distribution/compose.pinned.yaml \
  --profile verification down --remove-orphans >/dev/null 2>&1 || true

if [[ $status -ne 0 ]]; then
  echo "FAIL: pinned install smoke test failed (exit $status)" >&2
  exit "$status"
fi

echo "OK  pinned install smoke test passed"
