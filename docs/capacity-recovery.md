# Capacity Assumptions and Recovery Procedures

## Capacity assumptions

Observable is designed to scale from single-node evaluation deployments to multi-node Kubernetes production topologies.
The following are capacity envelopes and sizing guidelines based on the default and production configurations.

### Ingestion

| Parameter | Default | Notes |
|-----------|---------|-------|
| Trace ingest rate limit | 100 req/s per tenant | Configurable via `TRACE_INGEST_RATE_LIMIT_PER_SECOND` |
| Log ingest rate limit | 100 req/s per tenant | Configurable via `LOG_INGEST_RATE_LIMIT_PER_SECOND` |
| Metric ingest rate limit | 100 req/s per tenant | Configurable via `METRIC_INGEST_RATE_LIMIT_PER_SECOND` |
| Metric series budget | 10,000 per tenant | Configurable via `METRIC_SERIES_BUDGET_PER_TENANT` |
| gRPC max message size | 4 MiB | Configurable via `INGEST_GRPC_MAX_MESSAGE_BYTES` |
| Prometheus Remote Write body limit | 5 MiB | Hardcoded; requests above this return 413 |

### Processing

| Parameter | Default | Notes |
|-----------|---------|-------|
| Stream-processor batch size | 500 envelopes | Configurable via `STREAM_PROCESSOR_BATCH_SIZE` |
| Batch flush interval | 200 ms | Configurable via `STREAM_PROCESSOR_BATCH_INTERVAL_MS` |
| Span metrics aggregation flush | 60 s | Hardcoded |
| Alert evaluation interval | 60 s | Configurable via `ALERT_EVAL_INTERVAL_SECONDS` |

### Storage

| Parameter | Notes |
|-----------|-------|
| ClickHouse connections | Each service opens one connection pool to ClickHouse |
| PostgreSQL max connections | 5 per service (default PgPoolOptions) |
| Redpanda topic retention | Governed by Redpanda broker configuration |
| ClickHouse retention | Governed by ClickHouse TTL settings |

## Kubernetes Production Sizing & Sizing Profiles

For Kubernetes environments, cluster resources should be dimensioned based on anticipated ingestion volume, query concurrency, and retention requirements. Three standardized deployment tiers are supported:

| Sizing Profile | Target Throughput | Daily Event Volume | Recommended Cluster Capacity | Target Workloads |
|----------------|-------------------|--------------------|------------------------------|------------------|
| **Evaluation / Dev** | 50–100 req/s | 5M–10M events/day | 1 node, 4 vCPU, 16 GiB RAM | Local kind clusters, development and staging validation |
| **Small Production** | 500 req/s sustained (1,000 req/s peak) | ~50M events/day | 3 nodes, 12 vCPU, 48 GiB RAM total | Small production clusters, single-region business services |
| **Medium Production** | 2,500 req/s sustained (5,000 req/s peak) | ~250M events/day | 5–6 nodes, 40–48 vCPU, 160–192 GiB RAM total | Multi-team production platforms, moderate service fleets |
| **Large Production** | 10,000 req/s sustained (20,000 req/s peak) | ~1B events/day | 10+ nodes, 64–128 vCPU, 256–512 GiB RAM total | High-volume microservice estates, enterprise observability |

### Workload Resource Allocation Matrix

The table below specifies recommended CPU/memory requests, limits, and replica counts for each component across sizing tiers. Resource definitions correspond to values in `charts/observable/values.production-example.yaml`.

