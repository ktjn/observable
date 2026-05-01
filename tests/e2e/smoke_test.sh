#!/usr/bin/env bash
set -euo pipefail

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "FAIL: required command '$name' is not available"
    exit 1
  fi
}

ensure_prereqs() {
  require_command curl
  require_command jq
  require_command grpcurl
}

INGEST="${INGEST_URL:-http://localhost:4318}"
GRPC_INGEST="${GRPC_INGEST_URL:-http://localhost:4317}"
QUERY="${QUERY_URL:-http://localhost:8090}"
TOKEN="dev-api-key-0000"
TENANT_ID="00000000-0000-0000-0000-000000000001"
RUN_ID="${RUN_ID:-$(date +%s%N)}"
SERVICE_NAME="smoke-svc-${RUN_ID}"
GRPC_SERVICE_NAME="smoke-grpc-svc-${RUN_ID}"
TRACE_ID="$(printf '%032x' "$((10#$RUN_ID % 4294967295))")"

wait_for_json_count() {
  local label="$1"
  local url="$2"
  local jq_expr="$3"
  local attempts="${4:-20}"
  local delay_seconds="${5:-1}"
  local result
  local count

  for _ in $(seq 1 "$attempts"); do
    result=$(curl -sf -H "X-Tenant-ID: $TENANT_ID" "$url" || true)
    count=$(echo "$result" | jq -r "$jq_expr" 2>/dev/null || echo 0)
    if [[ ! "$count" =~ ^[0-9]+$ ]]; then
      count=0
    fi
    if [ "$count" -gt 0 ]; then
      echo " OK ($label) - $count record(s)"
      return 0
    fi
    sleep "$delay_seconds"
  done

  echo " FAIL: $label did not return records after $attempts attempts"
  echo " Last result: ${result:-<empty>}"
  return 1
}

assert_http_status() {
  local label="$1"
  local expected_status="$2"
  shift 2

  local actual_status
  actual_status=$(curl -s -o /dev/null -w "%{http_code}" "$@")
  if [ "$actual_status" = "$expected_status" ]; then
    echo " OK ($label)"
  else
    echo " FAIL: $label returned HTTP $actual_status"
    exit 1
  fi
}

send_trace_until_queryable() {
  local trace_payload="$1"
  local max_attempts="${2:-2}"
  local attempt=1

  while [ "$attempt" -le "$max_attempts" ]; do
    echo "1. Sending trace..."
    curl -sf -X POST "$INGEST/v1/traces" \
      -H "Authorization: Bearer $TOKEN" \
      -H "Content-Type: application/json" \
      -d "$trace_payload"
    echo " OK"

    echo "2. Querying trace detail..."
    if wait_for_json_count "detail" "$QUERY/v1/traces/$TRACE_ID" '.spans | length'; then
      return 0
    fi

    if [ "$attempt" -lt "$max_attempts" ]; then
      echo " WARN: trace detail not visible yet, retrying ingest ($attempt/$max_attempts)"
    fi
    attempt=$((attempt + 1))
  done

  echo " FAIL: trace detail did not become visible after $max_attempts ingest attempt(s)"
  exit 1
}

main() {
  ensure_prereqs

  echo "=== Phase 1 Smoke Test ==="

  TRACE_PAYLOAD="{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"$SERVICE_NAME\"}}]},\"scopeSpans\":[{\"spans\":[{\"traceId\":\"$TRACE_ID\",\"spanId\":\"00f067aa0ba902b7\",\"name\":\"e2e-smoke\",\"startTimeUnixNano\":\"$(date +%s%N)\",\"endTimeUnixNano\":\"$(( $(date +%s%N) + 5000000 ))\",\"status\":{\"code\":1}}]}]}]}"
  send_trace_until_queryable "$TRACE_PAYLOAD" 2

  echo "1b. Checking missing auth rejection..."
  assert_http_status "missing auth rejected" "401" -X POST "$INGEST/v1/traces" \
    -H "Content-Type: application/json" \
    -d "{\"resourceSpans\":[]}"

  echo "1c. Checking viewer ingest rejection..."
  assert_http_status "viewer ingest rejected" "403" -X POST "$INGEST/v1/traces" \
    -H "Authorization: Bearer dev-viewer-key-0000" \
    -H "Content-Type: application/json" \
    -d "{\"resourceSpans\":[]}"

  echo "2b. Checking cross-tenant trace denial..."
  OTHER_TENANT_ID="00000000-0000-0000-0000-000000000002"
  CROSS_RESULT=$(curl -sf -H "X-Tenant-ID: $OTHER_TENANT_ID" "$QUERY/v1/traces/$TRACE_ID" || true)
  CROSS_SPAN_COUNT=$(echo "$CROSS_RESULT" | jq '.spans | length' 2>/dev/null || echo 0)
  if [[ ! "$CROSS_SPAN_COUNT" =~ ^[0-9]+$ ]]; then
    CROSS_SPAN_COUNT=0
  fi
  if [ "$CROSS_SPAN_COUNT" -eq 0 ]; then
    echo " OK (cross-tenant trace hidden)"
  else
    echo " FAIL: cross-tenant query exposed $CROSS_SPAN_COUNT span(s)"
    echo " Result: $CROSS_RESULT"
    exit 1
  fi

  echo "3. Searching traces..."
  wait_for_json_count "search" "$QUERY/v1/traces?service=$SERVICE_NAME" '.total'

  echo "4. Sending log..."
  curl -sf -X POST "$INGEST/v1/logs" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"resourceLogs\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"$SERVICE_NAME\"}}]},\"scopeLogs\":[{\"logRecords\":[{\"timeUnixNano\":\"$(date +%s%N)\",\"severityNumber\":9,\"body\":{\"stringValue\":\"smoke test log\"}}]}]}]}"
  echo " OK"

  echo "5. Sending metric..."
  curl -sf -X POST "$INGEST/v1/metrics" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"resourceMetrics\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"$SERVICE_NAME\"}}]},\"scopeMetrics\":[{\"metrics\":[{\"name\":\"smoke.counter\",\"sum\":{\"dataPoints\":[{\"asDouble\":1.0,\"timeUnixNano\":\"$(date +%s%N)\"}],\"aggregationTemporality\":2,\"isMonotonic\":true}}]}]}]}"
  echo " OK"

  echo "5a. Verifying metric series is queryable..."
  METRIC_SERIES_ID=""
  for _ in $(seq 1 20); do
    METRIC_SERIES_RESULT=$(curl -sf -H "X-Tenant-ID: $TENANT_ID" "$QUERY/v1/metrics?service=$SERVICE_NAME" || true)
    METRIC_SERIES_ID=$(echo "$METRIC_SERIES_RESULT" | jq -r '.series[] | select(.metric_name == "smoke.counter") | .metric_series_id' 2>/dev/null | head -n 1)
    if [ -n "$METRIC_SERIES_ID" ] && [ "$METRIC_SERIES_ID" != "null" ]; then
      echo " OK (metric series) - $METRIC_SERIES_ID"
      break
    fi
    sleep 1
  done
  if [ -z "$METRIC_SERIES_ID" ] || [ "$METRIC_SERIES_ID" = "null" ]; then
    echo " FAIL: smoke.counter series did not become queryable"
    echo " Last result: ${METRIC_SERIES_RESULT:-<empty>}"
    exit 1
  fi

  echo "5b. Verifying metric points are queryable..."
  wait_for_json_count "metric points" "$QUERY/v1/metrics/$METRIC_SERIES_ID" '.points | length'

  echo "5c. Sending log via gRPC..."
  GRPC_HOST=$(echo "$GRPC_INGEST" | sed 's|http://||')
  GRPC_BODY="smoke-grpc-log-$(date +%s%N)"
  grpcurl -plaintext \
    -import-path /proto/otlp \
    -proto opentelemetry/proto/collector/logs/v1/logs_service.proto \
    -H "Authorization: Bearer $TOKEN" \
    -d "{\"resourceLogs\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"$GRPC_SERVICE_NAME\"}}]},\"scopeLogs\":[{\"logRecords\":[{\"timeUnixNano\":\"$(date +%s%N)\",\"severityNumber\":9,\"body\":{\"stringValue\":\"$GRPC_BODY\"}}]}]}]}" \
    "$GRPC_HOST" \
    opentelemetry.proto.collector.logs.v1.LogsService/Export
  echo " OK (sent)"

  echo "5d. Verifying gRPC log landed in ClickHouse..."
  wait_for_json_count "grpc logs" "$QUERY/v1/logs?service=$GRPC_SERVICE_NAME" '.logs | length'

  echo "5e. Verifying log histogram endpoint returns buckets..."
  FROM_ISO=$(date -u -d "1 hour ago" +"%Y-%m-%dT%H:%M:%SZ" 2>/dev/null || date -u -v-1H +"%Y-%m-%dT%H:%M:%SZ")
  TO_ISO=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
  HIST_RESULT=$(curl -sf -H "X-Tenant-ID: $TENANT_ID" \
    "$QUERY/v1/logs/histogram?service=$SERVICE_NAME&from=${FROM_ISO}&to=${TO_ISO}&buckets=30" || true)
  BUCKET_COUNT=$(echo "$HIST_RESULT" | jq '.buckets | length' 2>/dev/null || echo 0)
  if [ "$BUCKET_COUNT" -eq 30 ]; then
    echo " OK (histogram) - 30 buckets"
  else
    echo " FAIL: histogram returned $BUCKET_COUNT buckets instead of 30"
    echo " Result: ${HIST_RESULT:-<empty>}"
    exit 1
  fi

  echo "6. Checking discovery endpoints..."
  wait_for_json_count \
    "discovery" \
    "$QUERY/v1/services" \
    "[.items[] | select(. == \"$SERVICE_NAME\")] | length"

  echo ""
  echo "=== ALL CHECKS PASSED ==="
}

if [[ "${SMOKE_TEST_SOURCE_ONLY:-0}" != "1" ]]; then
  main "$@"
fi
