# Testbench Gateway API Exposure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite `scripts/testbench.sh` to expose the Observable frontend (`http://localhost:8080/`) and testbench shop-frontend (`http://localhost:3000/`) via the Kubernetes Gateway API (nginx-gateway-fabric), and block indefinitely instead of exiting so the cluster stays up for manual testing.

**Architecture:** A single nginx-gateway-fabric instance handles both UIs via a two-listener `Gateway`. kind `extraPortMappings` forward host ports 8080 and 3000 to fixed NodePorts (30080, 30300) on the cluster node. After all deploys succeed the script enters an idle loop until Ctrl+C, which tears down the cluster.

**Tech Stack:** Bash, kind, kubectl, helm, Kubernetes Gateway API v1.2.1, nginx-gateway-fabric chart 1.5.1, jq (new prereq)

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `scripts/testbench-kind-config.yaml` | Create | kind cluster config with two `extraPortMappings` |
| `scripts/testbench.sh` | Rewrite | Orchestrate full testbench lifecycle: build → cluster → deploy Observable → deploy testbench → Gateway API → idle loop |
| `scripts/kind-test.sh` | Modify | Add `--deploy-only` flag that skips smoke checks and rollback demo |
| `spec/19-testbench.md` | Modify | Update access URLs and deployment model description |

---

## Task 1: Create kind cluster config

**Files:**
- Create: `scripts/testbench-kind-config.yaml`

- [ ] **Create the file**

```yaml
# Kind cluster configuration for the Observable test bench.
#
# extraPortMappings forward host ports to the nginx-gateway-fabric NodePorts so
# both UIs are reachable from the host without kubectl port-forward:
#
#   http://localhost:8080/  → Observable frontend   (NodePort 30080)
#   http://localhost:3000/  → Testbench shop         (NodePort 30300)
kind: Cluster
apiVersion: kind.x-k8s.io/v1alpha4
nodes:
  - role: control-plane
    extraPortMappings:
      - containerPort: 30080
        hostPort: 8080
        protocol: TCP
      - containerPort: 30300
        hostPort: 3000
        protocol: TCP
```

- [ ] **Verify syntax**

```bash
kind build node-image --help >/dev/null   # just confirms kind is on PATH
cat scripts/testbench-kind-config.yaml    # visual check
```

- [ ] **Commit**

```bash
git add scripts/testbench-kind-config.yaml
git commit -m "feat(testbench): add kind cluster config with Gateway API port mappings"
```

---

## Task 2: Add `--deploy-only` to kind-test.sh

**Files:**
- Modify: `scripts/kind-test.sh`

`--deploy-only` makes `kind-test.sh` skip the ingest-to-query smoke checks (port-forward block, lines ~340–425) and the Helm rollback demonstration (lines ~430–467). The deploy and wait-for-rollout sections are unchanged. This is the only caller change needed so `testbench.sh` can use `kind-test.sh` as a pure deploy helper.

- [ ] **Add `DEPLOY_ONLY` variable after the existing variable declarations (around line 46)**

Current block ends with:
```bash
REUSE_CLUSTER=false
```

Add after it:
```bash
DEPLOY_ONLY=false
```

- [ ] **Add `--deploy-only` case to the argument-parsing `while` loop (around line 60, before the `*` catch-all)**

```bash
    --deploy-only)
      DEPLOY_ONLY=true
      shift
      ;;
```

- [ ] **Wrap the smoke-check block in a guard (lines ~336–425)**

Find the line:
```bash
log "Running ingest-to-query smoke checks"
```

Wrap everything from that line through `trap 'cleanup' EXIT` (the line after `cleanup_pf`) in:
```bash
if [[ "$DEPLOY_ONLY" == "false" ]]; then
  log "Running ingest-to-query smoke checks"
  # ... (existing smoke-check content unchanged) ...
  cleanup_pf
  trap 'cleanup' EXIT
fi
```

- [ ] **Wrap the rollback-demo block in the same guard (lines ~427–467)**

Find the line:
```bash
log "Demonstrating helm rollback"
```

