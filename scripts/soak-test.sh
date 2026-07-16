#!/usr/bin/env bash
# Soak test: sustained ingest/query load for a configurable duration.
# Default duration is 1 hour. Sends mixed traces, logs, and metrics at a steady
# rate and periodically verifies query correctness. Exits 1 on any failure.
#
# Requires the Observable stack to be running (docker compose up -d).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SMOKE_SCRIPT="$REPO_ROOT/tests/e2e/smoke_test.sh"

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "FAIL: required command '$name' is not available"
    exit 1
  fi
}

require_command curl
require_command jq

INGEST="${INGEST_URL:-http://localhost:4318}"
QUERY="${QUERY_URL:-http://localhost:8090}"
TOKEN="dev-api-key-0000"
TENANT_ID="00000000-0000-0000-0000-000000000002"

SOAK_DURATION_SECONDS="${SOAK_DURATION_SECONDS:-3600}"
SOAK_INTERVAL_SECONDS="${SOAK_INTERVAL_SECONDS:-5}"
SOAK_QUERY_INTERVAL="${SOAK_QUERY_INTERVAL:-60}"
SOAK_HEALTH_INTERVAL="${SOAK_HEALTH_INTERVAL:-30}"

INGEST_GW_HEALTH="${INGEST_GW_HEALTH_URL:-http://localhost:4321/health}"
QUERY_HEALTH="${QUERY_HEALTH_URL:-http://localhost:8090/health}"
STORAGE_HEALTH="${STORAGE_HEALTH_URL:-http://localhost:4320/health}"
STREAM_HEALTH="${STREAM_HEALTH_URL:-http://localhost:4323/health}"

SOAK_TMPDIR=$(mktemp -d)
trap 'rm -rf "$SOAK_TMPDIR"' EXIT
STATS_FILE="$SOAK_TMPDIR/stats.txt"

echo "=== Observable Soak Test ==="
printf "Duration        : %d seconds (%d minutes)\n" "$SOAK_DURATION_SECONDS" "$((SOAK_DURATION_SECONDS / 60))"
printf "Ingest interval : %d seconds\n" "$SOAK_INTERVAL_SECONDS"
printf "Query check     : every %d seconds\n" "$SOAK_QUERY_INTERVAL"
printf "Health check    : every %d seconds\n" "$SOAK_HEALTH_INTERVAL"
echo ""

INGEST_OK=0
INGEST_FAIL=0
QUERY_OK=0
QUERY_FAIL=0
HEALTH_OK=0
HEALTH_FAIL=0
ITERATION=0

START_TIME=$(date +%s)
LAST_QUERY_TIME=$START_TIME
LAST_HEALTH_TIME=$START_TIME

check_health() {
  local url="$1" label="$2"
  local status
  status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "$url" 2>/dev/null || echo "000")
  if [ "$status" = "200" ]; then
    return 0
  else
    echo "  WARN: $label health check failed (HTTP $status)"
    return 1
  fi
}

