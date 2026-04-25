#!/usr/bin/env bash
# P2-S9a: Performance smoke baseline for ingest and query paths.
# Reports P50/P95 latency per endpoint and exits 1 if any path exceeds its threshold.
#
# Thresholds (overridable via env):
#   Ingest  P50 < 500ms,  P95 < 1000ms
#   Query   P50 < 1000ms, P95 < 3000ms  (spec/11-testing.md §18.3)
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

INGEST="${INGEST_URL:-http://localhost:4318}"
QUERY="${QUERY_URL:-http://localhost:8090}"
TOKEN="dev-api-key-0000"
TENANT_ID="00000000-0000-0000-0000-000000000001"
SAMPLE_COUNT="${SAMPLE_COUNT:-20}"
P50_INGEST_MAX_MS="${P50_INGEST_MAX_MS:-500}"
P95_INGEST_MAX_MS="${P95_INGEST_MAX_MS:-1000}"
P50_QUERY_MAX_MS="${P50_QUERY_MAX_MS:-1000}"
P95_QUERY_MAX_MS="${P95_QUERY_MAX_MS:-3000}"

TRACE_ID="4bf92f3577b34da6a3ce929d0e0e4736"
PERF_TMPDIR=$(mktemp -d)
trap 'rm -rf "$PERF_TMPDIR"' EXIT
RESULTS_FILE="$PERF_TMPDIR/results.txt"
touch "$RESULTS_FILE"

echo "=== P2-S9a Performance Smoke Baseline ==="
printf "Samples per path : %d\n" "$SAMPLE_COUNT"
printf "Thresholds       : ingest P50 <%dms P95 <%dms | query P50 <%dms P95 <%dms\n" \
  "$P50_INGEST_MAX_MS" "$P95_INGEST_MAX_MS" "$P50_QUERY_MAX_MS" "$P95_QUERY_MAX_MS"
echo ""

# Compute Nth percentile from a file of integers (one per line).
# Usage: percentile <pct 0-100> <file>
percentile() {
  local pct="$1" file="$2"
  sort -n "$file" | awk -v pct="$pct" '
    { a[NR] = $1 }
    END {
      if (NR == 0) { print 0; exit }
      idx = int((NR - 1) * pct / 100) + 1
      print a[idx]
    }
  '
}

# Send one request and append response time in ms to a file.
measure_one() {
  local method="$1" url="$2" out_file="$3"
  shift 3
  local t
  t=$(curl -o /dev/null -s -w "%{time_total}" -X "$method" "$url" "$@" 2>/dev/null)
  LC_ALL=C awk -v t="$t" 'BEGIN { printf "%d\n", t * 1000 }' >> "$out_file"
}

# Run SAMPLE_COUNT requests, report P50/P95, and record to RESULTS_FILE.
# Usage: run_samples <label> <method> <url> <kind:ingest|query> [curl-args...]
run_samples() {
  local label="$1" method="$2" url="$3" kind="$4"
  shift 4
  local sample_file
  sample_file="$PERF_TMPDIR/$(printf '%s' "$label" | tr ' /{}' '____').txt"
  local i=0
  while [ "$i" -lt "$SAMPLE_COUNT" ]; do
    measure_one "$method" "$url" "$sample_file" "$@"
    i=$((i + 1))
  done
  local p50 p95
  p50=$(percentile 50 "$sample_file")
  p95=$(percentile 95 "$sample_file")
  printf "  %-45s  P50: %5dms  P95: %5dms\n" "$label" "$p50" "$p95"
  printf "%s\t%d\t%d\t%s\n" "$kind" "$p50" "$p95" "$label" >> "$RESULTS_FILE"
}

# ---- seed one trace/log/metric so query paths return real data ----
echo "--- Seeding test data ---"
NOW_NS=$(date +%s%N)
END_NS=$((NOW_NS + 5000000))

curl -sf -X POST "$INGEST/v1/traces" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"perf-svc\"}}]},\"scopeSpans\":[{\"spans\":[{\"traceId\":\"$TRACE_ID\",\"spanId\":\"00f067aa0ba902b7\",\"name\":\"perf-smoke\",\"startTimeUnixNano\":\"$NOW_NS\",\"endTimeUnixNano\":\"$END_NS\",\"status\":{\"code\":1}}]}]}]}" \
  > /dev/null
echo "  trace seeded"

curl -sf -X POST "$INGEST/v1/logs" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"resourceLogs\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"perf-svc\"}}]},\"scopeLogs\":[{\"logRecords\":[{\"timeUnixNano\":\"$NOW_NS\",\"severityNumber\":9,\"body\":{\"stringValue\":\"perf smoke\"}}]}]}]}" \
  > /dev/null
echo "  log seeded"

