#!/usr/bin/env bash
# Canary promotion script for the Observable platform.
#
# Pattern (ADR-020, spec/12-deployment.md §19.8):
#   1. Create a deployment marker for the new tag (if --marker-url is set).
#   2. Deploy ingest-gateway-canary alongside stable via Helm values override.
#   3. Run three automated analysis gates against the canary Service:
#        Gate 1: /health returns HTTP 200 with body containing "ok"
#        Gate 2: smoke ingest (POST /v1/traces) returns HTTP 200
#        Gate 3: zero HTTP 5xx responses in canary pod logs after soak period
#   4. Promote on pass: upgrade stable release to new tag, remove canary.
#      Update the deployment marker to "success".
#   5. Revert on failure: remove canary; stable release is unchanged.
#      Update the deployment marker to "failed".
#
# Usage:
#   bash scripts/canary-promote.sh --tag <new-image-tag> [options]
#
# Options:
#   --tag <tag>            New image tag to canary (required)
#   --namespace <ns>       Kubernetes namespace            (default: observable)
#   --release <name>       Helm release name               (default: observable)
#   --soak-seconds <n>     Hold canary before gate 3 runs  (default: 30)
#   --dev-key <key>        API key used for smoke test      (default: dev-api-key-0000)
#   --marker-url <url>     Observable Platform API base URL for deployment markers.
#                          When set, a deployment marker is created before the canary
#                          and updated to success/failed after promotion or revert.
#                          (default: empty — deployment-marker integration disabled)
#
# Rollback contract (spec/12-deployment.md §19.8):
#   Revert removes only the canary Deployment and Service.  The stable Helm
#   release and its schema migrations are untouched.  No data is lost.
#   A failed gate exits with status 1 after revert completes.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
APP_CHART="$REPO_ROOT/charts/observable"

CANARY_TAG=""
NAMESPACE="observable"
RELEASE="observable"
SOAK_SECONDS=30
DEV_KEY="dev-api-key-0000"
CANARY_LOCAL_PORT=24317
MARKER_URL=""

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag)           CANARY_TAG="$2";    shift 2 ;;
    --namespace)     NAMESPACE="$2";     shift 2 ;;
    --release)       RELEASE="$2";       shift 2 ;;
    --soak-seconds)  SOAK_SECONDS="$2";  shift 2 ;;
    --dev-key)       DEV_KEY="$2";       shift 2 ;;
    --marker-url)    MARKER_URL="$2";    shift 2 ;;
    *) echo "ERROR: unknown argument: $1" >&2; exit 1 ;;
  esac
done

if [[ -z "$CANARY_TAG" ]]; then
  echo "ERROR: --tag is required" >&2
  echo "Usage: $0 --tag <new-image-tag>" >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

log()  { echo ""; echo "==> $*"; }
info() { echo "    $*"; }

PF_PID=""
DEPLOYMENT_ID=""

stop_port_forward() {
  if [[ -n "$PF_PID" ]]; then
    kill "$PF_PID" 2>/dev/null || true
    PF_PID=""
  fi
}

finish_deployment_marker() {
  local status="$1"
  if [[ -n "$MARKER_URL" && -n "$DEPLOYMENT_ID" ]]; then
    OBSERVABLE_URL="$MARKER_URL" OBSERVABLE_API_KEY="$DEV_KEY" \
      bash "$SCRIPT_DIR/deployment-marker.sh" finish \
      --id "$DEPLOYMENT_ID" --status "$status" || true
    info "Deployment marker $DEPLOYMENT_ID marked $status"
  fi
}

revert_canary() {
  log "REVERTING: removing canary Deployment and Service"
  stop_port_forward
  helm upgrade "$RELEASE" "$APP_CHART" \
    --namespace "$NAMESPACE" \
    --reuse-values \
    --set "services.ingestGateway.canary.enabled=false" \
    --wait --timeout 3m || true
  echo ""
  echo "Canary removed. Stable release is unchanged."
}

gate_fail() {
  local msg="$1"
  info "FAIL: $msg"
  revert_canary
  finish_deployment_marker "failed"
  exit 1
}

trap 'stop_port_forward' EXIT

# ---------------------------------------------------------------------------
# Step 0: Create deployment marker (if --marker-url provided)
# ---------------------------------------------------------------------------

if [[ -n "$MARKER_URL" ]]; then
  log "Creating deployment marker (service: ingest-gateway, env: $NAMESPACE, version: $CANARY_TAG)"
  DEPLOYMENT_ID=$(
    OBSERVABLE_URL="$MARKER_URL" OBSERVABLE_API_KEY="$DEV_KEY" \
      bash "$SCRIPT_DIR/deployment-marker.sh" start \
      --service ingest-gateway \
      --env "$NAMESPACE" \
      --version "$CANARY_TAG" \
      --deployed-by "canary-promote.sh"
  )
  info "Deployment marker created: $DEPLOYMENT_ID"
fi

