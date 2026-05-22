#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RELEASE_CANDIDATE_SUITES_SOURCE_ONLY="${RELEASE_CANDIDATE_SUITES_SOURCE_ONLY:-0}"
RELEASE_CANDIDATE_STACK_CLEANUP=0

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "FAIL: required command '$name' is not available"
    exit 1
  fi
}

run_stage() {
  local label="$1"
  shift

  echo ""
  echo "=== $label ==="
  "$@"
}

main() {
  require_command docker
  require_command bash
  require_command kind
  require_command kubectl
  require_command helm

  local core_services=(
    auth-service
    storage-writer
    stream-processor
    ingest-gateway
    query-api
    alert-evaluator
    frontend
  )

  cleanup() {
    if [[ $RELEASE_CANDIDATE_STACK_CLEANUP -eq 1 ]]; then
      docker compose down --remove-orphans >/dev/null 2>&1 || true
    fi
  }
  trap cleanup EXIT

  run_stage "Core stack" docker compose up -d --wait "${core_services[@]}"
  RELEASE_CANDIDATE_STACK_CLEANUP=1

  run_stage "Load baseline" docker compose run --rm perf-smoke
  run_stage "Tenant escape smoke" docker compose run --rm smoke-test
  run_stage "Chaos probe" bash "$SCRIPT_DIR/chaos-smoke.sh"
  run_stage "Upgrade and rollback" bash "$SCRIPT_DIR/kind-test.sh"

  echo ""
  echo "=== P4-S8 RELEASE-READINESS SUITE PASSED ==="
}

if [[ "$RELEASE_CANDIDATE_SUITES_SOURCE_ONLY" != "1" ]]; then
  main "$@"
fi
