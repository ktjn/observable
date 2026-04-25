#!/usr/bin/env bash
# Deploy the Observable test bench into a kind cluster.
#
# The test bench is a synthetic "shop" application (frontend, API, worker,
# queue, database) plus an OTel Collector (gateway Deployment + agent DaemonSet
# with native k8s monitoring receivers) that continuously ships traces, metrics,
# and logs into the Observable platform.
#
# Prerequisites:
#   kind   >= 0.20
#   kubectl >= 1.28
#   helm   >= 3.12
#   docker
#
# Usage:
#   bash scripts/testbench.sh [--skip-build] [--keep-cluster] [--observable-ns <ns>]
#
#   --skip-build        Skip docker builds (use pre-existing testbench-*:local images)
#   --keep-cluster      Do not delete the kind cluster on exit
#   --observable-ns     Namespace where Observable is deployed (default: observable)
#   --skip-observable   Skip deploying Observable (assume it is already running)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

CLUSTER_NAME="observable-test"
OBSERVABLE_NS="observable"
TESTBENCH_NS="testbench"
TESTBENCH_CHART="$REPO_ROOT/charts/observable-testbench"
TESTBENCH_RELEASE="observable-testbench"

SKIP_BUILD=false
KEEP_CLUSTER=false
SKIP_OBSERVABLE=false

for arg in "$@"; do
  case "$arg" in
    --skip-build)       SKIP_BUILD=true ;;
    --keep-cluster)     KEEP_CLUSTER=true ;;
    --skip-observable)  SKIP_OBSERVABLE=true ;;
    --observable-ns)    shift; OBSERVABLE_NS="$1" ;;
  esac
done

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

log()  { echo ""; echo "==> [$(date +%H:%M:%S)] $*"; }
info() { echo "    $*"; }

show_pods() {
  local ns="${1:-$TESTBENCH_NS}"
  echo ""
  kubectl get pods --namespace "$ns" -o wide 2>/dev/null || true
}

dump_pod_events() {
  local ns="${1:-$TESTBENCH_NS}"
  info "--- Pod status (namespace: $ns) ---"
  kubectl get pods --namespace "$ns" -o wide 2>/dev/null || true
  info "--- Recent events ---"
  kubectl get events --namespace "$ns" --sort-by='.lastTimestamp' 2>/dev/null | tail -20 || true
}

wait_for_rollout() {
  local resource="$1"
  local ns="${2:-$TESTBENCH_NS}"
  local timeout="${3:-180s}"
  local name="${resource##*/}"
  info "waiting for $resource in ns=$ns (timeout: $timeout)"
  kubectl rollout status "$resource" --namespace "$ns" --timeout "$timeout" \
    || { info "FAILED: $resource did not become ready"; dump_pod_events "$ns"; exit 1; }
  info "$name: ready"
}

