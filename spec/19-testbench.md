# 19 — Test Bench

## Purpose

The Observable test bench is a self-contained synthetic workload that runs inside a `kind` cluster alongside the Observable platform. It provides a realistic multi-service application — a small "shop" — whose traffic continuously produces traces, metrics, and logs so that the full ingest-to-query pipeline can be exercised, demonstrated, and performance-tested without any external dependencies.

**Goals:**
- Realistic multi-signal telemetry (traces, metrics, logs) from diverse service types (HTTP API, background worker, browser-like frontend, queue consumer, database)
- Kubernetes-native cluster monitoring: pod/node metrics, k8s Events, and container logs shipped alongside application telemetry
- Deterministic deployment via a single Helm chart and one script — usable locally and in CI
- No dependency on external networks or cloud services; everything runs in-cluster

---

## 19.1 Testbench Namespace

The Observable platform deploys into the `observable` namespace (ADR-010, ADR-020). The test bench deploys into a separate `testbench` namespace so the two can be independently managed and so cross-namespace traffic mirrors a real customer deployment.

---

## 19.2 Service Topology

```
┌─────────────────────────────── testbench namespace ─────────────────────────────────────────┐
│                                                                                              │
│  shop-loadgen  ──HTTP──▶  shop-frontend ──HTTP──▶  shop-api ──SQL──▶ shop-db (PostgreSQL)   │
│      │                                                 │                                     │
│      └─────────────────────────────────────────────▶  └──AMQP──▶ shop-queue (RabbitMQ)      │
│                                                                         │                    │
│                                                              shop-worker ──SQL──▶ shop-db    │
│                                                                                              │
│  ┌──────────────────── OTel collection ────────────────────────────────────────────────┐    │
│  │                                                                                     │    │
│  │  All services ──OTLP/gRPC──▶ otel-collector-gateway (Deployment, 4317/4318)        │    │
│  │                                       │                                             │    │
│  │  k8s API ◀── k8s_cluster receiver ───┤  (cluster metrics)                         │    │
│  │  k8s API ◀── k8sobjects receiver ────┤  (k8s Events as logs)                      │    │
│  │                                       │                                             │    │
│  │  otel-collector-agent (DaemonSet) ───▶│                                            │    │
│  │    ├─ kubeletstats (per-node/pod CPU, mem, net)                                    │    │
│  │    └─ filelog (/var/log/containers → container stdout/stderr)                      │    │
│  │                                       │                                             │    │
│  └───────────────────────────────────────┼─────────────────────────────────────────────┘   │
│                                          │ OTLP/HTTP                                        │
└──────────────────────────────────────────┼──────────────────────────────────────────────────┘
                                           ▼
                     ingest-gateway.observable.svc.cluster.local:4318
                                           │
                              Observable data pipeline
                     (Redpanda → stream-processor → storage-writer → ClickHouse)
```

### Services

