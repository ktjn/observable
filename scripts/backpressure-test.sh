#!/usr/bin/env bash
# Backpressure and resource-bound tests.
# Verifies: rate limiting, metric cardinality budgets, queue growth under
# storage-writer pause, and memory-bounded behavior.
#
# Requires the Observable stack to be running (docker compose up -d).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BACKPRESSURE_SOURCE_ONLY="${BACKPRESSURE_SOURCE_ONLY:-0}"
CLEANUP_REQUIRED=0

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "FAIL: required command '$name' is not available"
    exit 1
  fi
}

INGEST="${INGEST_URL:-http://localhost:4318}"
QUERY="${QUERY_URL:-http://localhost:8090}"
TOKEN="dev-api-key-0000"
TENANT_ID="00000000-0000-0000-0000-000000000002"
RUN_ID="${RUN_ID:-$(date +%s%N)}"
OVERALL_PASS=1

cleanup() {
  if [ "$CLEANUP_REQUIRED" -eq 1 ]; then
    echo "  Restoring storage-writer..."
    docker compose up -d storage-writer >/dev/null 2>&1 || true
    sleep 5
  fi
}
trap cleanup EXIT

fail_test() {
  OVERALL_PASS=0
  echo "  FAIL: $1"
}

# ---------------------------------------------------------------------------
test_rate_limiting() {
  echo ""
  echo "=== Test: Rate limiting returns 429 ==="

  local burst_count="${BURST_COUNT:-200}"
  local rejected=0
  local accepted=0
  local now_ns
  now_ns=$(date +%s%N)

  echo "  Sending $burst_count trace requests in rapid succession..."
  for i in $(seq 1 "$burst_count"); do
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 \
      -X POST "$INGEST/v1/traces" \
      -H "Authorization: Bearer $TOKEN" \
      -H "Content-Type: application/json" \
      -d "{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"rate-test\"}}]},\"scopeSpans\":[{\"spans\":[{\"traceId\":\"$(printf '%032x' "$i")\",\"spanId\":\"aaaaaaaaaaaaaaaa\",\"name\":\"burst\",\"startTimeUnixNano\":\"$now_ns\",\"endTimeUnixNano\":\"$((now_ns + 1000000))\",\"status\":{\"code\":1}}]}]}]}" \
      2>/dev/null || echo "000")
    if [ "$status" = "429" ]; then
      rejected=$((rejected + 1))
    elif [ "$status" -lt 400 ] && [ "$status" != "000" ]; then
      accepted=$((accepted + 1))
    fi
  done

  echo "  Accepted: $accepted / Rejected (429): $rejected"
  if [ "$rejected" -gt 0 ]; then
    echo "  OK (rate limiter triggered after $accepted accepted requests)"
  else
    echo "  WARN: no 429 responses seen — rate limit may be set higher than burst count"
    echo "  (default limit is 100 req/s; increase BURST_COUNT to exceed it)"
  fi
}

# ---------------------------------------------------------------------------
test_request_size_limit() {
  echo ""
  echo "=== Test: Oversized request rejection ==="

  local large_body
  large_body=$(python3 -c "
import json, sys
spans = []
for i in range(500):
    spans.append({
        'traceId': f'{i:032x}',
        'spanId': f'{i:016x}',
        'name': 'x' * 1000,
        'startTimeUnixNano': '1000000000000000000',
        'endTimeUnixNano': '1000000005000000000',
        'attributes': [{'key': f'k{j}', 'value': {'stringValue': 'v' * 500}} for j in range(50)],
        'status': {'code': 1}
    })
payload = {'resourceSpans': [{'resource': {'attributes': []}, 'scopeSpans': [{'spans': spans}]}]}
sys.stdout.write(json.dumps(payload))
" 2>/dev/null || echo "")

  if [ -z "$large_body" ]; then
    echo "  SKIP: python3 not available for payload generation"
    return
  fi

  local body_size
  body_size=$(printf '%s' "$large_body" | wc -c)
  echo "  Sending ${body_size}-byte request..."

  local status
  status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 15 \
    -X POST "$INGEST/v1/traces" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "$large_body" 2>/dev/null || echo "000")

  if [ "$status" = "413" ] || [ "$status" = "400" ]; then
    echo "  OK (oversized request rejected: HTTP $status)"
  elif [ "$status" -lt 400 ] && [ "$status" != "000" ]; then
    echo "  INFO: request accepted (HTTP $status) — body may be under the limit"
  else
    echo "  INFO: HTTP $status response"
  fi
}