Wrap everything from that line through the replica count check in:
```bash
if [[ "$DEPLOY_ONLY" == "false" ]]; then
  log "Demonstrating helm rollback"
  # ... (existing rollback content unchanged) ...
fi
```

- [ ] **Update the done message (lines ~470–477)**

Replace:
```bash
log "kind integration test PASSED"
log "Helm chart renders, deploys, and rolls back cleanly against a real k8s cluster."
info ""
info "Rollback path documented in spec/12-deployment.md §19.7"
info "Chart layout documented in ADR-020"
```

With:
```bash
if [[ "$DEPLOY_ONLY" == "true" ]]; then
  log "Observable platform deployed"
else
  log "kind integration test PASSED"
  log "Helm chart renders, deploys, and rolls back cleanly against a real k8s cluster."
  info ""
  info "Rollback path documented in spec/12-deployment.md §19.7"
  info "Chart layout documented in ADR-020"
fi
```

- [ ] **Smoke-test the flag parses correctly**

```bash
bash -n scripts/kind-test.sh && echo "syntax OK"
bash scripts/kind-test.sh --help 2>&1 | head -5 || true
```

Expected: `syntax OK` (the `--help` will error with "unknown argument" — that is fine, confirms the parser runs).

- [ ] **Commit**

```bash
git add scripts/kind-test.sh
git commit -m "feat(testbench): add --deploy-only flag to kind-test.sh"
```

---

## Task 3: Rewrite testbench.sh — header, variables, and CLI flags

**Files:**
- Modify: `scripts/testbench.sh`

Replace the entire file with the new version. This task covers the header comment, variable declarations, and argument-parsing section. Tasks 4–6 cover the functional sections.

- [ ] **Replace the file header and variable block (lines 1–65)**

```bash
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
    --skip-build)      SKIP_BUILD=true;      shift ;;
    --keep-cluster)    KEEP_CLUSTER=true;    shift ;;
    --recreate)        RECREATE_CLUSTER=true; shift ;;
    --skip-observable) SKIP_OBSERVABLE=true; shift ;;
    --observable-ns)
      if [[ $# -lt 2 ]]; then
        echo "ERROR: --observable-ns requires a namespace value." >&2; exit 1
      fi
      OBSERVABLE_NS="$2"; shift 2 ;;
    *)
      echo "ERROR: unknown argument: $1" >&2; exit 1 ;;
  esac
done
```

- [ ] **Verify syntax so far**

```bash
bash -n scripts/testbench.sh && echo "syntax OK"
```

Expected: `syntax OK`

---

## Task 4: Rewrite testbench.sh — helpers, prereqs, build, cluster, deploys

**Files:**
- Modify: `scripts/testbench.sh`

Add the helpers block, prereq check, build section, cluster section, and Observable + testbench deploy sections. Append these after the variable block from Task 3.

- [ ] **Append helpers + prereq check**

```bash
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

wait_for_rollout() {
  local resource="$1" ns="${2:-$TESTBENCH_NS}" timeout="${3:-180s}"
  info "waiting for $resource in ns=$ns (timeout: $timeout)"
  kubectl rollout status "$resource" --namespace "$ns" --timeout "$timeout" \
    || { info "FAILED: $resource did not become ready"; dump_pod_events "$ns"; exit 1; }
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
  log "Building testbench Docker images"
  for entry in "${TESTBENCH_IMAGES[@]}"; do
    tag="${entry%:*}"
    context="${entry##*:}"
    info "Building $tag from $context"
    docker build --tag "$tag" "$context"
  done
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

log "Loading testbench images into kind cluster"
for entry in "${TESTBENCH_IMAGES[@]}"; do
  img="${entry%:*}"
  info "Loading $img"
  kind load docker-image "$img" --name "$CLUSTER_NAME"
done

log "Creating testbench namespace"
kubectl create namespace "$TESTBENCH_NS" --dry-run=client -o yaml | kubectl apply -f -

reset_stale_release "$TESTBENCH_RELEASE" "$TESTBENCH_NS"

log "Installing observable-testbench chart"
helm upgrade --install "$TESTBENCH_RELEASE" "$TESTBENCH_CHART" \
  --namespace "$TESTBENCH_NS" \
  --wait \
  --timeout 10m \
  || { dump_pod_events "$TESTBENCH_NS"; exit 1; }

show_pods "$TESTBENCH_NS"

log "Waiting for testbench Deployments"
for svc in otel-collector-gateway shop-api shop-frontend shop-loadgen shop-worker; do
  wait_for_rollout "deployment/$svc" "$TESTBENCH_NS"
done

log "Waiting for otel-collector-agent DaemonSet"
kubectl rollout status daemonset/otel-collector-agent \
  --namespace "$TESTBENCH_NS" --timeout 120s \
  || info "WARN: agent DaemonSet not fully ready"
```

