#!/usr/bin/env bash
set -euo pipefail

INGEST="${INGEST_URL:-http://localhost:4317}"
QUERY="${QUERY_URL:-http://localhost:8090}"
TOKEN="dev-api-key-0000"
TENANT_ID="00000000-0000-0000-0000-000000000001"
TRACE_ID="4bf92f3577b34da6a3ce929d0e0e4736"

echo "=== Phase 1 Smoke Test ==="

echo "1. Sending trace..."
curl -sf -X POST "$INGEST/v1/traces" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"smoke-svc\"}}]},\"scopeSpans\":[{\"spans\":[{\"traceId\":\"$TRACE_ID\",\"spanId\":\"00f067aa0ba902b7\",\"name\":\"e2e-smoke\",\"startTimeUnixNano\":\"$(date +%s%N)\",\"endTimeUnixNano\":\"$(( $(date +%s%N) + 5000000 ))\",\"status\":{\"code\":1}}]}]}]}"
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
SEARCH_RESULT=$(curl -sf -H "X-Tenant-ID: $TENANT_ID" "$QUERY/v1/traces?service=smoke-svc")
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
  -d "{\"resourceLogs\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"smoke-svc\"}}]},\"scopeLogs\":[{\"logRecords\":[{\"timeUnixNano\":\"$(date +%s%N)\",\"severityNumber\":9,\"body\":{\"stringValue\":\"smoke test log\"}}]}]}]}"
echo " OK"

echo "5. Sending metric..."
curl -sf -X POST "$INGEST/v1/metrics" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"resourceMetrics\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"smoke-svc\"}}]},\"scopeMetrics\":[{\"metrics\":[{\"name\":\"smoke.counter\",\"sum\":{\"dataPoints\":[{\"asDouble\":1.0,\"timeUnixNano\":\"$(date +%s%N)\"}],\"aggregationTemporality\":2,\"isMonotonic\":true}}]}]}]}"
echo " OK"

echo "6. Checking discovery endpoints..."
SERVICES=$(curl -sf -H "X-Tenant-ID: $TENANT_ID" "$QUERY/v1/services")
if echo "$SERVICES" | jq -e '.items | contains(["smoke-svc"])' > /dev/null; then
  echo " OK (discovery)"
else
  echo " FAIL: smoke-svc not found in discovery items"
  echo " Result: $SERVICES"
  exit 1
fi

echo ""
echo "=== ALL CHECKS PASSED ==="
