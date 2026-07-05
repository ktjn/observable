#!/usr/bin/env bash
# Deploy the Observable test bench into a kind cluster and keep it running
# for manual testing.
#
# Both the Observable frontend and the testbench shop-frontend are exposed
# through the Kubernetes Gateway API (nginx-gateway-fabric):
#
#   http://localhost:8080/   Observable frontend
#   http://localhost:3000/   Testbench shop (shop-frontend BFF)
#
# The script blocks until you press Ctrl+C, which tears down the cluster.
#
# Prerequisites:
#   kind   >= 0.20
#   kubectl >= 1.28
#   helm   >= 3.12
#   docker
#   jq
#
# Usage:
#   bash scripts/testbench.sh [options]
#
#   --skip-build        Skip docker builds (use pre-existing testbench-*:local images)
#   --keep-cluster      Do not delete the kind cluster on exit
#   --recreate          Delete and recreate the kind cluster even if it already exists
#   --skip-observable   Skip deploying Observable (assume it is already running)
#   --observable-ns     Namespace where Observable is deployed (default: observable)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=scripts/lib.sh
source "$SCRIPT_DIR/lib.sh"

CLUSTER_NAME="observable-test"
OBSERVABLE_NS="observable"
TESTBENCH_NS="testbench"
TESTBENCH_CHART="$REPO_ROOT/charts/observable-testbench"
TESTBENCH_RELEASE="observable-testbench"
KIND_CONFIG="$SCRIPT_DIR/testbench-kind-config.yaml"

# Gateway API — update versions when newer stable releases are available:
#   https://github.com/kubernetes-sigs/gateway-api/releases
#   https://github.com/nginx/nginx-gateway-fabric/releases
GATEWAY_API_VERSION="v1.2.1"
NGF_CHART_VERSION="1.5.1"
NGF_NAMESPACE="nginx-gateway"
NGF_RELEASE="ngf"
GATEWAY_NODEPORT_OBSERVABLE=30080
GATEWAY_NODEPORT_SHOP=30300
GATEWAY_HOST_PORT_OBSERVABLE=8080
GATEWAY_HOST_PORT_SHOP=3000