- [ ] **Verify syntax**

```bash
bash -n scripts/testbench.sh && echo "syntax OK"
```

Expected: `syntax OK`

---

## Task 5: Rewrite testbench.sh — Gateway API install, routes, patch, smoke, idle loop

**Files:**
- Modify: `scripts/testbench.sh`

Append the Gateway API sections and the idle loop at the end of the file.

- [ ] **Append Gateway API CRDs + nginx-gateway-fabric install**

```bash
# ---------------------------------------------------------------------------
# Install Kubernetes Gateway API CRDs
# ---------------------------------------------------------------------------

log "Installing Kubernetes Gateway API CRDs (${GATEWAY_API_VERSION})"
kubectl apply -f \
  "https://github.com/kubernetes-sigs/gateway-api/releases/download/${GATEWAY_API_VERSION}/standard-install.yaml"

# CRDs must be established before nginx-gateway-fabric starts
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
  --set "service.nodePorts.http=${GATEWAY_NODEPORT_OBSERVABLE}" \
  --wait --timeout 5m
```

- [ ] **Append Gateway and HTTPRoute resources**

```bash
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
```

- [ ] **Append NodePort patch for the shop listener**

nginx-gateway-fabric adds port 3000 to its Service after the Gateway is created. Wait for it to appear, then patch the NodePort to the fixed value that kind is configured to forward.

```bash
# ---------------------------------------------------------------------------
# Patch nginx-gateway-fabric Service — pin shop NodePort to GATEWAY_NODEPORT_SHOP
# ---------------------------------------------------------------------------

log "Waiting for nginx-gateway-fabric Service to expose shop port (3000)"
NGF_SVC="${NGF_RELEASE}-nginx-gateway-fabric"
for i in $(seq 1 24); do
  PORT_COUNT=$(kubectl get service "$NGF_SVC" -n "$NGF_NAMESPACE" -o json \
    | jq '[.spec.ports[] | select(.port == 3000)] | length')
  if [[ "$PORT_COUNT" -ge 1 ]]; then
    info "port 3000 found on Service after ${i}x5s"
    break
  fi
  info "  [${i}/24] waiting for port 3000 on Service..."
  sleep 5
done

PORT_COUNT=$(kubectl get service "$NGF_SVC" -n "$NGF_NAMESPACE" -o json \
  | jq '[.spec.ports[] | select(.port == 3000)] | length')
if [[ "$PORT_COUNT" -lt 1 ]]; then
  echo "ERROR: port 3000 never appeared on Service $NGF_SVC — check nginx-gateway-fabric logs" >&2
  kubectl logs -n "$NGF_NAMESPACE" deploy/"$NGF_RELEASE"-nginx-gateway-fabric 2>/dev/null | tail -30 || true
  exit 1
fi

log "Patching shop NodePort to ${GATEWAY_NODEPORT_SHOP}"
PATCH=$(kubectl get service "$NGF_SVC" -n "$NGF_NAMESPACE" -o json \
  | jq --argjson np "${GATEWAY_NODEPORT_SHOP}" \
       '[.spec.ports | to_entries[]
        | select(.value.port == 3000)
        | {"op": "replace",
           "path": "/spec/ports/\(.key)/nodePort",
           "value": $np}]')
kubectl patch service "$NGF_SVC" -n "$NGF_NAMESPACE" --type=json -p="$PATCH"
info "NodePort ${GATEWAY_NODEPORT_SHOP} set for port 3000"
```

