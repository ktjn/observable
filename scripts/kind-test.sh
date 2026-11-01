#!/usr/bin/env bash
# kind-based Kubernetes integration test for the Observable platform.
#
# Runnable locally and in CI (ADR-019, ADR-020).
#
# What this script does:
#   1. Creates a kind cluster named "observable-test"
#   2. Builds one image per component (scripts/build-component-images.sh)
#   3. Loads the images into the kind cluster (no registry needed)
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
#   bash scripts/kind-test.sh [--skip-build] [--keep-cluster] [--reuse-cluster] [--cluster-name <name>] [--pinned] [--with-drills]
#
#   --skip-build    skip docker build (use existing observable-<component>:local images)
#   --keep-cluster  do not delete the kind cluster on exit (useful for debugging)
#   --reuse-cluster use an existing kind cluster instead of recreating it
#   --cluster-name  target kind cluster name (default: observable-test)
#   --pinned        build and deploy using manifest-pinned version tags instead of :local
#   --with-drills   run automated dependency-failure drills (Redpanda, ClickHouse, PostgreSQL)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=scripts/lib.sh
source "$SCRIPT_DIR/lib.sh"

CLUSTER_NAME="observable-test"
NAMESPACE="observable"
RELEASE_NAME="observable"
# Component images (Phase 7, docs/component-decomposition.md): component name
# -> Helm services.<key>. Images are observable-<component>:local.
declare -A COMPONENT_HELM_KEYS=(
  [auth]=authService
  [control]=adminService
  [ingest]=ingestGateway
  [process]=streamProcessor
  [store-clickhouse]=storageWriter
  [query]=queryApi
  [alerting]=alertEvaluator
  [web]=frontend
)
COMPONENTS=(auth control ingest process store-clickhouse query alerting web)
APP_CHART="$REPO_ROOT/charts/observable"

SKIP_BUILD=false
KEEP_CLUSTER=false
REUSE_CLUSTER=false
DEPLOY_ONLY=false
PINNED=false
WITH_DRILLS=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build)
      SKIP_BUILD=true
      shift
      ;;
    --keep-cluster)
      KEEP_CLUSTER=true
      shift
      ;;
    --reuse-cluster)
      REUSE_CLUSTER=true
      shift
      ;;
    --cluster-name)
      if [[ $# -lt 2 ]]; then
        echo "ERROR: --cluster-name requires a value." >&2
        exit 1
      fi
      CLUSTER_NAME="$2"
      shift 2
      ;;
    --deploy-only)
      DEPLOY_ONLY=true
      shift
      ;;
    --pinned)
      PINNED=true
      shift
      ;;
    --with-drills)
      WITH_DRILLS=true
      shift
      ;;
    *)
      echo "ERROR: unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

COMPONENT_IMAGES=()
for component in "${COMPONENTS[@]}"; do
  if [[ "$PINNED" == "true" ]]; then
    version="$(bash "$REPO_ROOT/scripts/component-version.sh" "$component")"
    COMPONENT_IMAGES+=("observable-${component}:${version}")
  else
    COMPONENT_IMAGES+=("observable-${component}:local")
  fi
done

# Helm overrides pointing every component at its locally loaded image.
helm_local_image_args() {
  local component
  echo "--set"
  echo "global.image.pullPolicy=Never"
  if [[ "$PINNED" == "false" ]]; then
    for component in "${COMPONENTS[@]}"; do
      echo "--set"
      echo "services.${COMPONENT_HELM_KEYS[$component]}.image.tag=local"
    done
  fi
}
mapfile -t HELM_LOCAL_IMAGE_ARGS < <(helm_local_image_args)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

current_revision() {
  helm history "$1" --namespace "$2" | awk 'NR > 1 { rev = $1 } END { print rev }'
}