| Component | Metric | Small Tier (500 req/s) | Medium Tier (2,500 req/s) | Large Tier (10,000 req/s) | Notes |
|-----------|--------|------------------------|---------------------------|--------------------------|-------|
| `observable-ingest` (`ingest-gateway`) | Replicas<br>CPU Req / Lim<br>Mem Req / Lim | 2<br>250m / 1000m<br>256Mi / 512Mi | 4<br>500m / 2000m<br>512Mi / 1Gi | 8–12<br>1000m / 4000m<br>1Gi / 2Gi | Stateless; horizontal autoscaling via HPA based on CPU/HTTP connection saturation. |
| `observable-process` (`stream-processor`) | Replicas<br>CPU Req / Lim<br>Mem Req / Lim | 1–2<br>500m / 1500m<br>512Mi / 1Gi | 3–4<br>1000m / 3000m<br>1Gi / 2Gi | 6–8<br>2000m / 4000m<br>2Gi / 4Gi | Replicas should not exceed Redpanda partition count for `telemetry.raw.v1`. |
| `observable-store-clickhouse` (`storage-writer`) | Replicas<br>CPU Req / Lim<br>Mem Req / Lim | 1–2<br>500m / 1500m<br>512Mi / 1Gi | 3–4<br>1000m / 3000m<br>1Gi / 2Gi | 6–8<br>2000m / 4000m<br>2Gi / 4Gi | Replicas bounded by `telemetry.normalized.v1` partition count. Performs batching to ClickHouse. |
| `observable-query` (`query-api`) | Replicas<br>CPU Req / Lim<br>Mem Req / Lim | 2<br>250m / 1000m<br>256Mi / 512Mi | 4<br>500m / 2000m<br>512Mi / 1Gi | 8<br>1000m / 4000m<br>1Gi / 2Gi | Scales horizontally with dashboard user concurrency and alerting evaluation load. |
| `observable-auth` (`auth-service`) | Replicas<br>CPU Req / Lim<br>Mem Req / Lim | 2<br>100m / 500m<br>128Mi / 256Mi | 3<br>250m / 1000m<br>256Mi / 512Mi | 4–6<br>500m / 1000m<br>512Mi / 1Gi | Lightweight token caching and session exchange. |
| `observable-control` (`admin-service`) | Replicas<br>CPU Req / Lim<br>Mem Req / Lim | 1<br>100m / 500m<br>128Mi / 256Mi | 2<br>100m / 500m<br>256Mi / 512Mi | 2<br>250m / 1000m<br>256Mi / 512Mi | Low traffic; administrative operations and member role synchronizations. |
| `observable-alerting` (`alert-evaluator`) | Replicas<br>CPU Req / Lim<br>Mem Req / Lim | 1<br>250m / 1000m<br>256Mi / 512Mi | 2<br>500m / 1500m<br>512Mi / 1Gi | 2<br>1000m / 2000m<br>1Gi / 2Gi | Periodic rule evaluation loop; scales with total rule count. |
| `observable-web` (`frontend`) | Replicas<br>CPU Req / Lim<br>Mem Req / Lim | 2<br>50m / 200m<br>64Mi / 128Mi | 2<br>100m / 200m<br>64Mi / 128Mi | 4<br>100m / 200m<br>128Mi / 256Mi | Static asset delivery and reverse-proxying. |

### Storage Dimensioning Formulas

#### 1. ClickHouse Columnar Storage

ClickHouse disk requirements depend on ingested volume, retention period, and columnar compression efficiency:

$$S_{\text{CH}} = E_{\text{daily}} \times B_{\text{avg}} \times C_{\text{ratio}} \times R_{\text{days}} \times (1 + M_{\text{headroom}})$$

Where:
- $E_{\text{daily}}$ = Total telemetry events (traces + logs) per day.
- $B_{\text{avg}}$ = Average raw event size:
  - Spans: ~800 bytes uncompressed
  - Logs: ~500 bytes uncompressed
- $C_{\text{ratio}}$ = Columnar compression ratio. Observable uses LZ4/ZSTD compression on MergeTree tables, yielding a compression ratio of $0.15\text{--}0.20$ (i.e., compressed data occupies 15% to 20% of raw size).
- $R_{\text{days}}$ = Retention window in days (default TTL: 30 days).
- $M_{\text{headroom}}$ = Free disk headroom reserved for background merges and mutations. Minimum recommended: $0.30$ (30% free space).

*Example Calculation (Small Tier, 50M events/day, 30-day retention):*
$$50{,}000{,}000 \times 650\text{ bytes} \times 0.18 \times 30 \times 1.30 \approx 228\text{ GB NVMe PVC}$$