- [ ] **Append gateway readiness wait + smoke check**

```bash
# ---------------------------------------------------------------------------
# Wait for Gateway to be programmed
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
```

- [ ] **Append idle loop (replaces the old "Done — print access instructions" block)**

```bash
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
```

- [ ] **Verify the complete script has no syntax errors**

```bash
bash -n scripts/testbench.sh && echo "syntax OK"
```

Expected: `syntax OK`

- [ ] **Commit**

```bash
git add scripts/testbench.sh
git commit -m "feat(testbench): expose both UIs via Gateway API, block until Ctrl+C"
```

---

## Task 6: Update spec/19-testbench.md

**Files:**
- Modify: `spec/19-testbench.md`

- [ ] **Update the Prerequisites list in the header area to add jq**

Find:
```
#   docker
```
Replace with:
```
#   docker
#   jq
```

(This comment block does not exist verbatim in the spec — locate the Prerequisites section `§19.6 Deployment Model` and update its entry-point description.)

In `spec/19-testbench.md`, locate **§19.6 Deployment Model** and update the entry-point description. Find:

```
**Entry point:** `bash scripts/testbench.sh [--skip-build] [--keep-cluster] [--observable-ns <ns>]`
```

Replace with:

```
**Entry point:** `bash scripts/testbench.sh [--skip-build] [--keep-cluster] [--recreate] [--observable-ns <ns>]`
```

- [ ] **Update the script step list in §19.6**

Find the numbered list under the entry point and replace steps 8–9:

Old:
```
8. Runs a smoke check (POST /orders → wait 15 s → query Observable)
9. Prints port-forward commands for interactive exploration
```

New:
```
8. Installs Kubernetes Gateway API CRDs and nginx-gateway-fabric
9. Applies a two-listener `Gateway` and two `HTTPRoute` resources
10. Patches the nginx-gateway-fabric Service to pin the shop NodePort
11. Runs a non-fatal smoke check against both gateway URLs
12. Blocks indefinitely — prints access URLs and waits for Ctrl+C
```

- [ ] **Update §19.7 Verification to reference gateway URLs**

Find the first block in §19.7:
```bash
# All testbench pods running
kubectl get pods -n testbench
```

Add before it:
```bash
# Both UIs reachable via Gateway API
curl -s http://localhost:8080/ | grep -i "<!doctype html"   # Observable frontend
curl -s http://localhost:3000/ | grep -i "<!doctype html"   # Testbench shop
```

- [ ] **Update the port-forward example in §19.7 for query-api**

The existing query examples use `kubectl port-forward svc/query-api 8090:8090` — leave those unchanged since query-api is not exposed via the Gateway. Add a note before the block:

```
# (Optional) Direct query-api access — bypasses the UI, useful for scripting:
```

- [ ] **Verify spec renders correctly**

```bash
# Just check it parses as valid markdown (no tool needed — eyeball the file)
head -60 spec/19-testbench.md
```

- [ ] **Commit**

```bash
git add spec/19-testbench.md
git commit -m "docs(testbench): update spec to reflect Gateway API exposure and idle-loop behaviour"
```

---

## Task 7: Manual verification checklist

This task cannot be automated — it confirms the full flow works end-to-end.

- [ ] **Run the testbench from scratch**

```bash
bash scripts/testbench.sh
```

