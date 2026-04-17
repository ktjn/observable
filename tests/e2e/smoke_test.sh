#!/usr/bin/env bash
set -euo pipefail

INGEST="http://localhost:4317"
QUERY="http://localhost:8090"
TOKEN="dev-api-key-0000"
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

echo "3. Querying trace..."
RESULT=$(curl -sf "$QUERY/v1/traces/$TRACE_ID")
echo "$RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert len(d['spans'])>0, 'no spans'"
echo " OK — $(echo "$RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d['spans']),'spans')")"

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

echo ""
echo "=== ALL CHECKS PASSED ==="
