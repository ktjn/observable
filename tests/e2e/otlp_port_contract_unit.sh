#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

assert_contains() {
  local file="$1"
  local expected="$2"
  local message="$3"
  if ! grep -Fq "$expected" "$file"; then
    echo "FAIL: $message"
    echo "expected to find: $expected"
    echo "in file: $file"
    exit 1
  fi
}

run_test() {
  local name="$1"
  shift

  echo "==> $name"
  "$@"
}

test_perf_smoke_uses_http_port_for_http_ingest() {
  assert_contains \
    "$REPO_ROOT/scripts/perf-smoke.sh" \
    'INGEST="${INGEST_URL:-http://localhost:4318}"' \
    "perf-smoke should default HTTP ingest traffic to OTLP/HTTP port 4318"
}

test_canary_promote_http_fallback_uses_http_port() {
  assert_contains \
    "$REPO_ROOT/scripts/canary-promote.sh" \
    '|| echo "4318")' \
    "canary-promote should fall back to OTLP/HTTP port 4318 for HTTP health and smoke gates"
}

run_test "perf-smoke uses OTLP/HTTP port" test_perf_smoke_uses_http_port_for_http_ingest
run_test "canary promote fallback uses OTLP/HTTP port" test_canary_promote_http_fallback_uses_http_port

echo "PASS: OTLP port contract defaults"
