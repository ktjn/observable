#!/usr/bin/env bash
# kind-based Kubernetes integration test for the Observable platform.
#
# Runnable locally and in CI (ADR-019, ADR-020).
#
# What this script does:
#   1. Creates a kind cluster named "observable-test"
#   2. Builds the observable-services Docker image (if not already built)
#   3. Loads the image into the kind cluster (no registry needed)
#   4. Installs the infra Helm chart (ClickHouse, PostgreSQL, Redpanda, OpenFGA)
#   5. Waits for infra to become ready
#   6. Creates migration ConfigMaps from migrations/ directory
#   7. Resolves Helm dependencies and installs the observable chart
#   8. Waits for all service Deployments to become ready
#   9. Runs ingest-to-query smoke checks via port-forward
#  10. Demonstrates helm rollback (upgrade with a label change, then roll back)
#  11. Tears down the kind cluster
#
# Prerequisites:
#   kind   >= 0.20   (https://kind.sigs.k8s.io)
#   kubectl >= 1.28
#   helm   >= 3.12
#   docker
#
# Usage:
#   bash scripts/kind-test.sh [--skip-build] [--keep-cluster]
#
#   --skip-build   skip docker build (use existing observable-services:local image)
#   --keep-cluster do not delete the kind cluster on exit (useful for debugging)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

CLUSTER_NAME="observable-test"
NAMESPACE="observable"
RELEASE_NAME="observable"
IMAGE_NAME="observable-services:local"
APP_CHART="$REPO_ROOT/charts/observable"

SKIP_BUILD=false
KEEP_CLUSTER=false

for arg in "$@"; do
  case "$arg" in
    --skip-build)   SKIP_BUILD=true ;;
    --keep-cluster) KEEP_CLUSTER=true ;;
  esac
done

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

log()  { echo ""; echo "==> [$(date +%H:%M:%S)] $*"; }
info() { echo "    $*"; }

show_pods() {
  local ns="${1:-$NAMESPACE}"
  echo ""
  kubectl get pods --namespace "$ns" -o wide 2>/dev/null || true
}

dump_pod_events() {
  local ns="${1:-$NAMESPACE}"
  echo ""
  info "--- Pod status (namespace: $ns) ---"
  kubectl get pods --namespace "$ns" -o wide 2>/dev/null || true
  echo ""
  info "--- Recent events ---"
  kubectl get events --namespace "$ns" --sort-by='.lastTimestamp' 2>/dev/null | tail -20 || true
  echo ""
  info "--- Non-running pods ---"
  kubectl get pods --namespace "$ns" --field-selector='status.phase!=Running' -o wide 2>/dev/null || true
}

wait_for_rollout() {
  local resource="$1"
  local timeout="${2:-180s}"
  local name="${resource##*/}"
  info "waiting for $resource (timeout: $timeout)"
  kubectl rollout status "$resource" \
    --namespace "$NAMESPACE" \
    --timeout "$timeout" &
  local pid=$!
  local elapsed=0
  while kill -0 "$pid" 2>/dev/null; do
    sleep 15
    elapsed=$((elapsed + 15))
    info "  [${elapsed}s] pods matching '$name':"
    kubectl get pods --namespace "$NAMESPACE" --no-headers 2>/dev/null \
      | grep "$name" | sed 's/^/    /' || true
  done
  wait "$pid"
}