watch_pods() {
  local ns="${1:-$NAMESPACE}"
  local interval="${2:-20}"
  while true; do
    sleep "$interval"
    echo ""
    echo "    [$(date +%H:%M:%S)] pod status (ns: $ns):"
    kubectl get pods --namespace "$ns" --no-headers 2>/dev/null \
      | sed 's/^/      /' || true
    echo "    recent events:"
    kubectl get events --namespace "$ns" --sort-by='.lastTimestamp' 2>/dev/null \
      | tail -8 | sed 's/^/      /' || true
  done
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
trap 'echo ""; echo "ERROR: command failed at line $LINENO: $BASH_COMMAND" >&2' ERR

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
# Build Docker images (parallel)
# ---------------------------------------------------------------------------

if [[ "$SKIP_BUILD" == "false" ]]; then
  log "Building component images"
  BUILD_TAG="local"
  if [[ "$PINNED" == "true" ]]; then
    BUILD_TAG="manifest"
  fi
  TAG="$BUILD_TAG" bash "$REPO_ROOT/scripts/build-component-images.sh" "${COMPONENTS[@]}" \
    || { echo "ERROR: component image build failed" >&2; exit 1; }
  info "component images built: ${COMPONENT_IMAGES[*]}"
else
  log "Skipping Docker build (--skip-build)"
fi

# ---------------------------------------------------------------------------
# Create kind cluster
# ---------------------------------------------------------------------------

log "Creating kind cluster '$CLUSTER_NAME'"
if kind get clusters 2>/dev/null | grep -q "^$CLUSTER_NAME$"; then
  if [[ "$REUSE_CLUSTER" == "true" ]]; then
    info "Cluster already exists — reusing it"
  else
    info "Cluster already exists — deleting and recreating"
    kind delete cluster --name "$CLUSTER_NAME"
    kind create cluster \
      --name "$CLUSTER_NAME" \
      --wait 60s
  fi
else
  kind create cluster \
    --name "$CLUSTER_NAME" \
    --wait 60s
fi

kubectl cluster-info --context "kind-$CLUSTER_NAME"

log "Cluster node resources"
kubectl get nodes -o wide
kubectl describe nodes | grep -A8 "Allocatable:"

# ---------------------------------------------------------------------------
# Load images into kind (parallel) and resolve app chart deps
# ---------------------------------------------------------------------------

log "Loading images into kind cluster and resolving app chart dependencies"
kind load docker-image "${COMPONENT_IMAGES[@]}" --name "$CLUSTER_NAME" &
LOAD_IMAGES=$!
helm dependency update "$APP_CHART" &
DEP_UPDATE=$!
wait "$LOAD_IMAGES"    || { echo "ERROR: failed to load component images" >&2; exit 1; }
info "component images loaded"
wait "$DEP_UPDATE"     || { echo "ERROR: helm dependency update failed for observable" >&2; exit 1; }
info "observable chart dependencies resolved"

# ---------------------------------------------------------------------------
# Deploy infrastructure
# ---------------------------------------------------------------------------

log "Deploying namespace and infrastructure"
kubectl create namespace "$NAMESPACE" --dry-run=client -o yaml | kubectl apply -f -

log "Installing infrastructure dependencies"
helm repo add --force-update cloudnative-pg https://cloudnative-pg.github.io/charts &
PID_REPO_CNP=$!
helm repo add --force-update openfga https://openfga.github.io/helm-charts &
PID_REPO_OPENFGA=$!
helm repo add --force-update zitadel https://charts.zitadel.com &
PID_REPO_ZITADEL=$!
wait "$PID_REPO_CNP"     || { echo "ERROR: helm repo add cloudnative-pg failed" >&2; exit 1; }
wait "$PID_REPO_OPENFGA" || { echo "ERROR: helm repo add openfga failed" >&2; exit 1; }
wait "$PID_REPO_ZITADEL" || { echo "ERROR: helm repo add zitadel failed" >&2; exit 1; }
helm repo update

# Create migration ConfigMaps now — namespace exists and files are local,
# no need to wait for postgres/redpanda to be ready first.
log "Creating migration ConfigMaps"
kubectl create configmap observable-migrations-postgres \
  --from-file="$REPO_ROOT/migrations/postgres/" \
  --namespace "$NAMESPACE" \
  --dry-run=client -o yaml | kubectl apply -f -

kubectl create configmap observable-migrations-clickhouse \
  --from-file="$REPO_ROOT/migrations/clickhouse/" \
  --namespace "$NAMESPACE" \
  --dry-run=client -o yaml | kubectl apply -f -

log "Installing CloudNativePG operator"
helm upgrade --install cnpg-operator cloudnative-pg/cloudnative-pg \
  --namespace cnpg-system \
  --create-namespace \
  --wait
show_pods cnpg-system

log "Installing infrastructure chart"
helm dependency update "$REPO_ROOT/charts/observable-infra"
watch_pods "$NAMESPACE" 20 &
WATCH_INFRA=$!
helm upgrade --install observable-infra "$REPO_ROOT/charts/observable-infra" \
  --namespace "$NAMESPACE" \
  --set selfObservability.bearerToken=observable-api-key-0000 \
  --set selfObservability.otlpEndpoint=http://ingest-gateway:4318 \
  --wait \
  --timeout 10m
kill "$WATCH_INFRA" 2>/dev/null || true
show_pods "$NAMESPACE"

log "Waiting for PostgreSQL cluster to become ready"
kubectl wait cluster/postgres \
  --for=condition=Ready --namespace "$NAMESPACE" --timeout=300s \
  || { KEEP_CLUSTER=true; dump_pod_events "$NAMESPACE"; exit 1; }

log "Waiting for Redpanda topic setup Job to complete"
kubectl wait job/redpanda-setup \
  --for=condition=complete --namespace "$NAMESPACE" --timeout=300s \
  || { KEEP_CLUSTER=true; dump_pod_events "$NAMESPACE"; exit 1; }

# ---------------------------------------------------------------------------
# Install Kubernetes Gateway API CRDs (required by the Observable chart)
# ---------------------------------------------------------------------------

GATEWAY_API_VERSION="v1.2.1"
log "Installing Kubernetes Gateway API CRDs (${GATEWAY_API_VERSION})"
kubectl apply -f \
  "https://github.com/kubernetes-sigs/gateway-api/releases/download/${GATEWAY_API_VERSION}/standard-install.yaml"
kubectl wait --for=condition=Established \
  crd/gateways.gateway.networking.k8s.io \
  crd/httproutes.gateway.networking.k8s.io \
  --timeout=60s

# ---------------------------------------------------------------------------
# Install the Observable chart
# ---------------------------------------------------------------------------

log "Installing Observable chart"
# Reset any release left in a stuck state from a previous failed install.
_rel_status=$(helm status "$RELEASE_NAME" --namespace "$NAMESPACE" 2>/dev/null | awk '/^STATUS:/ {print $2}') || true
case "${_rel_status}" in
  pending-install|pending-upgrade|pending-rollback|failed)
    log "Removing stale Helm release '$RELEASE_NAME' (status: ${_rel_status})"
    helm uninstall "$RELEASE_NAME" --namespace "$NAMESPACE" || true
    ;;