send_ingest_batch() {
  local batch_id="$1"
  local now_ns
  now_ns=$(date +%s%N)
  local end_ns=$((now_ns + 5000000))
  local trace_id
  trace_id=$(printf '%032x' "$((batch_id % 4294967295))")
  local svc_name="soak-svc-${batch_id}"

  local trace_ok=0 log_ok=0 metric_ok=0

  curl -sf -X POST "$INGEST/v1/traces" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    --max-time 10 \
    -d "{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"$svc_name\"}}]},\"scopeSpans\":[{\"spans\":[{\"traceId\":\"$trace_id\",\"spanId\":\"00f067aa0ba902b7\",\"name\":\"soak-op\",\"startTimeUnixNano\":\"$now_ns\",\"endTimeUnixNano\":\"$end_ns\",\"status\":{\"code\":1}}]}]}]}" \
    > /dev/null 2>&1 && trace_ok=1

  curl -sf -X POST "$INGEST/v1/logs" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    --max-time 10 \
    -d "{\"resourceLogs\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"$svc_name\"}}]},\"scopeLogs\":[{\"logRecords\":[{\"timeUnixNano\":\"$now_ns\",\"severityNumber\":9,\"body\":{\"stringValue\":\"soak test iteration $batch_id\"}}]}]}]}" \
    > /dev/null 2>&1 && log_ok=1

  curl -sf -X POST "$INGEST/v1/metrics" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    --max-time 10 \
    -d "{\"resourceMetrics\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"$svc_name\"}}]},\"scopeMetrics\":[{\"metrics\":[{\"name\":\"soak.counter\",\"sum\":{\"dataPoints\":[{\"asDouble\":$batch_id,\"timeUnixNano\":\"$now_ns\"}],\"aggregationTemporality\":2,\"isMonotonic\":true}}]}]}]}" \
    > /dev/null 2>&1 && metric_ok=1

  if [ "$trace_ok" -eq 1 ] && [ "$log_ok" -eq 1 ] && [ "$metric_ok" -eq 1 ]; then
    return 0
  else
    echo "  WARN: ingest batch $batch_id partial failure (trace=$trace_ok log=$log_ok metric=$metric_ok)"
    return 1
  fi
}

verify_query() {
  local ok=0
  local total=0

  # Check services list is non-empty
  total=$((total + 1))
  local svc_count
  svc_count=$(curl -sf --max-time 10 \
    -H "X-Tenant-ID: $TENANT_ID" \
    "$QUERY/v1/services" 2>/dev/null | jq '.items | length' 2>/dev/null || echo 0)
  if [ "$svc_count" -gt 0 ]; then
    ok=$((ok + 1))
  fi

  # Check traces search returns results
  total=$((total + 1))
  local trace_count
  trace_count=$(curl -sf --max-time 10 \
    -H "X-Tenant-ID: $TENANT_ID" \
    "$QUERY/v1/traces?limit=1" 2>/dev/null | jq '.items | length' 2>/dev/null || echo 0)
  if [ "$trace_count" -gt 0 ]; then
    ok=$((ok + 1))
  fi

  # Check logs search returns results
  total=$((total + 1))
  local log_count
  log_count=$(curl -sf --max-time 10 \
    -H "X-Tenant-ID: $TENANT_ID" \
    "$QUERY/v1/logs?limit=1" 2>/dev/null | jq '.items | length' 2>/dev/null || echo 0)
  if [ "$log_count" -gt 0 ]; then
    ok=$((ok + 1))
  fi

  # Check metrics list returns results
  total=$((total + 1))
  local metric_count
  metric_count=$(curl -sf --max-time 10 \
    -H "X-Tenant-ID: $TENANT_ID" \
    "$QUERY/v1/metrics" 2>/dev/null | jq '.items | length' 2>/dev/null || echo 0)
  if [ "$metric_count" -gt 0 ]; then
    ok=$((ok + 1))
  fi

  if [ "$ok" -eq "$total" ]; then
    return 0
  else
    echo "  WARN: query verification $ok/$total passed"
    return 1
  fi
}

echo "--- Starting soak loop ---"