SKIP_BUILD=false
KEEP_CLUSTER=false
RECREATE_CLUSTER=false
SKIP_OBSERVABLE=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build)      SKIP_BUILD=true;       shift ;;
    --keep-cluster)    KEEP_CLUSTER=true;     shift ;;
    --recreate)        RECREATE_CLUSTER=true; shift ;;
    --skip-observable) SKIP_OBSERVABLE=true;  shift ;;
    --observable-ns)
      if [[ $# -lt 2 ]]; then
        echo "ERROR: --observable-ns requires a namespace value." >&2; exit 1
      fi
      OBSERVABLE_NS="$2"; shift 2 ;;
    *)
      echo "ERROR: unknown argument: $1" >&2; exit 1 ;;
  esac
done

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

reset_stale_release() {
  local release="$1" ns="$2" status
  if ! status="$(helm status "$release" --namespace "$ns" 2>/dev/null | awk '/^STATUS:/ {print $2}')"; then
    status=""
  fi
  case "$status" in
    pending-install|pending-upgrade|pending-rollback|failed)
      log "Removing stale Helm release '$release' in namespace '$ns' (status: $status)"
      helm uninstall "$release" --namespace "$ns" || true
      ;;
  esac
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
trap 'echo ""; echo "ERROR: command failed at line $LINENO: $BASH_COMMAND" >&2; KEEP_CLUSTER=true' ERR

# ---------------------------------------------------------------------------
# Prerequisite checks
# ---------------------------------------------------------------------------

log "Checking prerequisites"
for cmd in kind kubectl helm docker jq; do
  if ! command -v "$cmd" &>/dev/null; then
    echo "ERROR: '$cmd' is not on PATH." >&2; exit 1
  fi
done
info "kind:    $(kind version)"
info "kubectl: $(kubectl version --client --short 2>/dev/null || kubectl version --client)"
info "helm:    $(helm version --short)"
info "docker:  $(docker version --format '{{.Client.Version}}')"
info "jq:      $(jq --version)"

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
  log "Building testbench Docker images in parallel"
  build_images_parallel "${TESTBENCH_IMAGES[@]}"
else
  log "Skipping Docker builds (--skip-build)"
fi

# ---------------------------------------------------------------------------
# Create or reuse kind cluster
# ---------------------------------------------------------------------------

log "Setting up kind cluster '$CLUSTER_NAME'"
if kind get clusters 2>/dev/null | grep -q "^$CLUSTER_NAME$"; then
  if [[ "$RECREATE_CLUSTER" == "true" ]]; then
    info "Deleting existing cluster for fresh start (--recreate)"
    kind delete cluster --name "$CLUSTER_NAME"
    kind create cluster \
      --name "$CLUSTER_NAME" \
      --config "$KIND_CONFIG" \
      --wait 60s
  else
    info "Cluster already exists — reusing it"
    info "NOTE: host ports 8080 and 3000 must have been mapped at cluster creation."
    info "      Run with --recreate if the cluster was not created by this script."
  fi
else
  kind create cluster \
    --name "$CLUSTER_NAME" \
    --config "$KIND_CONFIG" \
    --wait 60s
fi
kubectl cluster-info --context "kind-$CLUSTER_NAME"

# ---------------------------------------------------------------------------
# Deploy Observable platform (unless --skip-observable)
# ---------------------------------------------------------------------------

if [[ "$SKIP_OBSERVABLE" == "false" ]]; then
  log "Deploying Observable platform via kind-test.sh"
  kind_test_args=(
    --keep-cluster
    --reuse-cluster
    --cluster-name "$CLUSTER_NAME"
    --deploy-only
  )
  [[ "$SKIP_BUILD" == "true" ]] && kind_test_args=(--skip-build "${kind_test_args[@]}")
  bash "$SCRIPT_DIR/kind-test.sh" "${kind_test_args[@]}"
else
  log "Skipping Observable deployment (--skip-observable)"
fi

# ---------------------------------------------------------------------------
# Load testbench images + install testbench Helm chart
# ---------------------------------------------------------------------------

log "Loading testbench images and resolving chart dependencies in parallel"

TESTBENCH_IMAGE_TAGS=()
for entry in "${TESTBENCH_IMAGES[@]}"; do
  TESTBENCH_IMAGE_TAGS+=("${entry%:*}")
done

helm repo add --force-update open-telemetry https://open-telemetry.github.io/opentelemetry-helm-charts
helm repo update

load_images_parallel "$CLUSTER_NAME" "${TESTBENCH_IMAGE_TAGS[@]}" &
LOAD_PID=$!
helm dependency update "$TESTBENCH_CHART" &
DEP_PID=$!
wait "$LOAD_PID" || { KEEP_CLUSTER=true; echo "ERROR: image loading failed" >&2; exit 1; }
wait "$DEP_PID"  || { KEEP_CLUSTER=true; echo "ERROR: helm dependency update failed" >&2; exit 1; }

log "Creating testbench namespace"
kubectl create namespace "$TESTBENCH_NS" --dry-run=client -o yaml | kubectl apply -f -

reset_stale_release "$TESTBENCH_RELEASE" "$TESTBENCH_NS"

log "Installing observable-testbench chart"
helm upgrade --install "$TESTBENCH_RELEASE" "$TESTBENCH_CHART" \
  --namespace "$TESTBENCH_NS" \
  --wait \
  --timeout 10m \
  || { KEEP_CLUSTER=true; dump_pod_events "$TESTBENCH_NS"; exit 1; }

show_pods "$TESTBENCH_NS"

log "Waiting for testbench Deployments in parallel"
wait_for_rollouts_parallel "$TESTBENCH_NS" 300s \
  deployment/otel-collector-gateway \
  deployment/shop-api \
  deployment/shop-frontend \
  deployment/shop-loadgen \
  deployment/shop-worker \
  || { KEEP_CLUSTER=true; exit 1; }

log "Waiting for otel-collector-agent DaemonSet"
kubectl rollout status daemonset/otel-collector-agent \
  --namespace "$TESTBENCH_NS" --timeout 120s \
  || info "WARN: agent DaemonSet not fully ready"

# ---------------------------------------------------------------------------
# Install Kubernetes Gateway API CRDs
# ---------------------------------------------------------------------------

log "Installing Kubernetes Gateway API CRDs (${GATEWAY_API_VERSION})"
kubectl apply -f \
  "https://github.com/kubernetes-sigs/gateway-api/releases/download/${GATEWAY_API_VERSION}/standard-install.yaml"

kubectl wait --for=condition=Established \
  crd/gateways.gateway.networking.k8s.io \
  crd/httproutes.gateway.networking.k8s.io \
  --timeout=60s

# ---------------------------------------------------------------------------
# Install nginx-gateway-fabric
# ---------------------------------------------------------------------------

log "Installing nginx-gateway-fabric chart (${NGF_CHART_VERSION})"
helm upgrade --install "$NGF_RELEASE" \
  oci://ghcr.io/nginx/charts/nginx-gateway-fabric \
  --create-namespace \
  --namespace "$NGF_NAMESPACE" \
  --version "$NGF_CHART_VERSION" \
  --set service.type=NodePort \
  --wait --timeout 5m

# NGF static mode manages its Service ports at install time only; it does not
# add listener ports automatically when Gateway resources are created.
# Pin port 80's NodePort immediately — the port already exists in the Service.
NGF_SVC="${NGF_RELEASE}-nginx-gateway-fabric"
log "Pinning Observable NodePort (port 80 → ${GATEWAY_NODEPORT_OBSERVABLE})"
PATCH80=$(kubectl get service "$NGF_SVC" -n "$NGF_NAMESPACE" -o json \
  | jq --argjson np "${GATEWAY_NODEPORT_OBSERVABLE}" \
       '[.spec.ports | to_entries[]
        | select(.value.port == 80)
        | {"op": "replace", "path": "/spec/ports/\(.key)/nodePort", "value": $np}]')
kubectl patch service "$NGF_SVC" -n "$NGF_NAMESPACE" --type=json -p="$PATCH80"
info "port 80 → NodePort ${GATEWAY_NODEPORT_OBSERVABLE}"

# ---------------------------------------------------------------------------
# Apply Gateway and HTTPRoutes
# ---------------------------------------------------------------------------

log "Applying Gateway and HTTPRoutes"
kubectl apply -f - <<GATEWAY_MANIFEST
apiVersion: gateway.networking.k8s.io/v1
kind: Gateway
metadata:
  name: testbench-gateway
  namespace: ${OBSERVABLE_NS}
spec:
  gatewayClassName: nginx
  listeners:
    - name: observable
      protocol: HTTP
      port: 80
      allowedRoutes:
        namespaces:
          from: All
    - name: shop
      protocol: HTTP
      port: 3000
      allowedRoutes:
        namespaces:
          from: All
---
apiVersion: gateway.networking.k8s.io/v1
kind: HTTPRoute
metadata:
  name: observable-frontend
  namespace: ${OBSERVABLE_NS}
spec:
  parentRefs:
    - name: testbench-gateway
      namespace: ${OBSERVABLE_NS}
      sectionName: observable
  rules:
    - matches:
        - path:
            type: PathPrefix
            value: /
      backendRefs:
        - name: frontend
          port: 80
---
apiVersion: gateway.networking.k8s.io/v1
kind: HTTPRoute
metadata:
  name: testbench-shop
  namespace: ${TESTBENCH_NS}
spec:
  parentRefs:
    - name: testbench-gateway
      namespace: ${OBSERVABLE_NS}
      sectionName: shop
  rules:
    - matches:
        - path:
            type: PathPrefix
            value: /
      backendRefs:
        - name: shop-frontend
          port: 3000
GATEWAY_MANIFEST

# ---------------------------------------------------------------------------
# Add shop port 3000 to the NGF Service
# NGF static mode does not add Gateway listener ports to the Service; we
# append port 3000 directly.  NGINX is already listening on 3000 because NGF
# processed the Gateway resource applied above.
# ---------------------------------------------------------------------------

log "Adding shop listener to NGF Service (port 3000 → NodePort ${GATEWAY_NODEPORT_SHOP})"
# Use "replace" when port 3000 already exists (idempotent re-runs / --keep-cluster),
# "add" only when it is absent.
SHOP_PORT_IDX=$(kubectl get service "$NGF_SVC" -n "$NGF_NAMESPACE" -o json \
  | jq '[.spec.ports | to_entries[] | select(.value.port == 3000) | .key] | first // empty')
if [[ -n "$SHOP_PORT_IDX" ]]; then
  kubectl patch service "$NGF_SVC" -n "$NGF_NAMESPACE" --type=json \
    -p="[{\"op\":\"replace\",\"path\":\"/spec/ports/${SHOP_PORT_IDX}/nodePort\",\"value\":${GATEWAY_NODEPORT_SHOP}}]"
else
  kubectl patch service "$NGF_SVC" -n "$NGF_NAMESPACE" --type=json \
    -p="[{\"op\":\"add\",\"path\":\"/spec/ports/-\",\"value\":{\"name\":\"shop\",\"port\":3000,\"targetPort\":3000,\"protocol\":\"TCP\",\"nodePort\":${GATEWAY_NODEPORT_SHOP}}}]"
fi
info "port 3000 → NodePort ${GATEWAY_NODEPORT_SHOP}"

# ---------------------------------------------------------------------------
# Wait for Gateway to be Programmed
# ---------------------------------------------------------------------------

log "Waiting for Gateway to be Programmed"
kubectl wait gateway/testbench-gateway \
  --namespace "$OBSERVABLE_NS" \
  --for=condition=Programmed \
  --timeout=120s \
  || info "WARN: Gateway not yet Programmed — routes may need a few more seconds"

# ---------------------------------------------------------------------------
# Smoke check — non-fatal, warns if services are still starting
# ---------------------------------------------------------------------------

log "Running smoke check via Gateway"
sleep 5

info "Checking Observable frontend (http://localhost:${GATEWAY_HOST_PORT_OBSERVABLE}/)"
if curl -sf --max-time 10 "http://localhost:${GATEWAY_HOST_PORT_OBSERVABLE}/" \
    | grep -qi "<!doctype html"; then
  info "PASS: Observable frontend reachable"
else
  info "WARN: Observable frontend not yet reachable — may still be starting up"
fi

info "Checking testbench shop (http://localhost:${GATEWAY_HOST_PORT_SHOP}/)"
if curl -sf --max-time 10 "http://localhost:${GATEWAY_HOST_PORT_SHOP}/" \
    | grep -qi "<!doctype html"; then
  info "PASS: Testbench shop reachable"
else
  info "WARN: Testbench shop not yet reachable — may still be starting up"
fi

# ---------------------------------------------------------------------------
# Seed example alert rules (non-fatal — the demo is still usable without
# them, this just gives the Alerts & SLOs view something to show).
# Alert-rule writes go through admin-service, not query-api.
# ---------------------------------------------------------------------------

log "Seeding example alert rules via admin-service"
ADMIN_SERVICE_PORT_LOCAL=14324
DEV_KEY="dev-api-key-0000"
DEV_TENANT="00000000-0000-0000-0000-000000000002"

kubectl port-forward svc/admin-service "${ADMIN_SERVICE_PORT_LOCAL}:4324" \
  --namespace "$OBSERVABLE_NS" >/tmp/testbench-admin-port-forward.log 2>&1 &
ADMIN_PF_PID=$!

seed_alert_rule() {
  local body="$1"
  curl -sf --max-time 10 -X POST "http://localhost:${ADMIN_SERVICE_PORT_LOCAL}/v1/admin/alerts/rules" \
    -H "Authorization: Bearer ${DEV_KEY}" \
    -H "X-Tenant-ID: ${DEV_TENANT}" \
    -H "Content-Type: application/json" \
    -d "$body" >/dev/null
}

if timeout 15 bash -c "until curl -sf --max-time 2 http://localhost:${ADMIN_SERVICE_PORT_LOCAL}/health >/dev/null 2>&1; do sleep 1; done"; then
  if seed_alert_rule '{"name":"Checkout high error rate","alert_type":"threshold","metric_name":"http.server.errors","operator":"gt","threshold":0.05,"service_name":"shop-api"}' \
    && seed_alert_rule '{"name":"Checkout high latency","alert_type":"threshold","metric_name":"http.server.duration","operator":"gt","threshold":500,"service_name":"shop-api"}'; then
    info "PASS: seeded 2 example alert rules"
  else
    info "WARN: failed to seed one or more example alert rules — continuing without them"
  fi
else
  info "WARN: admin-service not reachable via port-forward — skipping alert rule seeding"
fi

kill "$ADMIN_PF_PID" 2>/dev/null || true
wait "$ADMIN_PF_PID" 2>/dev/null || true

# ---------------------------------------------------------------------------
# Ready — print access information and block until Ctrl+C
# ---------------------------------------------------------------------------

log "Test bench is running"
info ""
info "  Observable frontend:  http://localhost:${GATEWAY_HOST_PORT_OBSERVABLE}/"
info "  Testbench shop:       http://localhost:${GATEWAY_HOST_PORT_SHOP}/"
info ""
info "Useful commands:"
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
info "  # Query Observable directly (bypasses UI):"
info "  kubectl port-forward svc/query-api 8090:8090 -n $OBSERVABLE_NS &"
info "  DEV_KEY=dev-api-key-0000"
info "  TENANT=00000000-0000-0000-0000-000000000001"
info "  curl -s \"http://localhost:8090/v1/traces?tenant_id=\$TENANT\" \\"
info "    -H \"Authorization: Bearer \$DEV_KEY\" | jq '[.[].service_name] | unique'"
info ""
info "Press Ctrl+C to tear down the cluster."
info ""

log "Cluster running — press Ctrl+C to stop"
while true; do
  sleep 60
  echo "    [$(date +%H:%M:%S)] cluster running — Ctrl+C to stop"
done