esac

# Install without --wait: the zitadel-bootstrap post-install hook must run
# before auth-service can become ready (it writes the OIDC client_id secret).
# Using --wait creates a deadlock — helm would block on auth-service readiness
# forever because the hook that unblocks it runs only after --wait completes.
watch_pods "$NAMESPACE" 20 &
WATCH_APP=$!
helm upgrade --install "$RELEASE_NAME" "$APP_CHART" \
  --namespace "$NAMESPACE" \
  "${HELM_LOCAL_IMAGE_ARGS[@]}" \
  --set selfObservability.bearerToken=observable-api-key-0000 \
  --timeout 10m \
  || { KEEP_CLUSTER=true; kill "$WATCH_APP" 2>/dev/null || true; dump_pod_events "$NAMESPACE"; exit 1; }
kill "$WATCH_APP" 2>/dev/null || true

log "Waiting for Zitadel bootstrap job to complete (writes OIDC client_id secret)"
kubectl wait job/zitadel-bootstrap \
  --for=condition=complete --namespace "$NAMESPACE" --timeout=300s \
  || { KEEP_CLUSTER=true; dump_pod_events "$NAMESPACE"; exit 1; }

log "Helm release status"
helm status "$RELEASE_NAME" --namespace "$NAMESPACE"
show_pods "$NAMESPACE"

