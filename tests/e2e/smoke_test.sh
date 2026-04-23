#!/usr/bin/env bash
set -euo pipefail

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "FAIL: required command '$name' is not available"
    exit 1
  fi
}

require_command curl
require_command jq
require_command grpcurl

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

main() {
  echo "=== Phase 1 Smoke Test ==="

  echo "1. Sending trace..."
  curl -sf -X POST "$INGEST/v1/traces" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"$SERVICE_NAME\"}}]},\"scopeSpans\":[{\"spans\":[{\"traceId\":\"$TRACE_ID\",\"spanId\":\"00f067aa0ba902b7\",\"name\":\"e2e-smoke\",\"startTimeUnixNano\":\"$(date +%s%N)\",\"endTimeUnixNano\":\"$(( $(date +%s%N) + 5000000 ))\",\"status\":{\"code\":1}}]}]}]}"
  echo " OK"

  echo "1b. Checking missing auth rejection..."
  AUTH_STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$INGEST/v1/traces" \
    -H "Content-Type: application/json" \
    -d "{\"resourceSpans\":[]}")
  if [ "$AUTH_STATUS" = "401" ]; then
    echo " OK (missing auth rejected)"
  else
    echo " FAIL: missing auth returned HTTP $AUTH_STATUS"
    exit 1
  fi

  echo "2. Querying trace detail..."
  wait_for_json_count "detail" "$QUERY/v1/traces/$TRACE_ID" '.spans | length'

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

  echo "5b. Sending log via gRPC..."
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

  echo "5c. Verifying gRPC log landed in ClickHouse..."
  wait_for_json_count "grpc logs" "$QUERY/v1/logs?service=$GRPC_SERVICE_NAME" '.logs | length'

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
