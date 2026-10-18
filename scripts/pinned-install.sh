#!/usr/bin/env bash
set -euo pipefail

# pinned-install.sh
#
# Installs the distribution's pinned component set through Compose and verifies
# it end to end (docs/component-decomposition.md Phase 7 sub-step 5). It builds
# every deployable component image tagged with its manifest version, brings the
# full stack up from distribution/compose.pinned.yaml (waiting for the web UI
# healthcheck), then runs the end-to-end smoke test against the pinned versions
# rather than the `local` developer tags.
#
# Usage:
#   bash scripts/pinned-install.sh
#
# Environment:
#   COMPOSE_PROJECT_NAME  isolated project name (recommended)

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

command -v docker >/dev/null 2>&1 || { echo "FAIL: docker not found" >&2; exit 1; }

COMPOSE=(docker compose -f docker-compose.yml -f distribution/compose.pinned.yaml --profile verification)

# The override must match the manifest before anything is built or started.
bash "$REPO_ROOT/scripts/render-pinned-compose.sh" --check

echo "==> Building every component image tagged with its manifest version"
TAG=manifest bash "$REPO_ROOT/scripts/build-component-images.sh"

echo "==> Starting the pinned application stack (web UI + backends) and waiting for health"
# The web UI's nginx resolves every backend upstream at startup, so the whole
# stack must be up before it can become healthy.
docker compose -f docker-compose.yml -f distribution/compose.pinned.yaml up -d --wait

echo "==> Running the end-to-end smoke test against the pinned set"
status=0
"${COMPOSE[@]}" up smoke-test --abort-on-container-exit || status=$?

"${COMPOSE[@]}" down --remove-orphans >/dev/null 2>&1 || true

if [[ $status -ne 0 ]]; then
  echo "FAIL: pinned install verification failed (exit $status)" >&2
  exit "$status"
fi

echo "OK  pinned install verified (web UI healthy, smoke test passed)"