#### 2. Redpanda Queue Buffer Storage

Redpanda topic partitions act as durable shock absorbers between ingestion and storage:

$$S_{\text{RP}} = T_{\text{retention\_hours}} \times \text{Throughput}_{\text{peak\_bytes\_sec}} \times 3600 \times N_{\text{topics}} \times R_{\text{factor}}$$

Where:
- $T_{\text{retention\_hours}}$ = Planned absorption window (minimum 6–8 hours recommended to survive extended downstream maintenance without ingest backpressure).
- $\text{Throughput}_{\text{peak\_bytes\_sec}}$ = Peak uncompressed ingest rate.
- $N_{\text{topics}}$ = Number of active stream topics (`telemetry.raw.v1` and `telemetry.normalized.v1` = 2).
- $R_{\text{factor}}$ = Replication factor (1 for single broker, 3 for HA production cluster).

*Example Calculation (Small Tier, 1,000 req/s peak, 8 hours buffer, single broker):*
$$8 \times (1{,}000 \times 650) \times 3600 \times 2 \times 1 \approx 37.4\text{ GB PVC}$$ (Allocated: 50 GiB).

#### 3. PostgreSQL Database Sizing

PostgreSQL stores tenant configurations, API keys, service definitions, dashboard configurations, alert rules, and audit logs. Sizing guidelines:
- Evaluation: 1 GiB PVC
- Small Tier: 10–20 GiB PVC (replicated via CloudNativePG)
- Medium Tier: 50 GiB PVC
- Large Tier: 100 GiB PVC with automatic volume expansion.

## Service Recovery SLAs & Bounded Degradation Envelopes

The following recovery envelopes govern disaster recovery drills and production incident responses:

| Dependency Failure | Architectural Behavior | Client / User Impact | Target RTO | Target RPO | Automated Verification |
|-------------------|------------------------|----------------------|------------|------------|------------------------|
| **Redpanda Outage** | Fail-closed | Ingest returns HTTP 503. Clients retry with exponential backoff. No silent data drop. | < 2 minutes | 0 (all acknowledged records durable) | `scripts/kind-test.sh --with-drills` (Drill 1) |
| **ClickHouse Outage** | Bounded degradation / queue buffering | Queries return HTTP 500/503. Ingestion continues uninterrupted (buffered in Redpanda). | < 5 minutes | 0 (up to Redpanda buffer retention) | `scripts/kind-test.sh --with-drills` (Drill 2) |
| **PostgreSQL Outage** | Fail-closed | Ingest rejected (HTTP 401/500, cannot validate keys). Admin and query services fail health checks. Services automatically reconnect upon restore. | < 3 minutes | 0 (ACID persistent metadata) | `scripts/kind-test.sh --with-drills` (Drill 3) |
| **Platform Service Crash** | Pod restart / failover | Traffic rerouted to healthy replicas via Kubernetes Service endpoints. Zero-downtime rolling restart. | < 30 seconds | 0 | `scripts/kind-test.sh` (Rollout & Rollback) |

## Latency expectations

Observable 0.1 does not define formal SLAs. The following are design-intent
targets for a single-node evaluation deployment:

| Operation | Target | Notes |
|-----------|--------|-------|
| Ingest request (HTTP, no queue backpressure) | < 50 ms p99 | Measured gateway-side |
| Ingest request (gRPC, no queue backpressure) | < 50 ms p99 | Measured gateway-side |
| Queue-to-storage write latency | < 5 s p99 | Depends on batch size and ClickHouse load |
| Simple trace query | < 2 s p99 | Single-tenant, recent data |
| Alert evaluation cycle | < 30 s | Depends on number of active rules |

These are not guarantees. Actual performance depends on hardware, data volume,
and query complexity.

## Recovery procedures

For step-by-step incident response procedures, see the [Operational Runbooks](runbooks/README.md), [Kubernetes Failure Drills Runbook](runbooks/kubernetes-failure-drills.md), and [Platform Alerting Runbooks](platform-alerting.md).