| Service | Image | Port | Role |
|---|---|---|---|
| `shop-frontend` | `testbench-frontend:local` (Node.js 20 + Express) | 3000 | Web tier / BFF, server-side OTel auto-instrumentation |
| `shop-api` | `testbench-api:local` (Python 3.12 + FastAPI) | 8000 | REST API: products, orders, users |
| `shop-worker` | `testbench-worker:local` (Python 3.12) | — | RabbitMQ consumer, simulates order processing |
| `shop-db` | `postgres:16` | 5432 | Application-level database (separate from Observable's PostgreSQL) |
| `shop-queue` | `rabbitmq:3.13-management` | 5672 / 15672 | Application-level queue (separate from Observable's Redpanda) |
| `otel-collector-gateway` | `otel/opentelemetry-collector-contrib:0.106.1` | 4317 / 4318 | OTel gateway: receives OTLP, runs cluster-level k8s receivers, exports to Observable |
| `otel-collector-agent` | `otel/opentelemetry-collector-contrib:0.106.1` | — | DaemonSet: kubeletstats + filelog per node, forwards to gateway |
| `shop-loadgen` | `testbench-loadgen:local` (Python 3.12) | — | Random-interval traffic generator + direct metric/log emitter |

---

## 19.3 Traffic Generation

`shop-loadgen` runs a continuous Poisson-distributed loop (mean ≈ 5 s, clamped 1–30 s) selecting one of four weighted scenarios:

| Weight | Scenario | Signals produced |
|---|---|---|
| 40 % | Browse products (`GET /products`) | Trace: frontend → api → db; INFO log |
| 30 % | Place order (`POST /orders`) | Trace: api → queue publish → worker consume → db write; INFO log |
| 20 % | User lookup (`GET /users/{id}`) | Trace: api → db; INFO log |
| 10 % | Error paths (invalid id / bad auth) | Error span + ERROR log; HTTP 4xx/5xx |

Every 15 s the loadgen also emits direct OTel metrics:
- `shop.cart.active_count` (gauge, random 0–200)
- `shop.orders.pending_count` (gauge, random 0–50)
- `shop.requests.total` (counter, incremented each loop iteration)

`shop-frontend` independently:
- Pings `GET shop-api /health` every 30 s
- Queries `GET shop-api /products` every random(10, 60) s

---

## 19.4 OTel Instrumentation Contract

### SDKs and instrumentors

| Service | SDK | Auto-instrumented scopes | Manual spans |
|---|---|---|---|
| `shop-frontend` | `@opentelemetry/sdk-node` + `@opentelemetry/auto-instrumentations-node` | `http`, `express` | `frontend.api_call` |
| `shop-api` | `opentelemetry-sdk` + FastAPI/asyncpg/aio-pika instrumentors | HTTP server, SQL, AMQP publish | `order.place` |
| `shop-worker` | `opentelemetry-sdk` + pika instrumentor | AMQP consume | `worker.process_order` |
| `shop-loadgen` | `opentelemetry-sdk` | — | `loadgen.scenario.*`; direct metrics + logs |

### Resource attributes (all services)

Every service sets these resource attributes via SDK or downward API env vars:

| Attribute | Value |
|---|---|
| `service.name` | `shop-frontend` / `shop-api` / `shop-worker` / `shop-loadgen` |
| `service.version` | Image label `org.opencontainers.image.version` |
| `k8s.pod.name` | From `MY_POD_NAME` env (downward API) |
| `k8s.namespace.name` | From `MY_POD_NAMESPACE` env (downward API) |

The OTel Collector gateway's `resource` processor additionally upserts:
- `deployment.environment` = `testbench`
- `k8s.cluster.name` = `observable-test`

The `k8sattributes` processor in the gateway enriches all signals with:
- `k8s.pod.name`, `k8s.pod.uid`, `k8s.namespace.name`, `k8s.node.name`, `k8s.deployment.name`

### OTLP export endpoint

All services export to `otel-collector-gateway.testbench.svc.cluster.local:4317` (OTLP/gRPC).

---

## 19.5 OTel Collector Configuration

### 19.5.1 Gateway (Deployment)

Receives OTLP from services and the agent DaemonSet. Runs cluster-level k8s receivers (one instance only).

**Receivers:**
- `otlp` — gRPC on 4317, HTTP on 4318
- `k8s_cluster` — cluster metrics (pod/node counts, container restarts, resource requests/limits, node conditions `Ready`, `MemoryPressure`, `DiskPressure`); 30 s interval
- `k8sobjects` — watches `events.k8s.io/events` and emits them as OTLP log records

**Processors:**
- `k8sattributes` — enriches with pod metadata using ServiceAccount credentials
- `resource` — upserts `deployment.environment` and `k8s.cluster.name`
- `batch` — 256 items / 5 s timeout

**Exporter:** `otlphttp/observable` → `http://ingest-gateway.observable.svc.cluster.local:4318` with `Authorization: Bearer dev-api-key-0000`

**Pipelines:**
- traces: `otlp` → `k8sattributes, resource, batch` → `otlphttp/observable`
- metrics: `otlp, k8s_cluster` → `k8sattributes, resource, batch` → `otlphttp/observable`
- logs: `otlp, k8sobjects` → `resource, batch` → `otlphttp/observable`

### 19.5.2 Agent (DaemonSet)

Runs on every cluster node. Requires `hostPath` volumes and node-level kubelet access.

**Receivers:**
- `kubeletstats` — per-node/pod/container metrics (CPU, memory, network, filesystem) from kubelet stats summary API; 30 s interval; `insecure_skip_verify: true` (kind uses self-signed kubelet TLS)
- `filelog` — scrapes `/var/log/containers/*.log`; uses `container` operator to parse CRI format and attach k8s pod metadata

**Processors:** `k8sattributes`, `resource`, `batch` (same as gateway)

**Exporter:** `otlp/gateway` → `otel-collector-gateway.testbench.svc.cluster.local:4317` (plain gRPC, no TLS)

**Pipelines:**
- metrics: `kubeletstats` → `k8sattributes, resource, batch` → `otlp/gateway`
- logs: `filelog` → `k8sattributes, resource, batch` → `otlp/gateway`

### 19.5.3 RBAC

Both gateway and agent share a single `ServiceAccount` (`otel-collector`) bound to a `ClusterRole` granting `get`, `list`, `watch` on:

- `pods`, `namespaces`, `nodes`, `replicationcontrollers` (for `k8sattributes`)
- `resourcequotas`, `services`, `statefulsets`, `deployments`, `daemonsets`, `replicasets`, `horizontalpodautoscalers` (for `k8s_cluster`)
- `events`, `events.k8s.io` (for `k8sobjects`)

---

## 19.6 Deployment Model

The test bench is deployed as a Helm chart (`charts/observable-testbench`) into the `testbench` namespace of the `observable-test` kind cluster. The Observable platform must already be deployed in the `observable` namespace (handled by `scripts/kind-test.sh` or assumed pre-existing).

**Entry point:** `bash scripts/testbench.sh [--skip-build] [--keep-cluster] [--recreate] [--observable-ns <ns>]`

The script:
1. Checks prerequisites (kind, kubectl, helm, docker)
2. Builds four testbench Docker images (skip with `--skip-build`)
3. Creates or reuses the `observable-test` kind cluster
4. Deploys the Observable platform (`scripts/kind-test.sh` logic)
5. Loads testbench images into kind
6. `helm install observable-testbench charts/observable-testbench -n testbench --create-namespace`
7. Waits for all Deployments and the DaemonSet to become ready
8. Installs Kubernetes Gateway API CRDs and nginx-gateway-fabric
9. Applies a two-listener `Gateway` and two `HTTPRoute` resources
10. Patches the nginx-gateway-fabric Service to pin the shop NodePort
11. Runs a non-fatal smoke check against both gateway URLs
12. Blocks indefinitely — prints access URLs and waits for Ctrl+C

---

## 19.7 Verification

After `bash scripts/testbench.sh` exits 0 and the idle loop is running:

```bash
# Both UIs reachable via Gateway API
curl -s http://localhost:8080/ | grep -i "<!doctype html"   # Observable frontend
curl -s http://localhost:3000/ | grep -i "<!doctype html"   # Testbench shop
```

After `bash scripts/testbench.sh --keep-cluster` exits 0:

```bash
# All testbench pods running
kubectl get pods -n testbench

# Loadgen producing traffic
kubectl logs -n testbench deploy/shop-loadgen

# Gateway exporting without errors
kubectl logs -n testbench deploy/otel-collector-gateway

# Agent collecting node metrics and container logs
kubectl logs -n testbench daemonset/otel-collector-agent

# Query Observable for testbench signals
kubectl port-forward svc/query-api 8090:8090 -n observable &

# Application traces
curl -s "http://localhost:8090/v1/traces?tenant_id=00000000-0000-0000-0000-000000000001" \
  -H "Authorization: Bearer dev-api-key-0000" | jq '[.[].service_name] | unique'
# Expected: ["shop-api","shop-frontend","shop-loadgen","shop-worker"]

# Application metrics
curl -s "http://localhost:8090/v1/metrics?tenant_id=00000000-0000-0000-0000-000000000001" \
  -H "Authorization: Bearer dev-api-key-0000" | jq '[.[].name] | unique | map(select(startswith("shop.")))'
# Expected: ["shop.cart.active_count","shop.orders.pending_count","shop.requests.total"]

# Kubernetes cluster metrics
curl -s "http://localhost:8090/v1/metrics?tenant_id=00000000-0000-0000-0000-000000000001" \
  -H "Authorization: Bearer dev-api-key-0000" | jq '[.[].name] | unique | map(select(startswith("k8s.")))'
# Expected: ["k8s.node.condition.ready","k8s.pod.phase","k8s.container.cpu_request_utilization",...]

# Container/node resource metrics from kubeletstats
curl -s "http://localhost:8090/v1/metrics?tenant_id=00000000-0000-0000-0000-000000000001" \
  -H "Authorization: Bearer dev-api-key-0000" | jq '[.[].name] | unique | map(select(startswith("container.") or startswith("k8s.node.")))'

# Kubernetes Events as logs
curl -s "http://localhost:8090/v1/logs?tenant_id=00000000-0000-0000-0000-000000000001" \
  -H "Authorization: Bearer dev-api-key-0000" | jq '[.[] | select(.attributes["k8s.object.kind"] == "Event")] | length'
# Expected: > 0
```

---

## 19.8 ADR Alignment

| ADR | How the test bench applies it |
|---|---|
| ADR-001 | All testbench services emit OTLP exclusively; no proprietary format |
| ADR-010 | Test bench runs in kind under Helm; Docker Compose is not used for testbench |
| ADR-019 | `scripts/testbench.sh` is the single locally-runnable entry point |
| ADR-020 | Test bench packaged as a Helm chart under `charts/observable-testbench/` |
| ADR-023 | OTel Collector uses standard ports 4317 (gRPC) and 4318 (HTTP) |

No new ADR is required: the test bench introduces no new architectural decisions. It is an implementation of existing patterns (OTel ingest, Helm on kind) applied to a synthetic workload.
