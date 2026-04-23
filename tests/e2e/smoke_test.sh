#!/usr/bin/env bash
set -euo pipefail

INGEST="${INGEST_URL:-http://localhost:4318}"
GRPC_INGEST="${GRPC_INGEST_URL:-http://localhost:4317}"
QUERY="${QUERY_URL:-http://localhost:8090}"
TOKEN="dev-api-key-0000"
TENANT_ID="00000000-0000-0000-0000-000000000001"
RUN_ID="${RUN_ID:-$(date +%s%N)}"
SERVICE_NAME="smoke-svc-${RUN_ID}"
GRPC_SERVICE_NAME="smoke-grpc-svc-${RUN_ID}"
TRACE_ID="$(printf '%032x' "$((10#$RUN_ID % 4294967295))")"

echo "=== Phase 1 Smoke Test ==="

echo "1. Sending trace..."
curl -sf -X POST "$INGEST/v1/traces" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"$SERVICE_NAME\"}}]},\"scopeSpans\":[{\"spans\":[{\"traceId\":\"$TRACE_ID\",\"spanId\":\"00f067aa0ba902b7\",\"name\":\"e2e-smoke\",\"startTimeUnixNano\":\"$(date +%s%N)\",\"endTimeUnixNano\":\"$(( $(date +%s%N) + 5000000 ))\",\"status\":{\"code\":1}}]}]}]}"
echo " OK"

echo "2. Waiting for pipeline..."
sleep 3

echo "3. Querying trace detail..."
RESULT=$(curl -sf -H "X-Tenant-ID: $TENANT_ID" "$QUERY/v1/traces/$TRACE_ID")
SPAN_COUNT=$(echo "$RESULT" | jq '.spans | length')
if [ "$SPAN_COUNT" -gt 0 ]; then
  echo " OK (detail) — $SPAN_COUNT spans"
else
  echo " FAIL: no spans found in trace"
  exit 1
fi

echo "3b. Searching traces..."
SEARCH_RESULT=$(curl -sf -H "X-Tenant-ID: $TENANT_ID" "$QUERY/v1/traces?service=$SERVICE_NAME")
TOTAL=$(echo "$SEARCH_RESULT" | jq '.total')
if [ "$TOTAL" -gt 0 ]; then
  echo " OK (search) — $TOTAL traces found"
else
  echo " FAIL: total traces is 0"
  exit 1
fi

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
sleep 3
GRPC_LOG_RESULT=$(curl -sf -H "X-Tenant-ID: $TENANT_ID" "$QUERY/v1/logs?service=$GRPC_SERVICE_NAME")
GRPC_LOG_COUNT=$(echo "$GRPC_LOG_RESULT" | jq '.logs | length')
if [ "$GRPC_LOG_COUNT" -gt 0 ]; then
  echo " OK (verified) — $GRPC_LOG_COUNT log record(s) in ClickHouse"
else
  echo " FAIL: gRPC log not found in ClickHouse"
  echo " Result: $GRPC_LOG_RESULT"
  exit 1
fi

echo "6. Checking discovery endpoints..."
SERVICES=$(curl -sf -H "X-Tenant-ID: $TENANT_ID" "$QUERY/v1/services")
if echo "$SERVICES" | jq -e --arg service_name "$SERVICE_NAME" '.items | contains([$service_name])' > /dev/null; then
  echo " OK (discovery)"
else
  echo " FAIL: $SERVICE_NAME not found in discovery items"
  echo " Result: $SERVICES"
  exit 1
fi

echo ""
echo "=== ALL CHECKS PASSED ==="