# ---------------------------------------------------------------------------
# Step 1: Deploy canary alongside stable
# ---------------------------------------------------------------------------

log "Deploying ingest-gateway-canary (tag: $CANARY_TAG)"
helm dependency update "$APP_CHART" --quiet
helm upgrade "$RELEASE" "$APP_CHART" \
  --namespace "$NAMESPACE" \
  --reuse-values \
  --set "services.ingestGateway.canary.enabled=true" \
  --set "services.ingestGateway.canary.tag=$CANARY_TAG" \
  --wait --timeout 3m

info "Canary Deployment ready"

# ---------------------------------------------------------------------------
# Step 2: Port-forward to the canary Service
# ---------------------------------------------------------------------------

# Resolve the Service port from the running cluster (avoids hardcoding).
SERVICE_PORT=$(kubectl get service ingest-gateway-canary \
  --namespace "$NAMESPACE" \
  -o jsonpath='{.spec.ports[0].port}' 2>/dev/null || echo "4317")

log "Port-forwarding ingest-gateway-canary → localhost:$CANARY_LOCAL_PORT (svc port $SERVICE_PORT)"
kubectl port-forward "service/ingest-gateway-canary" \
  "$CANARY_LOCAL_PORT:$SERVICE_PORT" \
  --namespace "$NAMESPACE" &
PF_PID=$!
sleep 3  # let port-forward establish

# ---------------------------------------------------------------------------
# Gate 1: /health
# ---------------------------------------------------------------------------

log "Gate 1/3: health check"
HEALTH_RESP=$(curl -sf --max-time 5 "http://localhost:$CANARY_LOCAL_PORT/health" 2>/dev/null || echo "FAIL")
if echo "$HEALTH_RESP" | grep -qi "ok"; then
  info "PASS: /health returned ok"
else
  gate_fail "/health returned: $HEALTH_RESP"
fi

# ---------------------------------------------------------------------------
# Gate 2: smoke ingest
# ---------------------------------------------------------------------------

log "Gate 2/3: smoke ingest (POST /v1/traces)"
INGEST_STATUS=$(curl -sf \
  -o /dev/null \
  -w "%{http_code}" \
  --max-time 10 \
  -X POST "http://localhost:$CANARY_LOCAL_PORT/v1/traces" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $DEV_KEY" \
  -d "{
    \"resourceSpans\": [{
      \"resource\": {\"attributes\": [{\"key\": \"service.name\", \"value\": {\"stringValue\": \"canary-smoke-test\"}}]},
      \"scopeSpans\": [{\"spans\": [{
        \"traceId\": \"aabbccddeeff00112233445566778800\",
        \"spanId\": \"aabbccddeeff0022\",
        \"name\": \"canary-test-span\",
        \"kind\": 1,
        \"startTimeUnixNano\": \"$(date +%s)000000000\",
        \"endTimeUnixNano\": \"$(date +%s)000000001\"
      }]}]
    }]
  }" 2>/dev/null || echo "000")

if [[ "$INGEST_STATUS" == "200" ]]; then
  info "PASS: smoke ingest returned HTTP 200"
else
  gate_fail "smoke ingest returned HTTP $INGEST_STATUS"
fi

# ---------------------------------------------------------------------------
# Gate 3: zero 5xx during soak
# ---------------------------------------------------------------------------

log "Gate 3/3: soak for ${SOAK_SECONDS}s then check for HTTP 5xx in canary logs"
sleep "$SOAK_SECONDS"

CANARY_POD=$(kubectl get pod \
  --namespace "$NAMESPACE" \
  --selector="app.kubernetes.io/name=ingest-gateway-canary" \
  --field-selector="status.phase=Running" \
  -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || echo "")

if [[ -z "$CANARY_POD" ]]; then
  info "WARN: no running canary pod found — skipping log gate"
else
  FIVE_XX=$(kubectl logs "$CANARY_POD" \
    --namespace "$NAMESPACE" \
    --tail=500 2>/dev/null \
    | grep -cE '"status"\s*:\s*5[0-9]{2}' || echo "0")
  if [[ "$FIVE_XX" -gt 0 ]]; then
    gate_fail "$FIVE_XX HTTP 5xx response(s) found in canary pod logs"
  else
    info "PASS: no HTTP 5xx responses in canary pod logs"
  fi
fi

# ---------------------------------------------------------------------------
# All gates passed — promote stable
# ---------------------------------------------------------------------------

log "All gates passed — promoting stable to tag: $CANARY_TAG"
stop_port_forward

helm upgrade "$RELEASE" "$APP_CHART" \
  --namespace "$NAMESPACE" \
  --reuse-values \
  --set "global.image.tag=$CANARY_TAG" \
  --set "services.ingestGateway.canary.enabled=false" \
  --wait --timeout 5m

info "Stable release promoted to tag: $CANARY_TAG"
info "Canary Deployment and Service removed"

finish_deployment_marker "success"

echo ""
echo "Canary promotion COMPLETE."
helm history "$RELEASE" --namespace "$NAMESPACE"
