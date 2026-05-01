#!/usr/bin/env bash
# Hot-reload one or more Observable services into a running kind testbench cluster
# without recreating the cluster.
#
# Typical development loop:
#
#   # One-time setup — cluster keeps running after Ctrl+C:
#   bash scripts/testbench.sh --keep-cluster
#
#   # After each code change (seconds, not minutes):
#   bash scripts/dev-reload.sh --service query-api
#   bash scripts/dev-reload.sh --service frontend --run-tests
#
# How it works:
#   1. Build the Docker image for the changed service (layer-cached, fast).
#   2. Load the image into the kind cluster's containerd store.
#   3. Rolling-restart the affected Deployment(s) so new pods pick up the image.
#   4. Wait for rollout to finish.
#
# Because all Observable images use imagePullPolicy=Never, no registry is needed.
# kind load updates the tag→digest mapping in the node's containerd store, so
# pods started after a reload always use the freshly built image.
#
# Usage:
#   bash scripts/dev-reload.sh [options]
#
# Options:
#   --service <name>     Which service(s) to rebuild and redeploy.
#                        Default: all
#
#                        Rust backend (share observable-services:local image):
#                          query-api, stream-processor, ingest-gateway,
#                          storage-writer, auth-service, alert-evaluator
#                        Shorthand for all Rust services:
#                          backend
#                        Frontend:
#                          frontend
#                        Testbench services:
#                          testbench-api, testbench-frontend,
#                          testbench-worker, testbench-loadgen
#                        Shorthand for all testbench services:
#                          testbench
#                        Rebuild and redeploy everything:
#                          all  (default)
#
#   --skip-build         Skip docker build; load the existing local image and
#                        restart deployments. Useful when the image is already
#                        up to date (e.g. only config changed).
#
#   --cluster-name <n>   kind cluster name (default: observable-test)
#
#   --run-tests          Run unit tests after reload:
#                          Rust service changed → cargo test --workspace
#                          Frontend changed     → npm run test run -w apps/frontend
#
#   --smoke              Run the smoke test via kind-test.sh after reload.
#                        Requires the cluster to be fully up.
#
# Prerequisites: kind, kubectl, docker (+ npm/cargo when --run-tests is used)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

CLUSTER_NAME="observable-test"
OBSERVABLE_NS="observable"
TESTBENCH_NS="testbench"
SERVICE="all"
SKIP_BUILD=false
RUN_TESTS=false
RUN_SMOKE=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --service)
      if [[ $# -lt 2 ]]; then
        echo "ERROR: --service requires a value." >&2; exit 1
      fi
      SERVICE="$2"; shift 2 ;;
    --skip-build)   SKIP_BUILD=true;  shift ;;
    --cluster-name)
      if [[ $# -lt 2 ]]; then
        echo "ERROR: --cluster-name requires a value." >&2; exit 1
      fi
      CLUSTER_NAME="$2"; shift 2 ;;
    --run-tests)    RUN_TESTS=true;   shift ;;
    --smoke)        RUN_SMOKE=true;   shift ;;
    *)
      echo "ERROR: unknown argument: $1" >&2
      echo "       Run 'bash scripts/dev-reload.sh --help' for usage." >&2
      exit 1 ;;
  esac
done

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

log()  { echo ""; echo "==> [$(date +%H:%M:%S)] $*"; }
info() { echo "    $*"; }
ok()   { echo "    OK  $*"; }
warn() { echo "    WARN $*"; }

# ---------------------------------------------------------------------------
# Validate cluster is running
# ---------------------------------------------------------------------------

log "Checking kind cluster '$CLUSTER_NAME'"
if ! kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
  echo "" >&2
  echo "ERROR: kind cluster '$CLUSTER_NAME' is not running." >&2
  echo "" >&2
  echo "Start it first with:" >&2
  echo "  bash scripts/testbench.sh --keep-cluster" >&2
  echo "" >&2
  exit 1
fi
ok "cluster '$CLUSTER_NAME' is running"

# ---------------------------------------------------------------------------
# Resolve which images/deployments to reload
# ---------------------------------------------------------------------------

# Each entry: "docker_tag:image_context:namespace:deployment[,deployment,...]"
RELOAD_TARGETS=()

rust_backend_target() {
  echo "observable-services:local|${REPO_ROOT}|${OBSERVABLE_NS}|query-api,stream-processor,ingest-gateway,storage-writer,auth-service,alert-evaluator"
}

rust_single_target() {
  # $1 = deployment name
  echo "observable-services:local|${REPO_ROOT}|${OBSERVABLE_NS}|$1"
}