Expected sequence of log lines (timing varies):
```
==> Checking prerequisites
==> Building testbench Docker images
==> Setting up kind cluster 'observable-test'
==> Deploying Observable platform via kind-test.sh
    Observable platform deployed
==> Loading testbench images into kind cluster
==> Installing observable-testbench chart
==> Waiting for testbench Deployments
==> Waiting for otel-collector-agent DaemonSet
==> Installing Kubernetes Gateway API CRDs (v1.2.1)
==> Installing nginx-gateway-fabric chart (1.5.1)
==> Applying Gateway and HTTPRoutes
==> Waiting for nginx-gateway-fabric Service to expose shop port (3000)
==> Patching shop NodePort to 30300
==> Waiting for Gateway to be Programmed
==> Running smoke check via Gateway
    PASS: Observable frontend reachable
    PASS: Testbench shop reachable
==> Test bench is running
    Observable frontend:  http://localhost:8080/
    Testbench shop:       http://localhost:3000/
    ...
==> Cluster running — press Ctrl+C to stop
```

- [ ] **Open both URLs in a browser**

- `http://localhost:8080/` → Observable frontend SPA loads
- `http://localhost:3000/` → Shop Testbench page with product listing

- [ ] **Verify telemetry is flowing**

```bash
kubectl logs -f -n testbench deploy/shop-loadgen
# Expected: continuous scenario lines like "scenario=browse_products"

kubectl logs -n testbench deploy/otel-collector-gateway 2>&1 | grep -i "export"
# Expected: export success lines to ingest-gateway
```

- [ ] **Verify Ctrl+C tears down the cluster**

Press Ctrl+C. Expected:
```
==> Tearing down kind cluster 'observable-test'
```
Then `kind get clusters` returns nothing.

- [ ] **Verify --keep-cluster preserves cluster**

```bash
bash scripts/testbench.sh --skip-build --keep-cluster
# Ctrl+C
kind get clusters   # should still list observable-test
kind delete cluster --name observable-test
```

- [ ] **Verify --recreate forces fresh cluster**

```bash
# First run (creates cluster)
bash scripts/testbench.sh --keep-cluster
# Ctrl+C
# Second run (reuse)
bash scripts/testbench.sh --keep-cluster
# Ctrl+C (check logs: "Cluster already exists — reusing it")
# Third run (force recreate)
bash scripts/testbench.sh --recreate --keep-cluster
# Ctrl+C (check logs: "Deleting existing cluster for fresh start")
kind delete cluster --name observable-test
```

- [ ] **Verify kind-test.sh local-CI path is unchanged**

```bash
# This should still run smoke checks AND rollback demo (no --deploy-only)
bash scripts/kind-test.sh --keep-cluster 2>&1 | grep -E "smoke|rollback|PASSED"
# Expected: lines containing "smoke checks", "rollback", "integration test PASSED"
kind delete cluster --name observable-test
```

---

## Self-Review

**Spec coverage:**
- ✅ kind cluster with extraPortMappings — Task 1
- ✅ Gateway API CRDs + nginx-gateway-fabric — Task 5
- ✅ Observable frontend at localhost:8080 via HTTPRoute — Task 5
- ✅ Shop frontend at localhost:3000 via HTTPRoute — Task 5
- ✅ NodePort patch using jq (port-by-number, not by index) — Task 5
- ✅ Idle loop until Ctrl+C — Task 5
- ✅ `--recreate` flag — Task 3 (replaces old silent reuse)
- ✅ `--deploy-only` in kind-test.sh — Task 2
- ✅ spec/19-testbench.md updated — Task 6
- ✅ jq added as prereq — Tasks 3 & 6

**Version pins in script:**
- `GATEWAY_API_VERSION="v1.2.1"` — check https://github.com/kubernetes-sigs/gateway-api/releases before running
- `NGF_CHART_VERSION="1.5.1"` — check https://github.com/nginx/nginx-gateway-fabric/releases before running

**Potential fragility:**
- nginx-gateway-fabric Service name is `${NGF_RELEASE}-nginx-gateway-fabric` (`ngf-nginx-gateway-fabric`). If the chart's `fullnameOverride` or naming convention changes in a newer version, this will break. The polling loop will time out with a clear error message pointing to the service logs.
- The NodePort patch polls for port 3000 to appear (24 × 5 s = 2 min max). If nginx-gateway-fabric does not reflect the Gateway listener within that window, the script exits with a clear error.