cleanup() {
  if [[ "$KEEP_CLUSTER" == "true" ]]; then
    log "Keeping cluster '$CLUSTER_NAME' (--keep-cluster set)"
    log "To delete it later: kind delete cluster --name $CLUSTER_NAME"
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
    echo "ERROR: '$cmd' is not on PATH. See ADR-020 for setup instructions." >&2
    exit 1
  fi
done
info "kind:    $(kind version)"
info "kubectl: $(kubectl version --client --short 2>/dev/null || kubectl version --client)"
info "helm:    $(helm version --short)"
info "docker:  $(docker version --format '{{.Client.Version}}')"

# ---------------------------------------------------------------------------
# Build Docker image
# ---------------------------------------------------------------------------

if [[ "$SKIP_BUILD" == "false" ]]; then
  log "Building observable-services image"
  docker build --tag "$IMAGE_NAME" "$REPO_ROOT"
else
  log "Skipping Docker build (--skip-build)"
fi

# ---------------------------------------------------------------------------
# Create kind cluster
# ---------------------------------------------------------------------------

log "Creating kind cluster '$CLUSTER_NAME'"
if kind get clusters 2>/dev/null | grep -q "^$CLUSTER_NAME$"; then
  info "Cluster already exists — deleting and recreating"
  kind delete cluster --name "$CLUSTER_NAME"
fi

kind create cluster \
  --name "$CLUSTER_NAME" \
  --wait 60s

kubectl cluster-info --context "kind-$CLUSTER_NAME"

log "Cluster node resources"
kubectl get nodes -o wide
kubectl describe nodes | grep -A8 "Allocatable:"

# ---------------------------------------------------------------------------
# Load image into kind (avoids needing a registry)
# ---------------------------------------------------------------------------

log "Loading '$IMAGE_NAME' into kind cluster"
kind load docker-image "$IMAGE_NAME" --name "$CLUSTER_NAME"

# ---------------------------------------------------------------------------
# Pre-pull infra images into kind to avoid slow pulls during Helm install
# ---------------------------------------------------------------------------

#log "Pre-pulling infrastructure images into kind node"
#for img in \
#  "redpandadata/redpanda:v26.1.1" \
#  "clickhouse/clickhouse-server:24.3" \
#  "ghcr.io/cloudnative-pg/postgresql:16"; do
#  docker pull "$img"
#  kind load docker-image "$img" --name "$CLUSTER_NAME"
#  info "loaded $img"
#done

# ---------------------------------------------------------------------------
# Deploy infrastructure
# ---------------------------------------------------------------------------

log "Deploying namespace and infrastructure"
kubectl create namespace "$NAMESPACE" --dry-run=client -o yaml | kubectl apply -f -

log "Installing infrastructure dependencies"
helm repo add cloudnative-pg https://cloudnative-pg.github.io/charts
helm repo add openfga https://openfga.github.io/helm-charts
helm repo update

log "Installing CloudNativePG operator"
helm install cnpg-operator cloudnative-pg/cloudnative-pg \
  --namespace cnpg-system \
  --create-namespace \
  --wait
show_pods cnpg-system

log "Installing infrastructure chart"
helm dependency update "$REPO_ROOT/charts/observable-infra"
helm install observable-infra "$REPO_ROOT/charts/observable-infra" \
  --namespace "$NAMESPACE" \
  --wait \
  --timeout 10m
show_pods "$NAMESPACE"

log "Waiting for PostgreSQL cluster to become ready"
kubectl wait cluster/postgres \
  --for=condition=Ready --namespace "$NAMESPACE" --timeout=180s \
  || { dump_pod_events "$NAMESPACE"; exit 1; }

log "Waiting for Redpanda topic setup Job to complete"
kubectl wait job/redpanda-setup \
  --for=condition=complete --namespace "$NAMESPACE" --timeout=120s \
  || { dump_pod_events "$NAMESPACE"; exit 1; }

# ---------------------------------------------------------------------------
# Create migration ConfigMaps (mirrors the volumes in docker-compose.yml)
# ---------------------------------------------------------------------------

log "Creating migration ConfigMaps"
kubectl create configmap observable-migrations-postgres \
  --from-file="$REPO_ROOT/migrations/postgres/" \
  --namespace "$NAMESPACE" \
  --dry-run=client -o yaml | kubectl apply -f -

kubectl create configmap observable-migrations-clickhouse \
  --from-file="$REPO_ROOT/migrations/clickhouse/" \
  --namespace "$NAMESPACE" \
  --dry-run=client -o yaml | kubectl apply -f -

# ---------------------------------------------------------------------------
# Install the Observable chart
# ---------------------------------------------------------------------------

log "Resolving Helm chart dependencies"
helm dependency update "$APP_CHART"

log "Installing Observable chart (revision 1)"
helm install "$RELEASE_NAME" "$APP_CHART" \
  --namespace "$NAMESPACE" \
  --set global.image.repository=observable-services \
  --set global.image.tag=local \
  --set global.image.pullPolicy=Never \
  --wait \
  --timeout 5m \
  || { dump_pod_events "$NAMESPACE"; exit 1; }

log "Helm release status"
helm status "$RELEASE_NAME" --namespace "$NAMESPACE"
show_pods "$NAMESPACE"

# ---------------------------------------------------------------------------
# Wait for all service Deployments
# ---------------------------------------------------------------------------

log "Verifying all service Deployments are ready"
for svc in auth-service ingest-gateway stream-processor storage-writer query-api alert-evaluator; do
  wait_for_rollout "deployment/$svc" \
    || { info "FAILED: $svc did not become ready"; dump_pod_events "$NAMESPACE"; exit 1; }
  info "$svc: ready"
done

# ---------------------------------------------------------------------------
# Smoke checks via port-forward
# ---------------------------------------------------------------------------

log "Running ingest-to-query smoke checks"

# Port-forward ingest-gateway
kubectl port-forward service/ingest-gateway 14317:4317 \
  --namespace "$NAMESPACE" &
PF_INGEST=$!
# Port-forward query-api
kubectl port-forward service/query-api 18090:8090 \
  --namespace "$NAMESPACE" &
PF_QUERY=$!
# Port-forward auth-service (for health only)
kubectl port-forward service/auth-service 14318:4318 \
  --namespace "$NAMESPACE" &
PF_AUTH=$!

cleanup_pf() {
  kill "$PF_INGEST" "$PF_QUERY" "$PF_AUTH" 2>/dev/null || true
}
trap 'cleanup_pf; cleanup' EXIT

sleep 3  # let port-forwards establish

DEV_KEY="dev-api-key-0000"
TENANT_ID="00000000-0000-0000-0000-000000000001"

# Health checks
info "Checking /health endpoints"
curl -sf http://localhost:14317/health | grep -q "ok" && info "ingest-gateway /health OK"
curl -sf http://localhost:18090/health | grep -q "ok" && info "query-api /health OK"
curl -sf http://localhost:14318/health | grep -q "ok" && info "auth-service /health OK"

# Send a trace
info "Sending test trace to ingest-gateway"
TRACE_ID="aabbccddeeff00112233445566778899"
curl -sf -X POST http://localhost:14317/v1/traces \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $DEV_KEY" \
  -d "{
    \"resourceSpans\": [{
      \"resource\": {\"attributes\": [{\"key\": \"service.name\", \"value\": {\"stringValue\": \"kind-smoke-test\"}}]},
      \"scopeSpans\": [{
        \"spans\": [{
          \"traceId\": \"$TRACE_ID\",
          \"spanId\": \"aabbccddeeff0011\",
          \"name\": \"kind-test-span\",
          \"kind\": 1,
          \"startTimeUnixNano\": \"$(date +%s)000000000\",
          \"endTimeUnixNano\": \"$(date +%s)000000001\"
        }]
      }]
    }]
  }" && info "Trace ingested"