case "$SERVICE" in
  all)
    RELOAD_TARGETS=(
      "$(rust_backend_target)"
      "observable-frontend:local|${REPO_ROOT}/apps/frontend|${OBSERVABLE_NS}|frontend"
      "testbench-api:local|${REPO_ROOT}/testbench/api|${TESTBENCH_NS}|shop-api"
      "testbench-frontend:local|${REPO_ROOT}/testbench/frontend|${TESTBENCH_NS}|shop-frontend"
      "testbench-worker:local|${REPO_ROOT}/testbench/worker|${TESTBENCH_NS}|shop-worker"
      "testbench-loadgen:local|${REPO_ROOT}/testbench/loadgen|${TESTBENCH_NS}|shop-loadgen"
    ) ;;
  backend)
    RELOAD_TARGETS=("$(rust_backend_target)") ;;
  query-api|stream-processor|ingest-gateway|storage-writer|auth-service|alert-evaluator)
    RELOAD_TARGETS=("$(rust_single_target "$SERVICE")") ;;
  frontend)
    RELOAD_TARGETS=("observable-frontend:local|${REPO_ROOT}/apps/frontend|${OBSERVABLE_NS}|frontend") ;;
  testbench)
    RELOAD_TARGETS=(
      "testbench-api:local|${REPO_ROOT}/testbench/api|${TESTBENCH_NS}|shop-api"
      "testbench-frontend:local|${REPO_ROOT}/testbench/frontend|${TESTBENCH_NS}|shop-frontend"
      "testbench-worker:local|${REPO_ROOT}/testbench/worker|${TESTBENCH_NS}|shop-worker"
      "testbench-loadgen:local|${REPO_ROOT}/testbench/loadgen|${TESTBENCH_NS}|shop-loadgen"
    ) ;;
  testbench-api)
    RELOAD_TARGETS=("testbench-api:local|${REPO_ROOT}/testbench/api|${TESTBENCH_NS}|shop-api") ;;
  testbench-frontend)
    RELOAD_TARGETS=("testbench-frontend:local|${REPO_ROOT}/testbench/frontend|${TESTBENCH_NS}|shop-frontend") ;;
  testbench-worker)
    RELOAD_TARGETS=("testbench-worker:local|${REPO_ROOT}/testbench/worker|${TESTBENCH_NS}|shop-worker") ;;
  testbench-loadgen)
    RELOAD_TARGETS=("testbench-loadgen:local|${REPO_ROOT}/testbench/loadgen|${TESTBENCH_NS}|shop-loadgen") ;;
  *)
    echo "ERROR: unknown service '$SERVICE'." >&2
    echo "       Valid values: all, backend, query-api, stream-processor, ingest-gateway," >&2
    echo "       storage-writer, auth-service, alert-evaluator, frontend," >&2
    echo "       testbench, testbench-api, testbench-frontend, testbench-worker, testbench-loadgen" >&2
    exit 1 ;;
esac

# ---------------------------------------------------------------------------
# Build → load → restart each target
# ---------------------------------------------------------------------------

NEED_RUST_TESTS=false
NEED_FRONTEND_TESTS=false

# Track which images have already been loaded (the observable-services image is
# shared by multiple single-service targets; we only need to build+load it once).
declare -A LOADED_IMAGES=()

for target in "${RELOAD_TARGETS[@]}"; do
  IFS='|' read -r img img_context ns deployments_csv <<< "$target"

  # Build
  if [[ "$SKIP_BUILD" == "false" && -z "${LOADED_IMAGES[$img]:-}" ]]; then
    log "Building $img"
    info "context: $img_context"
    docker build --tag "$img" "$img_context"
    ok "$img built"
  elif [[ "$SKIP_BUILD" == "true" ]]; then
    info "Skipping build for $img (--skip-build)"
  fi

  # Load into kind (once per unique image)
  if [[ -z "${LOADED_IMAGES[$img]:-}" ]]; then
    log "Loading $img into cluster '$CLUSTER_NAME'"
    kind load docker-image "$img" --name "$CLUSTER_NAME"
    ok "$img loaded"
    LOADED_IMAGES[$img]=1
  fi

  # Track which test suites are affected
  case "$img" in
    observable-services:local|observable-frontend:local)
      if [[ "$img" == "observable-frontend:local" ]]; then
        NEED_FRONTEND_TESTS=true
      else
        NEED_RUST_TESTS=true
      fi ;;
  esac

  # Rolling restart each deployment
  IFS=',' read -ra deployments <<< "$deployments_csv"
  for dep in "${deployments[@]}"; do
    log "Restarting deployment/$dep (ns: $ns)"
    if kubectl get deployment "$dep" --namespace "$ns" &>/dev/null; then
      kubectl rollout restart "deployment/$dep" --namespace "$ns"
      kubectl rollout status "deployment/$dep" --namespace "$ns" --timeout 120s
      ok "deployment/$dep ready"
    else
      warn "deployment/$dep not found in ns=$ns — skipping"
    fi
  done
done

# ---------------------------------------------------------------------------
# Run unit tests (optional)
# ---------------------------------------------------------------------------

if [[ "$RUN_TESTS" == "true" ]]; then
  if [[ "$NEED_RUST_TESTS" == "true" ]]; then
    log "Running Rust unit tests"
    cd "$REPO_ROOT"
    cargo test --workspace
    ok "Rust tests passed"
  fi

  if [[ "$NEED_FRONTEND_TESTS" == "true" ]]; then
    log "Running frontend unit tests"
    cd "$REPO_ROOT"
    npm run test run --workspace=apps/frontend
    ok "Frontend tests passed"
  fi

  # Always run for testbench changes too (no dedicated tests, skip silently)
fi

# ---------------------------------------------------------------------------
# Smoke test (optional)
# ---------------------------------------------------------------------------

if [[ "$RUN_SMOKE" == "true" ]]; then
  log "Running smoke test"
  bash "$SCRIPT_DIR/kind-test.sh" \
    --skip-build \
    --keep-cluster \
    --reuse-cluster \
    --cluster-name "$CLUSTER_NAME" \
    --deploy-only
  ok "Smoke test passed"
fi

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------

log "Reload complete"
info ""
info "  Observable frontend:  http://localhost:8080/"
info "  Testbench shop:       http://localhost:3000/"
info ""
info "Useful debug commands:"
info "  kubectl logs -f -n ${OBSERVABLE_NS} deploy/query-api"
info "  kubectl logs -f -n ${TESTBENCH_NS} deploy/shop-loadgen"
info ""
