# Resource Requirements

Minimum resources for an Observable 0.1 evaluation deployment.

## Evaluation (Docker Compose)

The evaluation topology runs all services on a single host.

| Resource | Minimum | Recommended |
|----------|---------|-------------|
| CPU | 4 cores | 8 cores |
| Memory | 8 GB | 16 GB |
| Disk | 20 GB | 50 GB |
| Docker | 20.10+ | Latest stable |

### Expected idle resource use

With no active ingestion, the platform consumes approximately:

| Component | CPU | Memory |
|-----------|-----|--------|
| ClickHouse | <100m | ~300 MB |
| PostgreSQL | <50m | ~100 MB |
| Redpanda | <100m | ~500 MB |
| Zitadel | <50m | ~150 MB |
| OpenFGA | <50m | ~50 MB |
| Platform services (7) | <100m total | ~100 MB each (~700 MB total) |
| Frontend (nginx) | <10m | ~30 MB |
| **Total idle** | **~500m** | **~1.8 GB** |

## Kubernetes (Helm)

The Helm chart sets resource requests and limits for each service.
See `charts/observable/values.yaml` for defaults and
`charts/observable/values.production-example.yaml` for a production baseline.

### Per-service defaults

| Service | CPU request | Memory request | CPU limit | Memory limit |
|---------|------------|----------------|-----------|-------------|
| auth-service | 100m | 128Mi | 500m | 256Mi |
| admin-service | 100m | 128Mi | 500m | 256Mi |
| ingest-gateway | 200m | 256Mi | 1000m | 512Mi |
| stream-processor | 100m | 128Mi | 500m | 256Mi |
| storage-writer | 200m | 256Mi | 1000m | 512Mi |
| query-api | 200m | 256Mi | 1000m | 512Mi |
| alert-evaluator | 100m | 128Mi | 500m | 256Mi |
| frontend | 50m | 64Mi | 200m | 128Mi |

Total default requests: **1050m CPU, 1.2 GiB memory** (platform services only,
excluding infrastructure).

### Scaling considerations

- **ingest-gateway** is the primary scaling target. Scale horizontally based on
  ingestion throughput.
- **storage-writer** may need scaling if ClickHouse write latency increases
  under load.
- **query-api** scales with concurrent query load.
- **alert-evaluator** is single-instance by design (uses a polling timer, not
  distributed locking). Running multiple replicas causes duplicate evaluations.
- **stream-processor** can run multiple replicas if the Redpanda topic has
  multiple partitions.

## Disk

| Component | Growth driver | Retention |
|-----------|--------------|-----------|
| ClickHouse | Span, log, and metric volume | Configurable via `RETENTION_*` env vars on storage-writer; default is no automatic deletion |
| PostgreSQL | Tenant config, API keys, dashboards, alerts | Grows slowly; typically <1 GB |
| Redpanda | In-flight telemetry queue | Configurable topic retention; default 24h |

Plan ClickHouse disk based on expected ingestion rate. As a rough guide:
~1000 spans/sec sustained produces approximately 1-2 GB/day of compressed
ClickHouse data (varies with span attribute count and cardinality).