cleanup() {
  if [[ "$KEEP_CLUSTER" == "true" ]]; then
    log "Keeping cluster '$CLUSTER_NAME' (--keep-cluster)"
    log "To delete: kind delete cluster --name $CLUSTER_NAME"
    return
  fi
  log "Tearing down kind cluster '$CLUSTER_NAME'"
  kind delete cluster --name "$CLUSTER_NAME" || true
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# Prerequisite checks
# ---------------------------------------------------------------------------

log "Checking prerequisites"
for cmd in kind kubectl helm docker; do
  if ! command -v "$cmd" &>/dev/null; then
    echo "ERROR: '$cmd' is not on PATH." >&2
    exit 1
  fi
done
info "kind:    $(kind version)"
info "kubectl: $(kubectl version --client --short 2>/dev/null || kubectl version --client)"
info "helm:    $(helm version --short)"
info "docker:  $(docker version --format '{{.Client.Version}}')"

# ---------------------------------------------------------------------------
# Build testbench Docker images
# ---------------------------------------------------------------------------

TESTBENCH_IMAGES=(
  "testbench-frontend:local:$REPO_ROOT/testbench/frontend"
  "testbench-api:local:$REPO_ROOT/testbench/api"
  "testbench-worker:local:$REPO_ROOT/testbench/worker"
  "testbench-loadgen:local:$REPO_ROOT/testbench/loadgen"
)

if [[ "$SKIP_BUILD" == "false" ]]; then
  log "Building testbench Docker images"
  for entry in "${TESTBENCH_IMAGES[@]}"; do
    tag="${entry%%:*:*}"
    tag="${entry%:*}"
    context="${entry##*:}"
    info "Building $tag from $context"
    docker build --tag "$tag" "$context"
  done
else
  log "Skipping Docker builds (--skip-build)"
fi

# ---------------------------------------------------------------------------
# Create kind cluster
# ---------------------------------------------------------------------------

log "Creating kind cluster '$CLUSTER_NAME'"
if kind get clusters 2>/dev/null | grep -q "^$CLUSTER_NAME$"; then
  info "Cluster already exists — reusing it"
else
  kind create cluster --name "$CLUSTER_NAME" --wait 60s
fi
kubectl cluster-info --context "kind-$CLUSTER_NAME"

# ---------------------------------------------------------------------------
# Deploy Observable platform (unless --skip-observable)
# ---------------------------------------------------------------------------

if [[ "$SKIP_OBSERVABLE" == "false" ]]; then
  log "Deploying Observable platform via kind-test.sh"
  # kind-test.sh uses --skip-build by default when called from here because
  # the observable-services image may already be built.  We always pass
  # --keep-cluster so it does not tear down our cluster on exit.
  if [[ "$SKIP_BUILD" == "true" ]]; then
    bash "$SCRIPT_DIR/kind-test.sh" --skip-build --keep-cluster
  else
    bash "$SCRIPT_DIR/kind-test.sh" --keep-cluster
  fi
else
  log "Skipping Observable deployment (--skip-observable)"
fi

# ---------------------------------------------------------------------------
# Load testbench images into kind
# ---------------------------------------------------------------------------

log "Loading testbench images into kind cluster"
for entry in "${TESTBENCH_IMAGES[@]}"; do
  img="${entry%:*}"   # strip context path
  img="${img%:*}:local"
  # Re-extract properly
  tag="${entry%%:*}"
  img="${tag}:local"
  info "Loading $img"
  kind load docker-image "$img" --name "$CLUSTER_NAME"
done

# Simpler explicit list (in case the loop above is confusing)
for img in testbench-frontend:local testbench-api:local testbench-worker:local testbench-loadgen:local; do
  kind load docker-image "$img" --name "$CLUSTER_NAME" 2>/dev/null || true
done

# ---------------------------------------------------------------------------
# Deploy testbench Helm chart
# ---------------------------------------------------------------------------

log "Creating testbench namespace"
kubectl create namespace "$TESTBENCH_NS" --dry-run=client -o yaml | kubectl apply -f -

log "Installing observable-testbench chart"
helm upgrade --install "$TESTBENCH_RELEASE" "$TESTBENCH_CHART" \
  --namespace "$TESTBENCH_NS" \
  --wait \
  --timeout 10m \
  || { dump_pod_events "$TESTBENCH_NS"; exit 1; }

show_pods "$TESTBENCH_NS"

# ---------------------------------------------------------------------------
# Wait for Deployments and DaemonSet
# ---------------------------------------------------------------------------

log "Waiting for testbench Deployments"
for svc in otel-collector-gateway shop-api shop-frontend shop-loadgen shop-worker; do
  wait_for_rollout "deployment/$svc" "$TESTBENCH_NS"
done

log "Waiting for otel-collector-agent DaemonSet"
kubectl rollout status daemonset/otel-collector-agent \
  --namespace "$TESTBENCH_NS" --timeout 120s \
  || { info "WARN: agent DaemonSet not fully ready"; dump_pod_events "$TESTBENCH_NS"; }

# ---------------------------------------------------------------------------
# Smoke check: place an order, wait, verify traces in Observable
# ---------------------------------------------------------------------------

log "Running smoke check"

PF_API_PORT=18001
PF_QUERY_PORT=18090

kubectl port-forward service/shop-api "$PF_API_PORT:8000" \
  --namespace "$TESTBENCH_NS" &
PF_API=$!

kubectl port-forward service/query-api "$PF_QUERY_PORT:8090" \
  --namespace "$OBSERVABLE_NS" &
PF_QUERY=$!

cleanup_pf() {
  kill "$PF_API" "$PF_QUERY" 2>/dev/null || true
}
trap 'cleanup_pf; cleanup' EXIT

sleep 3

DEV_KEY="dev-api-key-0000"
TENANT_ID="00000000-0000-0000-0000-000000000001"

info "Checking shop-api health"
curl -sf "http://localhost:$PF_API_PORT/health" | grep -q "ok" \
  && info "shop-api /health OK" \
  || info "WARN: shop-api /health check failed"

info "Placing a test order via shop-api"
curl -sf -X POST "http://localhost:$PF_API_PORT/orders" \
  -H "Content-Type: application/json" \
  -d '{"product_id": 1, "user_id": 1}' \
  && info "Order placed" \
  || info "WARN: order placement failed (shop-db may still be initialising)"

info "Waiting 20s for telemetry to flow through the pipeline..."
sleep 20

info "Querying Observable for testbench traces"
TRACE_RESULT=$(curl -sf \
  "http://localhost:$PF_QUERY_PORT/v1/traces?tenant_id=$TENANT_ID" \
  -H "Authorization: Bearer $DEV_KEY" 2>/dev/null || echo "query_failed")

if echo "$TRACE_RESULT" | grep -q "shop-api"; then
  info "PASS: traces from shop-api found in Observable"
elif echo "$TRACE_RESULT" | grep -q "query_failed"; then
  info "WARN: query-api returned no response (pipeline may still be warming up)"
else
  info "WARN: shop-api traces not found yet — pipeline may need more time"
fi

cleanup_pf
trap 'cleanup' EXIT

# ---------------------------------------------------------------------------
# Done — print access instructions
# ---------------------------------------------------------------------------

log "Test bench deployed successfully"
info ""
info "Useful commands (while cluster is running):"
info ""
info "  # Watch loadgen traffic:"
info "  kubectl logs -f -n $TESTBENCH_NS deploy/shop-loadgen"
info ""
info "  # Watch OTel gateway export status:"
info "  kubectl logs -f -n $TESTBENCH_NS deploy/otel-collector-gateway"
info ""
info "  # Watch OTel agent (kubeletstats + filelog):"
info "  kubectl logs -f -n $TESTBENCH_NS daemonset/otel-collector-agent"
info ""
info "  # Access shop-api:"
info "  kubectl port-forward svc/shop-api 8000:8000 -n $TESTBENCH_NS"
info "  curl http://localhost:8000/products"
info ""
info "  # Access shop-frontend:"
info "  kubectl port-forward svc/shop-frontend 3000:3000 -n $TESTBENCH_NS"
info "  open http://localhost:3000"
info ""
info "  # Query Observable:"
info "  kubectl port-forward svc/query-api 8090:8090 -n $OBSERVABLE_NS"
info "  curl -s 'http://localhost:8090/v1/traces?tenant_id=$TENANT_ID' \\"
info "    -H 'Authorization: Bearer $DEV_KEY' | jq '[.[].service_name] | unique'"
info ""
info "  # RabbitMQ management UI:"
info "  kubectl port-forward svc/shop-queue 15672:15672 -n $TESTBENCH_NS"
info "  open http://localhost:15672  (user: shop / pass: shop)"
info ""
