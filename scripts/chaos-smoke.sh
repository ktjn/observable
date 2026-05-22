#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SMOKE_SCRIPT="$REPO_ROOT/tests/e2e/smoke_test.sh"
CHAOS_SERVICE="${CHAOS_SERVICE:-storage-writer}"
CHAOS_HEALTH_URL="${CHAOS_HEALTH_URL:-http://localhost:4320/health}"
CHAOS_SMOKE_SOURCE_ONLY="${CHAOS_SMOKE_SOURCE_ONLY:-0}"
CHAOS_CLEANUP_REQUIRED=0

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "FAIL: required command '$name' is not available"
    exit 1
  fi
}

source_smoke_helpers() {
  SMOKE_TEST_SOURCE_ONLY=1 source "$SMOKE_SCRIPT"
}

wait_for_health() {
  local attempts="${1:-30}"
  local delay_seconds="${2:-1}"

  for _ in $(seq 1 "$attempts"); do
    if curl -sf "$CHAOS_HEALTH_URL" >/dev/null 2>&1; then
      echo " OK ($CHAOS_SERVICE healthy)"
      return 0
    fi
    sleep "$delay_seconds"
  done

  echo " FAIL: $CHAOS_SERVICE did not become healthy at $CHAOS_HEALTH_URL"
  return 1
}

main() {
  require_command curl
  require_command jq
  require_command docker
  source_smoke_helpers

  echo "=== P4-S8 Chaos Smoke ==="

  local run_id
  local primary_trace_id
  local followup_trace_id
  local service_name
  local primary_payload
  local followup_payload
  run_id="${RUN_ID:-$(date +%s%N)}"
  service_name="chaos-svc-${run_id}"
  primary_trace_id="$(printf '%032x' "$((10#$run_id % 4294967295))")"
  followup_trace_id="$(printf '%032x' "$((((10#$run_id % 4294967295) + 1) % 4294967295))")"

  cleanup() {
    if [[ $CHAOS_CLEANUP_REQUIRED -eq 1 ]]; then
      docker compose up -d "$CHAOS_SERVICE" >/dev/null 2>&1 || true
    fi
  }
  trap cleanup EXIT

  SERVICE_NAME="$service_name"
  TRACE_ID="$primary_trace_id"
  primary_payload="{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"$SERVICE_NAME\"}}]},\"scopeSpans\":[{\"spans\":[{\"traceId\":\"$TRACE_ID\",\"spanId\":\"00f067aa0ba902b7\",\"name\":\"chaos-baseline\",\"startTimeUnixNano\":\"$(date +%s%N)\",\"endTimeUnixNano\":\"$(( $(date +%s%N) + 5000000 ))\",\"status\":{\"code\":1}}]}]}]}"

  echo "1. Seeding baseline trace..."
  send_trace_until_queryable "$primary_payload" 3

  echo "2. Verifying tenant escape still fails closed..."
  CROSS_TENANT_RESULT=$(curl -sf \
    -H "X-Tenant-ID: 00000000-0000-0000-0000-000000000001" \
    -H "Authorization: Bearer $TOKEN" \
    "$QUERY/v1/traces/$TRACE_ID" || true)
  CROSS_SPAN_COUNT=$(echo "$CROSS_TENANT_RESULT" | jq '.spans | length' 2>/dev/null || echo 0)
  if [[ ! "$CROSS_SPAN_COUNT" =~ ^[0-9]+$ ]]; then
    CROSS_SPAN_COUNT=0
  fi
  if [[ "$CROSS_SPAN_COUNT" -ne 0 ]]; then
    echo "FAIL: cross-tenant query exposed $CROSS_SPAN_COUNT span(s)"
    echo "Result: $CROSS_TENANT_RESULT"
    exit 1
  fi
  echo " OK (cross-tenant denial)"

  echo "3. Simulating a service failure with $CHAOS_SERVICE..."
  CHAOS_CLEANUP_REQUIRED=1
  docker compose kill -s SIGKILL "$CHAOS_SERVICE"
  docker compose up -d "$CHAOS_SERVICE" >/dev/null
  wait_for_health

  echo "4. Confirming the pipeline recovers after restart..."
  TRACE_ID="$followup_trace_id"
  followup_payload="{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"$SERVICE_NAME\"}}]},\"scopeSpans\":[{\"spans\":[{\"traceId\":\"$TRACE_ID\",\"spanId\":\"1111111111111111\",\"name\":\"chaos-recovery\",\"startTimeUnixNano\":\"$(date +%s%N)\",\"endTimeUnixNano\":\"$(( $(date +%s%N) + 5000000 ))\",\"status\":{\"code\":1}}]}]}]}"
  send_trace_until_queryable "$followup_payload" 6

  echo ""
  echo "=== CHAOS SMOKE PASSED ==="
}

if [[ "$CHAOS_SMOKE_SOURCE_ONLY" != "1" ]]; then
  main "$@"
fi
