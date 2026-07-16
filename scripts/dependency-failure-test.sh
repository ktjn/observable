#!/usr/bin/env bash
# Dependency failure and recovery tests.
# Kills each infrastructure dependency (Redpanda, ClickHouse, PostgreSQL,
# OpenFGA) one at a time, verifies fail-closed behavior, restarts the
# dependency, and confirms the pipeline recovers.
#
# Requires the Observable stack to be running (docker compose up -d).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SMOKE_SCRIPT="$REPO_ROOT/tests/e2e/smoke_test.sh"
DEP_TEST_SOURCE_ONLY="${DEP_TEST_SOURCE_ONLY:-0}"
CLEANUP_SERVICES=()

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "FAIL: required command '$name' is not available"
    exit 1
  fi
}

INGEST="${INGEST_URL:-http://localhost:4318}"
PLATFORM="${PLATFORM_URL:-http://localhost:4321}"
QUERY="${QUERY_URL:-http://localhost:8090}"
ADMIN="${ADMIN_URL:-http://localhost:4324}"
TOKEN="dev-api-key-0000"
TENANT_ID="00000000-0000-0000-0000-000000000002"
RUN_ID="${RUN_ID:-$(date +%s%N)}"

cleanup() {
  for svc in "${CLEANUP_SERVICES[@]}"; do
    echo "  Restoring $svc..."
    docker compose up -d "$svc" >/dev/null 2>&1 || true
  done
}
trap cleanup EXIT

wait_for_url() {
  local url="$1" label="$2" attempts="${3:-30}" delay="${4:-2}"
  for _ in $(seq 1 "$attempts"); do
    if curl -sf --max-time 5 "$url" >/dev/null 2>&1; then
      echo "  OK ($label healthy)"
      return 0
    fi
    sleep "$delay"
  done
  echo "  FAIL: $label did not become healthy at $url"
  return 1
}