curl -sf -X POST "$INGEST/v1/metrics" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"resourceMetrics\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"perf-svc\"}}]},\"scopeMetrics\":[{\"metrics\":[{\"name\":\"perf.counter\",\"sum\":{\"dataPoints\":[{\"asDouble\":1.0,\"timeUnixNano\":\"$NOW_NS\"}],\"aggregationTemporality\":2,\"isMonotonic\":true}}]}]}]}" \
  > /dev/null
echo "  metric seeded"

echo "  waiting for pipeline to settle (3s)..."
sleep 3

METRIC_SERIES_ID=$(curl -sf -H "X-Tenant-ID: $TENANT_ID" "$QUERY/v1/metrics" \
  | jq -r '.items[0].id // empty' 2>/dev/null || true)

# ---- ingest paths ----
echo ""
echo "--- Ingest latency ($SAMPLE_COUNT samples each) ---"

TRACE_PAYLOAD="{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"perf-svc\"}}]},\"scopeSpans\":[{\"spans\":[{\"traceId\":\"ffffffffffffffffffffffffffffffff\",\"spanId\":\"ffffffffffffffff\",\"name\":\"perf-load\",\"startTimeUnixNano\":\"$NOW_NS\",\"endTimeUnixNano\":\"$END_NS\",\"status\":{\"code\":1}}]}]}]}"
LOG_PAYLOAD="{\"resourceLogs\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"perf-svc\"}}]},\"scopeLogs\":[{\"logRecords\":[{\"timeUnixNano\":\"$NOW_NS\",\"severityNumber\":9,\"body\":{\"stringValue\":\"perf\"}}]}]}]}"
METRIC_PAYLOAD="{\"resourceMetrics\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"perf-svc\"}}]},\"scopeMetrics\":[{\"metrics\":[{\"name\":\"perf.counter\",\"sum\":{\"dataPoints\":[{\"asDouble\":1.0,\"timeUnixNano\":\"$NOW_NS\"}],\"aggregationTemporality\":2,\"isMonotonic\":true}}]}]}]}"

run_samples "POST /v1/traces" POST "$INGEST/v1/traces" ingest \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "$TRACE_PAYLOAD"

run_samples "POST /v1/logs" POST "$INGEST/v1/logs" ingest \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "$LOG_PAYLOAD"

run_samples "POST /v1/metrics" POST "$INGEST/v1/metrics" ingest \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "$METRIC_PAYLOAD"

# ---- query paths ----
echo ""
echo "--- Query latency ($SAMPLE_COUNT samples each) ---"

run_samples "GET /v1/traces/{id}" GET "$QUERY/v1/traces/$TRACE_ID" query \
  -H "X-Tenant-ID: $TENANT_ID"

run_samples "GET /v1/traces (search)" GET "$QUERY/v1/traces?service=perf-svc" query \
  -H "X-Tenant-ID: $TENANT_ID"

run_samples "GET /v1/logs" GET "$QUERY/v1/logs?service=perf-svc" query \
  -H "X-Tenant-ID: $TENANT_ID"

run_samples "GET /v1/metrics (list)" GET "$QUERY/v1/metrics" query \
  -H "X-Tenant-ID: $TENANT_ID"

if [ -n "$METRIC_SERIES_ID" ]; then
  run_samples "GET /v1/metrics/{id} (points)" GET "$QUERY/v1/metrics/$METRIC_SERIES_ID" query \
    -H "X-Tenant-ID: $TENANT_ID"
else
  printf "  %-45s  [skipped: no metric series available]\n" "GET /v1/metrics/{id} (points)"
fi

run_samples "GET /v1/services" GET "$QUERY/v1/services" query \
  -H "X-Tenant-ID: $TENANT_ID"

run_samples "GET /v1/environments" GET "$QUERY/v1/environments" query \
  -H "X-Tenant-ID: $TENANT_ID"

# ---- threshold evaluation ----
echo ""
echo "--- Threshold evaluation ---"
OVERALL_PASS=1
while IFS=$'\t' read -r kind p50 p95 label; do
  if [ "$kind" = "ingest" ]; then
    p50_max=$P50_INGEST_MAX_MS
    p95_max=$P95_INGEST_MAX_MS
  else
    p50_max=$P50_QUERY_MAX_MS
    p95_max=$P95_QUERY_MAX_MS
  fi
  status="PASS"
  if [ "$p50" -gt "$p50_max" ] || [ "$p95" -gt "$p95_max" ]; then
    status="FAIL"
    OVERALL_PASS=0
  fi
  printf "  %-45s  P50:%5dms/<%dms  P95:%5dms/<%dms  %s\n" \
    "$label" "$p50" "$p50_max" "$p95" "$p95_max" "$status"
done < "$RESULTS_FILE"

echo ""
if [ "$OVERALL_PASS" -eq 1 ]; then
  echo "=== ALL BASELINES WITHIN THRESHOLDS ==="
  exit 0
else
  echo "=== ONE OR MORE BASELINES EXCEEDED THRESHOLDS ==="
  exit 1
fi