### Redpanda unavailability

**Symptom:** ingest-gateway returns 503 for new telemetry. stream-processor
stops consuming.

**Recovery:**
1. Restore Redpanda broker(s).
2. Verify topic exists and partitions are online.
3. stream-processor will resume consuming from its last committed offset.
4. ingest-gateway will resume enqueuing immediately.

**Data impact:** Telemetry received during the outage is rejected (fail-closed).
No data is silently dropped or buffered.

### ClickHouse unavailability

**Symptom:** storage-writer health checks fail. Queries return errors. Data
remains queued in Redpanda.

**Recovery:**
1. Restore ClickHouse.
2. storage-writer will resume writing from the Redpanda queue.
3. Data accumulated in Redpanda during the outage is written once ClickHouse
   is available, subject to Redpanda retention limits.

**Data impact:** No data loss if Redpanda retention exceeds the outage duration.

### PostgreSQL unavailability

**Symptom:** API key validation fails, so ingestion is rejected. Admin, auth,
and query services fail health checks.

**Recovery:**
1. Restore PostgreSQL.
2. All services reconnect via their connection pools automatically.

**Data impact:** Ingestion is rejected during the outage (API keys cannot be
validated). No data corruption.

### Zitadel unavailability

**Symptom:** Browser login fails. API-key ingestion continues working.

**Recovery:**
1. Restore Zitadel.
2. Browser login resumes. Existing sessions remain valid if cookies have not
   expired.

### OpenFGA unavailability

**Symptom:** Admin member-role mutations fail.

**Recovery:**
1. Restore OpenFGA.
2. Admin operations resume. Other operations are unaffected.

### Single service crash

**Symptom:** The crashed service's functionality is unavailable.

**Recovery:**
1. Restart the crashed service (Docker Compose restart policy or Kubernetes
   pod restart).
2. The service re-registers with its dependencies on startup.

**Data impact:** Depends on the service. Ingest-gateway crash means ingestion
is unavailable. Stream-processor crash means queued data accumulates in
Redpanda until restart.

## Backpressure behavior

- **Ingest gateway:** Per-tenant rate limiters reject excess requests with 429.
  Per-tenant metric cardinality budgets reject new series above the budget with
  a warning response (the request succeeds but the over-budget series is noted).
  Validated envelopes are published to `telemetry.raw.v1`.
- **Redpanda queue:** Provides durable buffering for `telemetry.raw.v1` and
  `telemetry.normalized.v1`. If brokers or consumers experience lag within retention limits,
  telemetry remains safely buffered without data loss.
- **Stream processor:** Consumes from `telemetry.raw.v1`, normalizes batches, and
  publishes to `telemetry.normalized.v1`. If storage-writer or ClickHouse is slow or
  experiencing an outage, stream processing continues publishing to Redpanda up to retention limits.
- **Storage writer:** Consumes from `telemetry.normalized.v1` and executes idempotent batch
  writes to ClickHouse. If ClickHouse write latency increases or ClickHouse becomes transiently
  unavailable, storage-writer withholds Kafka offset commits and retries with backpressure,
  ensuring at-least-once delivery with deduplication upon recovery.

## Monitoring

All 7 services expose `/health` and `/readyz` endpoints. All services expose
`/metrics` in Prometheus text format for scraping. Services also emit OTLP
traces and logs to themselves via the in-band self-observability path (see
[deployment.md](deployment.md)).

Key metrics to monitor:

| Metric | Service | What it indicates |
|--------|---------|-------------------|
| `ingest_gateway_requests_total` | ingest-gateway | Ingest traffic volume |
| `ingest_gateway_rejections_total` | ingest-gateway | Rate limit or validation failures |
| `stream_processor_batches_processed_total` | stream-processor | Processing throughput |
| `alert_evaluator_evaluations_total` | alert-evaluator | Alert cycle health |
| `*_http_requests_total` | all services | Per-service request volume |
| `*_http_request_duration_seconds` | all services | Per-service latency |