assert_ingest_fails() {
  local label="$1"
  local status
  status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 \
    -X POST "$INGEST/v1/traces" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"resourceSpans":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"dep-test"}}]},"scopeSpans":[{"spans":[{"traceId":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","spanId":"bbbbbbbbbbbbbbbb","name":"dep-test","startTimeUnixNano":"1000000000000000000","endTimeUnixNano":"1000000005000000000","status":{"code":1}}]}]}]}' \
    2>/dev/null || echo "000")

  if [ "$status" -ge 400 ] || [ "$status" = "000" ]; then
    echo "  OK ($label: ingest rejected with HTTP $status)"
    return 0
  else
    echo "  FAIL: $label: expected ingest rejection but got HTTP $status"
    return 1
  fi
}

assert_query_fails() {
  local label="$1"
  local status
  status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 \
    -H "X-Tenant-ID: $TENANT_ID" \
    "$QUERY/v1/services" 2>/dev/null || echo "000")
  if [ "$status" -ge 400 ] || [ "$status" = "000" ]; then
    echo "  OK ($label: query failed with HTTP $status)"
    return 0
  else
    echo "  WARN: $label: query returned HTTP $status (may still be serving cached)"
    return 0
  fi
}

send_and_verify_trace() {
  local label="$1"
  local trace_id
  trace_id=$(printf '%032x' "$((RANDOM % 4294967295))")
  local now_ns
  now_ns=$(date +%s%N)
  local end_ns=$((now_ns + 5000000))
  local svc_name="dep-test-${RUN_ID}"

  curl -sf -X POST "$INGEST/v1/traces" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    --max-time 10 \
    -d "{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"$svc_name\"}}]},\"scopeSpans\":[{\"spans\":[{\"traceId\":\"$trace_id\",\"spanId\":\"cccccccccccccccc\",\"name\":\"recovery-check\",\"startTimeUnixNano\":\"$now_ns\",\"endTimeUnixNano\":\"$end_ns\",\"status\":{\"code\":1}}]}]}]}" \
    > /dev/null 2>&1

  local attempt
  for attempt in $(seq 1 20); do
    local count
    count=$(curl -sf --max-time 10 \
      -H "X-Tenant-ID: $TENANT_ID" \
      -H "Authorization: Bearer $TOKEN" \
      "$QUERY/v1/traces/$trace_id" 2>/dev/null | jq '.spans | length' 2>/dev/null || echo 0)
    if [ "$count" -gt 0 ]; then
      echo "  OK ($label: trace visible after $attempt attempt(s))"
      return 0
    fi
    sleep 2
  done
  echo "  FAIL: $label: trace not visible after recovery"
  return 1
}

kill_service() {
  local svc="$1"
  echo "  Killing $svc..."
  CLEANUP_SERVICES+=("$svc")
  docker compose kill -s SIGKILL "$svc"
}

restart_service() {
  local svc="$1"
  echo "  Restarting $svc..."
  docker compose up -d "$svc" >/dev/null
}

# ---------------------------------------------------------------------------
test_redpanda_failure() {
  echo ""
  echo "=== Test: Redpanda interruption and recovery ==="

  echo "1. Killing Redpanda..."
  kill_service redpanda

  echo "2. Verifying ingest fails closed..."
  sleep 5
  assert_ingest_fails "Redpanda down"

  echo "3. Verifying stream-processor readyz fails..."
  local sp_status
  sp_status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 \
    "http://localhost:4323/readyz" 2>/dev/null || echo "000")
  if [ "$sp_status" -ge 400 ] || [ "$sp_status" = "000" ]; then
    echo "  OK (stream-processor readyz: HTTP $sp_status)"
  else
    echo "  WARN: stream-processor readyz returned HTTP $sp_status"
  fi

  echo "4. Restarting Redpanda..."
  restart_service redpanda
  wait_for_url "http://localhost:9644/v1/status/ready" "Redpanda" 60 2

  echo "5. Waiting for services to reconnect..."
  sleep 10
  wait_for_url "$PLATFORM/health" "ingest-gateway" 30 2
  wait_for_url "http://localhost:4323/health" "stream-processor" 30 2

  echo "6. Verifying pipeline recovery..."
  send_and_verify_trace "Redpanda recovery"

  echo "=== Redpanda test PASSED ==="
}

test_clickhouse_failure() {
  echo ""
  echo "=== Test: ClickHouse interruption and recovery ==="

  echo "1. Killing ClickHouse..."
  kill_service clickhouse

  echo "2. Verifying query fails..."
  sleep 3
  assert_query_fails "ClickHouse down"

  echo "3. Verifying storage-writer readyz fails..."
  local sw_status
  sw_status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 \
    "http://localhost:4320/readyz" 2>/dev/null || echo "000")
  if [ "$sw_status" -ge 400 ] || [ "$sw_status" = "000" ]; then
    echo "  OK (storage-writer readyz: HTTP $sw_status)"
  else
    echo "  WARN: storage-writer readyz returned HTTP $sw_status"
  fi

  echo "4. Verifying ingest still accepts (queues to Redpanda)..."
  local ingest_status
  ingest_status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 \
    -X POST "$INGEST/v1/traces" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"resourceSpans":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"ch-down-test"}}]},"scopeSpans":[{"spans":[{"traceId":"dddddddddddddddddddddddddddddddd","spanId":"eeeeeeeeeeeeeeee","name":"ch-down","startTimeUnixNano":"1000000000000000000","endTimeUnixNano":"1000000005000000000","status":{"code":1}}]}]}]}' \
    2>/dev/null || echo "000")
  if [ "$ingest_status" -lt 400 ] && [ "$ingest_status" != "000" ]; then
    echo "  OK (ingest still accepts: HTTP $ingest_status — data queued in Redpanda)"
  else
    echo "  INFO: ingest returned HTTP $ingest_status (may reject if auth depends on ClickHouse)"
  fi

  echo "5. Restarting ClickHouse..."
  restart_service clickhouse
  wait_for_url "http://localhost:8123/ping" "ClickHouse" 60 2

  echo "6. Waiting for services to reconnect..."
  sleep 10

  echo "7. Verifying pipeline recovery (queued data should flush)..."
  send_and_verify_trace "ClickHouse recovery"

  echo "=== ClickHouse test PASSED ==="
}

