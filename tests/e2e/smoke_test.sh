#!/usr/bin/env bash
set -euo pipefail

INGEST="http://localhost:4317"
QUERY="http://localhost:8090"
TOKEN="dev-api-key-0000"
TENANT_ID="00000000-0000-0000-0000-000000000001"
TRACE_ID="4bf92f3577b34da6a3ce929d0e0e4736"
CURL_BIN="${CURL_BIN:-curl}"

if grep -qi microsoft /proc/version 2>/dev/null && command -v curl.exe >/dev/null 2>&1; then
  CURL_BIN="curl.exe"
fi

echo "=== Phase 1 Smoke Test ==="

echo "1. Sending trace..."
"$CURL_BIN" -sf -X POST "$INGEST/v1/traces" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"smoke-svc\"}}]},\"scopeSpans\":[{\"spans\":[{\"traceId\":\"$TRACE_ID\",\"spanId\":\"00f067aa0ba902b7\",\"name\":\"e2e-smoke\",\"startTimeUnixNano\":\"$(date +%s%N)\",\"endTimeUnixNano\":\"$(( $(date +%s%N) + 5000000 ))\",\"status\":{\"code\":1}}]}]}]}"
echo " OK"

echo "2. Waiting for pipeline..."
sleep 3

echo "3. Querying trace detail..."

RESULT=$("$CURL_BIN -sf -H "X-Tenant-ID: $TENANT_ID" "$QUERY/v1/traces/$TRACE_ID")
echo "$RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert len(d['spans'])>0, 'no spans'"
echo " OK (detail) — $(echo "$RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d['spans']),'spans')")"

echo "3b. Searching traces..."

SEARCH_RESULT=$("$CURL_BIN -sf -H "X-Tenant-ID: $TENANT_ID" "$QUERY/v1/traces?service=smoke-svc")
echo "$SEARCH_RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['total'] > 0, 'total traces is 0'"
echo " OK (search)"

echo "4. Sending log..."
"$CURL_BIN" -sf -X POST "$INGEST/v1/logs" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"resourceLogs\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"smoke-svc\"}}]},\"scopeLogs\":[{\"logRecords\":[{\"timeUnixNano\":\"$(date +%s%N)\",\"severityNumber\":9,\"body\":{\"stringValue\":\"smoke test log\"}}]}]}]}"
echo " OK"

echo "5. Sending metric..."
"$CURL_BIN" -sf -X POST "$INGEST/v1/metrics" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"resourceMetrics\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"smoke-svc\"}}]},\"scopeMetrics\":[{\"metrics\":[{\"name\":\"smoke.counter\",\"sum\":{\"dataPoints\":[{\"asDouble\":1.0,\"timeUnixNano\":\"$(date +%s%N)\"}],\"aggregationTemporality\":2,\"isMonotonic\":true}}]}]}]}"
echo " OK"

echo "6. Checking discovery endpoints..."

"$CURL_BIN -sf -H "X-Tenant-ID: $TENANT_ID" "$QUERY/v1/services" | python3 -c "import sys,json; d=json.load(sys.stdin); assert 'smoke-svc' in d['items'], 'smoke-svc not found'"
echo " OK (discovery)"

echo ""
echo "=== ALL CHECKS PASSED ==="