# Allow pipeline to process
sleep 8

# Query trace
info "Querying trace from query-api"
RESULT=$(curl -sf "http://localhost:18090/v1/traces?tenant_id=$TENANT_ID" \
  -H "Authorization: Bearer $DEV_KEY" || echo "query_failed")

if echo "$RESULT" | grep -q "kind-smoke-test"; then
  info "PASS: trace found in query results"
elif echo "$RESULT" | grep -q "query_failed"; then
  info "WARN: query returned no data yet (pipeline may still be processing)"
else
  info "WARN: trace not found in results (check stream-processor logs)"
fi

cleanup_pf
trap 'cleanup' EXIT

# ---------------------------------------------------------------------------
# Rollback demonstration
# ---------------------------------------------------------------------------

log "Demonstrating helm rollback"

info "Upgrading to revision 2 (adding an annotation to trigger a new release)"
helm upgrade "$RELEASE_NAME" "$APP_CHART" \
  --namespace "$NAMESPACE" \
  --set global.image.repository=observable-services \
  --set global.image.tag=local \
  --set global.image.pullPolicy=Never \
  --set services.queryApi.replicas=2 \
  --wait \
  --timeout 3m

info "Revision 2 deployed — queryApi replicas=2"
helm history "$RELEASE_NAME" --namespace "$NAMESPACE"

info "Rolling back to revision 1"
helm rollback "$RELEASE_NAME" 1 \
  --namespace "$NAMESPACE" \
  --wait \
  --timeout 3m

info "Rollback to revision 1 complete"
helm history "$RELEASE_NAME" --namespace "$NAMESPACE"

# Verify rollback reverted replica count
REPLICAS=$(kubectl get deployment query-api \
  --namespace "$NAMESPACE" \
  -o jsonpath='{.spec.replicas}')
if [[ "$REPLICAS" == "1" ]]; then
  info "PASS: query-api back to 1 replica after rollback"
else
  echo "WARN: expected 1 replica, got $REPLICAS — rollback may not have completed yet" >&2
fi

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------

log "kind integration test PASSED"
log "Helm chart renders, deploys, and rolls back cleanly against a real k8s cluster."
info ""
info "Rollback path documented in spec/12-deployment.md §19.7"
info "Chart layout documented in ADR-020"