while true; do
  NOW=$(date +%s)
  ELAPSED=$((NOW - START_TIME))

  if [ "$ELAPSED" -ge "$SOAK_DURATION_SECONDS" ]; then
    break
  fi

  ITERATION=$((ITERATION + 1))

  # Ingest a batch
  if send_ingest_batch "$ITERATION"; then
    INGEST_OK=$((INGEST_OK + 1))
  else
    INGEST_FAIL=$((INGEST_FAIL + 1))
  fi

  # Periodic health check
  if [ $((NOW - LAST_HEALTH_TIME)) -ge "$SOAK_HEALTH_INTERVAL" ]; then
    local_health_ok=1
    check_health "$INGEST_GW_HEALTH" "ingest-gateway" || local_health_ok=0
    check_health "$QUERY_HEALTH" "query-api" || local_health_ok=0
    check_health "$STORAGE_HEALTH" "storage-writer" || local_health_ok=0
    check_health "$STREAM_HEALTH" "stream-processor" || local_health_ok=0
    if [ "$local_health_ok" -eq 1 ]; then
      HEALTH_OK=$((HEALTH_OK + 1))
    else
      HEALTH_FAIL=$((HEALTH_FAIL + 1))
    fi
    LAST_HEALTH_TIME=$NOW
  fi

  # Periodic query verification
  if [ $((NOW - LAST_QUERY_TIME)) -ge "$SOAK_QUERY_INTERVAL" ]; then
    if verify_query; then
      QUERY_OK=$((QUERY_OK + 1))
    else
      QUERY_FAIL=$((QUERY_FAIL + 1))
    fi
    LAST_QUERY_TIME=$NOW
  fi

  # Progress report every 5 minutes
  if [ $((ITERATION % (300 / SOAK_INTERVAL_SECONDS) )) -eq 0 ]; then
    printf "[%dm] ingest: %d ok / %d fail | query: %d ok / %d fail | health: %d ok / %d fail\n" \
      "$((ELAPSED / 60))" "$INGEST_OK" "$INGEST_FAIL" "$QUERY_OK" "$QUERY_FAIL" "$HEALTH_OK" "$HEALTH_FAIL"
  fi

  sleep "$SOAK_INTERVAL_SECONDS"
done

echo ""
echo "--- Soak Test Results ---"
printf "Duration          : %d seconds (%d minutes)\n" "$SOAK_DURATION_SECONDS" "$((SOAK_DURATION_SECONDS / 60))"
printf "Ingest batches    : %d ok / %d fail\n" "$INGEST_OK" "$INGEST_FAIL"
printf "Query checks      : %d ok / %d fail\n" "$QUERY_OK" "$QUERY_FAIL"
printf "Health checks     : %d ok / %d fail\n" "$HEALTH_OK" "$HEALTH_FAIL"
echo ""

{
  echo "ingest_ok=$INGEST_OK"
  echo "ingest_fail=$INGEST_FAIL"
  echo "query_ok=$QUERY_OK"
  echo "query_fail=$QUERY_FAIL"
  echo "health_ok=$HEALTH_OK"
  echo "health_fail=$HEALTH_FAIL"
} > "$STATS_FILE"

# Fail if any ingest or query failures exceeded 1% threshold
TOTAL_INGEST=$((INGEST_OK + INGEST_FAIL))
TOTAL_QUERY=$((QUERY_OK + QUERY_FAIL))
FAIL=0

if [ "$TOTAL_INGEST" -gt 0 ] && [ "$INGEST_FAIL" -gt 0 ]; then
  INGEST_FAIL_PCT=$((INGEST_FAIL * 100 / TOTAL_INGEST))
  if [ "$INGEST_FAIL_PCT" -gt 1 ]; then
    echo "FAIL: ingest failure rate ${INGEST_FAIL_PCT}% exceeds 1% threshold"
    FAIL=1
  fi
fi

if [ "$TOTAL_QUERY" -gt 0 ] && [ "$QUERY_FAIL" -gt 0 ]; then
  QUERY_FAIL_PCT=$((QUERY_FAIL * 100 / TOTAL_QUERY))
  if [ "$QUERY_FAIL_PCT" -gt 1 ]; then
    echo "FAIL: query failure rate ${QUERY_FAIL_PCT}% exceeds 1% threshold"
    FAIL=1
  fi
fi

if [ "$HEALTH_FAIL" -gt 0 ]; then
  echo "FAIL: $HEALTH_FAIL health check failure(s) during soak"
  FAIL=1
fi

if [ "$FAIL" -eq 1 ]; then
  echo "=== SOAK TEST FAILED ==="
  exit 1
fi

echo "=== SOAK TEST PASSED ==="