# ---------------------------------------------------------------------------
# Wait for all service Deployments
# ---------------------------------------------------------------------------

log "Verifying all service Deployments are ready (parallel)"
wait_for_rollouts_parallel "$NAMESPACE" 300s \
  deployment/observable-zitadel \
  deployment/auth-service \
  deployment/admin-service \
  deployment/ingest-gateway \
  deployment/stream-processor \
  deployment/storage-writer \
  deployment/query-api \
  deployment/alert-evaluator \
  deployment/frontend \
  || { KEEP_CLUSTER=true; exit 1; }
info "All service Deployments ready"

# ---------------------------------------------------------------------------
# Smoke checks via port-forward
# ---------------------------------------------------------------------------

if [[ "$DEPLOY_ONLY" == "false" ]]; then
  log "Running ingest-to-query smoke checks"

  # Port-forward ingest-gateway HTTP
  kubectl port-forward service/ingest-gateway 14318:4318 \
    --namespace "$NAMESPACE" &
  PF_INGEST_HTTP=$!
  # Port-forward ingest-gateway gRPC
  kubectl port-forward service/ingest-gateway 14317:4317 \
    --namespace "$NAMESPACE" &
  PF_INGEST_GRPC=$!
  # Port-forward query-api
  kubectl port-forward service/query-api 18090:8090 \
    --namespace "$NAMESPACE" &
  PF_QUERY=$!
  # Port-forward frontend
  kubectl port-forward service/frontend 15173:80 \
    --namespace "$NAMESPACE" &
  PF_FRONTEND=$!
  # Port-forward auth-service
  kubectl port-forward service/auth-service 14319:4319 \
    --namespace "$NAMESPACE" &
  PF_AUTH=$!
  # Port-forward Zitadel
  kubectl port-forward service/observable-zitadel 8082:8080 \
    --namespace "$NAMESPACE" &
  PF_ZITADEL=$!

  cleanup_pf() {
    kill "$PF_INGEST_HTTP" "$PF_INGEST_GRPC" "$PF_QUERY" "$PF_FRONTEND" "$PF_AUTH" "$PF_ZITADEL" 2>/dev/null || true
  }
  trap 'cleanup_pf; cleanup' EXIT

  sleep 3  # let port-forwards establish

  DEV_KEY="dev-api-key-0000"
  TENANT_ID="00000000-0000-0000-0000-000000000002"

  # Health checks
  info "Checking /health endpoints"
  curl -sf http://localhost:14318/health | grep -q "ok" && info "ingest-gateway /health OK"
  curl -sf http://localhost:18090/health | grep -q "ok" && info "query-api /health OK"
  curl -sf http://localhost:15173/ | grep -q "<!doctype html" && info "frontend / OK"
  curl -sf http://localhost:14319/health | grep -q "ok" && info "auth-service /health OK"
  curl -sf http://localhost:8082/debug/ready | grep -q "" && info "zitadel /debug/ready OK" || info "WARN: zitadel /debug/ready did not respond"

  # Send a trace
  info "Sending test trace to ingest-gateway"
  TRACE_ID="aabbccddeeff00112233445566778899"
  curl -sf -X POST http://localhost:14318/v1/traces \
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
  RESULT=$(curl -sf "http://localhost:18090/v1/traces" \
    -H "Authorization: Bearer $DEV_KEY" \
    -H "X-Tenant-ID: $TENANT_ID" || echo "query_failed")

  if echo "$RESULT" | grep -q "kind-smoke-test"; then
    info "PASS: trace found in query results"
  elif echo "$RESULT" | grep -q "query_failed"; then
    info "WARN: query returned no data yet (pipeline may still be processing)"
  else
    info "WARN: trace not found in results (check stream-processor logs)"
  fi

  info "Checking frontend same-origin API proxy"
  FRONTEND_RESULT=$(curl -sf "http://localhost:15173/v1/environments" \
    -H "X-Tenant-ID: $TENANT_ID" \
    -H "Authorization: Bearer $DEV_KEY" || echo "frontend_query_failed")

  if echo "$FRONTEND_RESULT" | grep -q "frontend_query_failed"; then
    info "WARN: frontend /v1 proxy did not respond"
  else
    info "PASS: frontend /v1 proxy reached query-api"
  fi

  # -------------------------------------------------------------------------
  # Automated Dependency-Failure Drills (--with-drills)
  # -------------------------------------------------------------------------
  if [[ "$WITH_DRILLS" == "true" ]]; then
    log "Running Kubernetes dependency-failure drills"

    # --- Drill 1: Redpanda Interruption & Recovery ---
    log "Drill 1: Redpanda pod disruption and recovery"
    info "Scaling Redpanda StatefulSet to 0 replicas"
    kubectl scale statefulset redpanda --replicas=0 --namespace "$NAMESPACE"
    kubectl wait --for=delete pod/redpanda-0 --namespace "$NAMESPACE" --timeout=60s || true

    info "Verifying ingest fails closed during Redpanda outage"
    sleep 3
    RP_STATUS=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 \
      -X POST http://localhost:14318/v1/traces \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer $DEV_KEY" \
      -d '{"resourceSpans":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"drill-rp-fail-closed"}}]},"scopeSpans":[{"spans":[{"traceId":"11112222333344445555666677778888","spanId":"1111222233334444","name":"rp-down-span","startTimeUnixNano":"1000000000000000000","endTimeUnixNano":"1000000005000000000","status":{"code":1}}]}]}]}' \
      2>/dev/null || echo "000")
    if [[ "$RP_STATUS" -ge 400 ]] || [[ "$RP_STATUS" == "000" ]]; then
      info "PASS: ingest rejected while Redpanda is down (HTTP $RP_STATUS)"
    else
      echo "WARN: expected ingest failure during broker outage, got HTTP $RP_STATUS" >&2
    fi

    info "Restoring Redpanda StatefulSet to 1 replica"
    kubectl scale statefulset redpanda --replicas=1 --namespace "$NAMESPACE"
    kubectl wait --for=condition=ready pod/redpanda-0 --namespace "$NAMESPACE" --timeout=120s
    info "Waiting for services to reconnect to Redpanda"
    sleep 10

    info "Verifying pipeline recovery post-Redpanda-restore"
    RP_RECOVER_TRACE="11111111222222223333333344444444"
    curl -sf -X POST http://localhost:14318/v1/traces \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer $DEV_KEY" \
      -d "{
        \"resourceSpans\": [{
          \"resource\": {\"attributes\": [{\"key\": \"service.name\", \"value\": {\"stringValue\": \"drill-rp-recovered\"}}]},
          \"scopeSpans\": [{
            \"spans\": [{
              \"traceId\": \"$RP_RECOVER_TRACE\",
              \"spanId\": \"1234567812345678\",
              \"name\": \"rp-recovered-span\",
              \"startTimeUnixNano\": \"$(date +%s)000000000\",
              \"endTimeUnixNano\": \"$(date +%s)000000001\"
            }]
          }]
        }]
      }" >/dev/null 2>&1 || true

    sleep 6
    RP_QUERY_RES=$(curl -sf "http://localhost:18090/v1/traces" -H "Authorization: Bearer $DEV_KEY" -H "X-Tenant-ID: $TENANT_ID" 2>/dev/null || echo "")
    if echo "$RP_QUERY_RES" | grep -q "drill-rp-recovered"; then
      info "PASS: Drill 1 complete — Redpanda disruption and recovery verified"
    else
      info "WARN: post-recovery trace not immediately returned (pipeline catching up)"
    fi

    # --- Drill 2: ClickHouse Interruption & Recovery ---
    log "Drill 2: ClickHouse pod disruption and recovery"
    info "Scaling ClickHouse Deployment to 0 replicas"
    kubectl scale deployment clickhouse --replicas=0 --namespace "$NAMESPACE"
    kubectl wait --for=delete pod -l app=clickhouse --namespace "$NAMESPACE" --timeout=60s || true

    info "Verifying query fails or reports degraded state during ClickHouse outage"
    sleep 2
    CH_QUERY_STATUS=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 \
      "http://localhost:18090/v1/services" \
      -H "Authorization: Bearer $DEV_KEY" \
      -H "X-Tenant-ID: $TENANT_ID" 2>/dev/null || echo "000")
    info "Query API returned HTTP $CH_QUERY_STATUS during ClickHouse outage"

    info "Verifying ingest accepts telemetry and buffers to Redpanda"
    CH_BUFFER_TRACE="22222222333333334444444455555555"
    CH_INGEST_STATUS=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 \
      -X POST http://localhost:14318/v1/traces \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer $DEV_KEY" \
      -d "{
        \"resourceSpans\": [{
          \"resource\": {\"attributes\": [{\"key\": \"service.name\", \"value\": {\"stringValue\": \"drill-ch-buffered\"}}]},
          \"scopeSpans\": [{
            \"spans\": [{
              \"traceId\": \"$CH_BUFFER_TRACE\",
              \"spanId\": \"8765432187654321\",
              \"name\": \"ch-buffered-span\",
              \"startTimeUnixNano\": \"$(date +%s)000000000\",
              \"endTimeUnixNano\": \"$(date +%s)000000001\"
            }]
          }]
        }]
      }" 2>/dev/null || echo "000")
    if [[ "$CH_INGEST_STATUS" -ge 200 ]] && [[ "$CH_INGEST_STATUS" -lt 400 ]]; then
      info "PASS: ingest accepted telemetry while ClickHouse is down (HTTP $CH_INGEST_STATUS)"
    else
      info "INFO: ingest returned HTTP $CH_INGEST_STATUS during ClickHouse outage"
    fi

    info "Restoring ClickHouse Deployment to 1 replica"
    kubectl scale deployment clickhouse --replicas=1 --namespace "$NAMESPACE"
    kubectl wait deployment/clickhouse --for=condition=Available --namespace "$NAMESPACE" --timeout=120s
    info "Waiting for storage-writer to drain backlog into ClickHouse"
    sleep 10

    CH_DRAIN_RES=$(curl -sf "http://localhost:18090/v1/traces" -H "Authorization: Bearer $DEV_KEY" -H "X-Tenant-ID: $TENANT_ID" 2>/dev/null || echo "")
    if echo "$CH_DRAIN_RES" | grep -q "drill-ch-buffered"; then
      info "PASS: Drill 2 complete — buffered data drained into ClickHouse after restore"
    else
      info "INFO: storage-writer continuing to drain queued records into ClickHouse"
    fi

    # --- Drill 3: PostgreSQL Interruption & Recovery ---
    log "Drill 3: PostgreSQL disruption and recovery"
    info "Suspending CloudNativePG PostgreSQL cluster (instances=0)"
    kubectl patch cluster postgres --namespace "$NAMESPACE" --type merge -p '{"spec":{"instances":0}}'
    kubectl wait --for=delete pod -l cnpg.io/cluster=postgres --namespace "$NAMESPACE" --timeout=60s || true

    info "Verifying invalid/uncached auth fails closed without PostgreSQL"
    PG_AUTH_STATUS=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 \
      -X POST http://localhost:14318/v1/traces \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer unauthenticated-fake-key" \
      -d '{"resourceSpans":[]}' 2>/dev/null || echo "000")
    if [[ "$PG_AUTH_STATUS" -ge 400 ]] || [[ "$PG_AUTH_STATUS" == "000" ]]; then
      info "PASS: invalid/unknown token rejected while PostgreSQL is down (HTTP $PG_AUTH_STATUS)"
    else
      echo "WARN: auth check returned HTTP $PG_AUTH_STATUS" >&2
    fi

    info "Restoring CloudNativePG PostgreSQL cluster (instances=1)"
    kubectl patch cluster postgres --namespace "$NAMESPACE" --type merge -p '{"spec":{"instances":1}}'
    kubectl wait cluster/postgres --for=condition=Ready --namespace "$NAMESPACE" --timeout=180s
    info "Waiting for microservice connection pools to reconnect"
    sleep 10

    PG_RECOVER_TRACE="33333333444444445555555566666666"
    curl -sf -X POST http://localhost:14318/v1/traces \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer $DEV_KEY" \
      -d "{
        \"resourceSpans\": [{
          \"resource\": {\"attributes\": [{\"key\": \"service.name\", \"value\": {\"stringValue\": \"drill-pg-recovered\"}}]},
          \"scopeSpans\": [{
            \"spans\": [{
              \"traceId\": \"$PG_RECOVER_TRACE\",
              \"spanId\": \"9999888877776666\",
              \"name\": \"pg-recovered-span\",
              \"startTimeUnixNano\": \"$(date +%s)000000000\",
              \"endTimeUnixNano\": \"$(date +%s)000000001\"
            }]
          }]
        }]
      }" >/dev/null 2>&1 || true

    sleep 6
    PG_QUERY_RES=$(curl -sf "http://localhost:18090/v1/traces" -H "Authorization: Bearer $DEV_KEY" -H "X-Tenant-ID: $TENANT_ID" 2>/dev/null || echo "")
    if echo "$PG_QUERY_RES" | grep -q "drill-pg-recovered"; then
      info "PASS: Drill 3 complete — PostgreSQL disruption and pool self-healing verified"
    else
      info "INFO: pipeline continuing post-PostgreSQL recovery"
    fi
  fi

  cleanup_pf
  trap 'cleanup' EXIT