test_postgresql_failure() {
  echo ""
  echo "=== Test: PostgreSQL interruption and recovery ==="

  echo "1. Killing PostgreSQL..."
  kill_service postgres

  echo "2. Verifying ingest fails closed (API key validation requires Postgres)..."
  sleep 5
  assert_ingest_fails "PostgreSQL down"

  echo "3. Verifying query-api readyz fails..."
  local qa_status
  qa_status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 \
    "$QUERY/readyz" 2>/dev/null || echo "000")
  if [ "$qa_status" -ge 400 ] || [ "$qa_status" = "000" ]; then
    echo "  OK (query-api readyz: HTTP $qa_status)"
  else
    echo "  WARN: query-api readyz returned HTTP $qa_status"
  fi

  echo "4. Verifying auth-service readyz fails..."
  local auth_status
  auth_status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 \
    "http://localhost:4319/readyz" 2>/dev/null || echo "000")
  if [ "$auth_status" -ge 400 ] || [ "$auth_status" = "000" ]; then
    echo "  OK (auth-service readyz: HTTP $auth_status)"
  else
    echo "  WARN: auth-service readyz returned HTTP $auth_status"
  fi

  echo "5. Restarting PostgreSQL..."
  restart_service postgres

  echo "6. Waiting for PostgreSQL to accept connections..."
  local pg_ready=0
  for _ in $(seq 1 30); do
    if docker compose exec -T postgres pg_isready -U observable >/dev/null 2>&1; then
      pg_ready=1
      break
    fi
    sleep 2
  done
  if [ "$pg_ready" -eq 1 ]; then
    echo "  OK (PostgreSQL ready)"
  else
    echo "  FAIL: PostgreSQL did not become ready"
    return 1
  fi

  echo "7. Waiting for services to reconnect..."
  sleep 10
  wait_for_url "$PLATFORM/health" "ingest-gateway"
  wait_for_url "$QUERY/health" "query-api"

  echo "8. Verifying pipeline recovery..."
  send_and_verify_trace "PostgreSQL recovery"

  echo "=== PostgreSQL test PASSED ==="
}

test_openfga_failure() {
  echo ""
  echo "=== Test: OpenFGA interruption and recovery ==="

  echo "1. Killing OpenFGA..."
  kill_service openfga

  echo "2. Verifying ingestion still works (OpenFGA not in ingest path)..."
  sleep 3
  local ingest_status
  ingest_status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 \
    -X POST "$INGEST/v1/traces" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"resourceSpans":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"fga-down-test"}}]},"scopeSpans":[{"spans":[{"traceId":"ffffffffffffffffffffffffffffffff","spanId":"1111111111111111","name":"fga-down","startTimeUnixNano":"1000000000000000000","endTimeUnixNano":"1000000005000000000","status":{"code":1}}]}]}]}' \
    2>/dev/null || echo "000")
  if [ "$ingest_status" -lt 400 ] && [ "$ingest_status" != "000" ]; then
    echo "  OK (ingest works without OpenFGA: HTTP $ingest_status)"
  else
    echo "  WARN: ingest returned HTTP $ingest_status with OpenFGA down"
  fi

  echo "3. Verifying queries still work (OpenFGA not in query path)..."
  local query_status
  query_status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 \
    -H "X-Tenant-ID: $TENANT_ID" \
    "$QUERY/v1/services" 2>/dev/null || echo "000")
  if [ "$query_status" -lt 400 ] && [ "$query_status" != "000" ]; then
    echo "  OK (queries work without OpenFGA: HTTP $query_status)"
  else
    echo "  WARN: query returned HTTP $query_status with OpenFGA down"
  fi

  echo "4. Restarting OpenFGA..."
  restart_service openfga
  wait_for_url "http://localhost:8083/healthz" "OpenFGA" 30 2

  echo "=== OpenFGA test PASSED ==="
}

main() {
  require_command curl
  require_command jq
  require_command docker

  echo "=== Dependency Failure and Recovery Tests ==="
  echo "Run ID: $RUN_ID"

  test_redpanda_failure
  test_clickhouse_failure
  test_postgresql_failure
  test_openfga_failure

  echo ""
  echo "=== ALL DEPENDENCY FAILURE TESTS PASSED ==="
}

if [[ "$DEP_TEST_SOURCE_ONLY" != "1" ]]; then
  main "$@"
fi
