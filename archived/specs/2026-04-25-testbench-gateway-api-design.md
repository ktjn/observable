# Testbench Gateway API Exposure Design

**Date:** 2026-04-25
**Status:** Approved

## Problem

`scripts/testbench.sh` currently exits after running smoke checks. Both UIs — the
Observable frontend and the testbench shop-frontend — are only reachable via manual
`kubectl port-forward` commands printed at the end of the script. There is no persistent
ingress and no Kubernetes Gateway API layer.

## Goal

The testbench script should:
1. Start a kind cluster (or reuse one) and deploy everything in it
2. Expose the Observable frontend at `http://localhost:8080/` and the testbench
   shop-frontend at `http://localhost:3000/` via the Kubernetes Gateway API
3. Block indefinitely — the developer uses the UIs manually and presses Ctrl+C to tear down

## Architecture

```
Host machine
  localhost:8080  ──▶  kind node:30080  ──▶  nginx-gateway-fabric Service (port 80, NodePort 30080)
  localhost:3000  ──▶  kind node:30300  ──▶  nginx-gateway-fabric Service (port 3000, NodePort 30300)
                                                        │
                               Gateway (observable ns)  │  listener:80   listener:3000
                                                        │
                    HTTPRoute (observable ns) ◀─────────┤  port 80
                      → frontend.observable:80          │
                                                        │
                    HTTPRoute (testbench ns)  ◀─────────┘  port 3000
                      → shop-frontend.testbench:3000
```

The two kind `extraPortMappings` (set at cluster creation) forward host ports to the
fixed NodePorts on the nginx-gateway-fabric Service.

## Cluster Lifecycle

| Scenario | Behaviour |
|---|---|
| No cluster exists | Create cluster with kind config, then run all install steps |
| Cluster exists, no flags | Skip cluster creation only; all install steps run (idempotent) |
| `--recreate` | Delete existing cluster, recreate with config, then run all install steps |
| `--skip-observable` | Skip Observable deploy step only |
| `--skip-build` | Skip all Docker builds |
| `--keep-cluster` | Do not delete cluster on Ctrl+C |
| Ctrl+C (default) | `kind delete cluster` via EXIT trap |

"Reuse" means skipping `kind create cluster` only — every `helm upgrade --install` and
`kubectl apply` step still runs because they are all idempotent. This ensures Gateway API
is always configured even on a cluster that already existed.

## Files Changed

### `scripts/testbench-kind-config.yaml` (new)

Kind cluster config with two `extraPortMappings`:

```yaml
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

### `scripts/testbench.sh` (rewrite)

Script sections in order:

1. **Prereq check** — unchanged
2. **Build testbench images** — unchanged
3. **Cluster** — reuse if exists and `--recreate` not set; otherwise delete + recreate with
   `testbench-kind-config.yaml`
4. **Deploy Observable** — calls `kind-test.sh --keep-cluster --reuse-cluster --cluster-name
   ... --deploy-only` (new flag, see below)
5. **Load images + Helm install** — unchanged
6. **Gateway API CRDs** — `kubectl apply -f` the standard-install.yaml from
   `kubernetes-sigs/gateway-api` releases
7. **nginx-gateway-fabric** — `helm upgrade --install` via OCI, `service.type=NodePort`,
   `service.nodePorts.http=30080`
8. **Apply Gateway + HTTPRoutes** — inline heredoc `kubectl apply -f -`
9. **Patch Service NodePort** — wait for port 3000 to appear on the Service, then patch
   `nodePort` to 30300
10. **Wait for Gateway Programmed** — `kubectl wait gateway/testbench-gateway --for=condition=Programmed`
11. **Smoke check** — non-fatal `curl` to both URLs, warns if not reachable yet
12. **Idle loop** — print URLs, block with `while true; sleep 60; done`

### `scripts/kind-test.sh` (small change)

Add `DEPLOY_ONLY=false` variable and `--deploy-only` CLI flag.

Wrap the smoke-check port-forward block and the Helm rollback demo in:
```bash
if [[ "$DEPLOY_ONLY" == "false" ]]; then
  # ... smoke checks and rollback demo ...
fi
```

The done message remains for both paths with slightly different wording:
- deploy-only: `"Observable platform deployed"`
- full: `"kind integration test PASSED"`

This keeps the local-CI caller (`local-ci.sh` or direct invocation) fully unchanged.

## Gateway API Configuration

### Gateway (in `observable` namespace)

```yaml
apiVersion: gateway.networking.k8s.io/v1
kind: Gateway
metadata:
  name: testbench-gateway
  namespace: observable
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
```

`allowedRoutes.namespaces.from: All` is required so the `testbench` namespace HTTPRoute
can bind to this Gateway without a `ReferenceGrant`.

### HTTPRoute — Observable frontend (in `observable` namespace)

```yaml
parentRefs: [{name: testbench-gateway, sectionName: observable}]
rules:
  - matches: [{path: {type: PathPrefix, value: /}}]
    backendRefs: [{name: frontend, port: 80}]
```

Observable's nginx handles the internal `/v1` → query-api proxy; no extra route needed.

### HTTPRoute — Testbench shop (in `testbench` namespace)

```yaml
parentRefs: [{name: testbench-gateway, namespace: observable, sectionName: shop}]
rules:
  - matches: [{path: {type: PathPrefix, value: /}}]
    backendRefs: [{name: shop-frontend, port: 3000}]
```

No path rewrite needed — the shop gets its own dedicated listener port and serves from root.

### nginx-gateway-fabric Service patch

After the Gateway is created, nginx-gateway-fabric adds both ports (80, 3000) to its
Service. The script waits for port 3000 to appear, then patches the NodePort to the
fixed value needed by kind.

The patch uses `jq` to locate the port-3000 entry by port number rather than by array
index (index-based patching is fragile if the controller reorders ports):

```bash
PATCH=$(kubectl get service ngf-nginx-gateway-fabric -n nginx-gateway -o json \
  | jq '[.spec.ports | to_entries[]
         | select(.value.port == 3000)
         | {"op": "replace",
            "path": "/spec/ports/\(.key)/nodePort",
            "value": 30300}]')
kubectl patch service ngf-nginx-gateway-fabric -n nginx-gateway \
  --type=json -p="$PATCH"
```

## Version Pins

| Component | Version | Update reference |
|---|---|---|
| Gateway API CRDs | `v1.2.1` | https://github.com/kubernetes-sigs/gateway-api/releases |
| nginx-gateway-fabric chart | `1.5.1` | https://github.com/nginx/nginx-gateway-fabric/releases |

Both are pinned as variables at the top of `testbench.sh` with comments pointing to
their release pages.

## What Does Not Change

- `local-ci.sh` — not touched; still calls `kind-test.sh` without `--deploy-only`
- `kind-test.sh` default behavior — unchanged; `--deploy-only` is additive only
- Testbench Helm chart and service templates — not touched
- Observable Helm chart — not touched
- `spec/19-testbench.md` — updated in the same PR to reflect the new access URLs and
  Gateway API deployment model