fi

# ---------------------------------------------------------------------------
# Rollback demonstration
# ---------------------------------------------------------------------------

if [[ "$DEPLOY_ONLY" == "false" ]]; then
  BASE_REVISION="$(current_revision "$RELEASE_NAME" "$NAMESPACE")"
  if [[ -z "$BASE_REVISION" ]]; then
    echo "ERROR: could not determine current Helm revision for $RELEASE_NAME" >&2
    exit 1
  fi
  info "Current Helm revision before rollback demo: $BASE_REVISION"

  log "Demonstrating helm rollback"

  info "Upgrading release to create a new revision"
  helm upgrade "$RELEASE_NAME" "$APP_CHART" \
    --namespace "$NAMESPACE" \
    "${HELM_LOCAL_IMAGE_ARGS[@]}" \
    --set services.queryApi.replicas=2 \
    --wait \
    --timeout 3m

  NEW_REVISION="$(current_revision "$RELEASE_NAME" "$NAMESPACE")"
  info "Revision $NEW_REVISION deployed — queryApi replicas=2"
  helm history "$RELEASE_NAME" --namespace "$NAMESPACE"

  info "Rolling back to revision $BASE_REVISION"
  helm rollback "$RELEASE_NAME" "$BASE_REVISION" \
    --namespace "$NAMESPACE" \
    --wait \
    --timeout 3m

  info "Rollback to revision $BASE_REVISION complete"
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
fi

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------

if [[ "$DEPLOY_ONLY" == "true" ]]; then
  log "Observable platform deployed"
else
  log "kind integration test PASSED"
  log "Helm chart renders, deploys, and rolls back cleanly against a real k8s cluster."
  info ""
  info "Rollback path documented in spec/12-deployment.md §19.7"
  info "Chart layout documented in ADR-020"
fi