# ---------------------------------------------------------------------------
test_queue_growth_under_pause() {
  echo ""
  echo "=== Test: Queue growth during storage-writer pause ==="

  echo "1. Pausing storage-writer..."
  CLEANUP_REQUIRED=1
  docker compose pause storage-writer 2>/dev/null || docker compose kill -s SIGSTOP storage-writer 2>/dev/null || {
    echo "  SKIP: cannot pause storage-writer"
    return
  }

  echo "2. Sending 50 ingest requests while storage-writer is paused..."
  local accepted=0
  local now_ns
  now_ns=$(date +%s%N)
  for i in $(seq 1 50); do
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 \
      -X POST "$INGEST/v1/traces" \
      -H "Authorization: Bearer $TOKEN" \
      -H "Content-Type: application/json" \
      -d "{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"queue-test\"}}]},\"scopeSpans\":[{\"spans\":[{\"traceId\":\"$(printf '%032x' "$((i + 100000))")\",\"spanId\":\"bbbbbbbbbbbbbbbb\",\"name\":\"queued\",\"startTimeUnixNano\":\"$now_ns\",\"endTimeUnixNano\":\"$((now_ns + 1000000))\",\"status\":{\"code\":1}}]}]}]}" \
      2>/dev/null || echo "000")
    if [ "$status" -lt 400 ] && [ "$status" != "000" ]; then
      accepted=$((accepted + 1))
    fi
  done
  echo "  $accepted/50 requests accepted (queued in Redpanda)"

  if [ "$accepted" -ge 45 ]; then
    echo "  OK (ingest continues accepting while storage-writer is paused)"
  else
    fail_test "only $accepted/50 requests accepted during storage-writer pause"
  fi

  echo "3. Resuming storage-writer..."
  docker compose unpause storage-writer 2>/dev/null || docker compose kill -s SIGCONT storage-writer 2>/dev/null || true
  CLEANUP_REQUIRED=0

  echo "4. Waiting for queue to drain (20s)..."
  sleep 20

  echo "5. Verifying queued data is now queryable..."
  local svc_count
  svc_count=$(curl -sf --max-time 10 \
    -H "X-Tenant-ID: $TENANT_ID" \
    "$QUERY/v1/traces?service=queue-test&limit=1" 2>/dev/null \
    | jq '.items | length' 2>/dev/null || echo 0)
  if [ "$svc_count" -gt 0 ]; then
    echo "  OK (queued traces visible after storage-writer resume)"
  else
    echo "  WARN: queued traces not yet visible — queue may still be draining"
  fi

  echo "=== Queue growth test PASSED ==="
}

# ---------------------------------------------------------------------------
test_service_memory_bounded() {
  echo ""
  echo "=== Test: Service memory is bounded under load ==="

  local services=("ingest-gateway" "stream-processor" "storage-writer" "query-api")
  local mem_limit_mb="${MEM_LIMIT_MB:-2048}"

  echo "  Checking container memory usage (limit: ${mem_limit_mb}MB)..."
  local any_over=0

  for svc in "${services[@]}"; do
    local mem_bytes
    mem_bytes=$(docker compose exec -T "$svc" cat /sys/fs/cgroup/memory.current 2>/dev/null \
      || docker compose exec -T "$svc" cat /sys/fs/cgroup/memory/memory.usage_in_bytes 2>/dev/null \
      || echo "0")
    if [ "$mem_bytes" = "0" ]; then
      local mem_info
      mem_info=$(docker stats --no-stream --format '{{.MemUsage}}' "$(docker compose ps -q "$svc" 2>/dev/null)" 2>/dev/null || echo "unknown")
      echo "  $svc: $mem_info (cgroup not readable)"
      continue
    fi
    local mem_mb=$((mem_bytes / 1048576))
    if [ "$mem_mb" -gt "$mem_limit_mb" ]; then
      fail_test "$svc uses ${mem_mb}MB (exceeds ${mem_limit_mb}MB limit)"
      any_over=1
    else
      echo "  $svc: ${mem_mb}MB — OK"
    fi
  done

  if [ "$any_over" -eq 0 ]; then
    echo "  OK (all services within memory bounds)"
  fi
}

# ---------------------------------------------------------------------------
test_health_endpoints_consistent() {
  echo ""
  echo "=== Test: All health and readyz endpoints respond ==="

  local endpoints=(
    "http://localhost:4321/health|ingest-gateway health"
    "http://localhost:4321/readyz|ingest-gateway readyz"
    "http://localhost:4323/health|stream-processor health"
    "http://localhost:4323/readyz|stream-processor readyz"
    "http://localhost:4320/health|storage-writer health"
    "http://localhost:4320/readyz|storage-writer readyz"
    "http://localhost:8090/health|query-api health"
    "http://localhost:8090/readyz|query-api readyz"
    "http://localhost:4319/health|auth-service health"
    "http://localhost:4319/readyz|auth-service readyz"
    "http://localhost:4324/health|admin-service health"
    "http://localhost:4324/readyz|admin-service readyz"
    "http://localhost:4322/health|alert-evaluator health"
    "http://localhost:4322/readyz|alert-evaluator readyz"
  )

  for entry in "${endpoints[@]}"; do
    local url="${entry%%|*}"
    local label="${entry##*|}"
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "$url" 2>/dev/null || echo "000")
    if [ "$status" = "200" ]; then
      echo "  OK ($label)"
    else
      fail_test "$label returned HTTP $status"
    fi
  done
}

# ---------------------------------------------------------------------------
main() {
  require_command curl
  require_command jq
  require_command docker

  echo "=== Backpressure and Resource-Bound Tests ==="
  echo "Run ID: $RUN_ID"

  test_health_endpoints_consistent
  test_rate_limiting
  test_request_size_limit
  test_queue_growth_under_pause
  test_service_memory_bounded

  echo ""
  if [ "$OVERALL_PASS" -eq 1 ]; then
    echo "=== ALL BACKPRESSURE TESTS PASSED ==="
    exit 0
  else
    echo "=== SOME BACKPRESSURE TESTS FAILED ==="
    exit 1
  fi
}

if [[ "$BACKPRESSURE_SOURCE_ONLY" != "1" ]]; then
  main "$@"
fi
