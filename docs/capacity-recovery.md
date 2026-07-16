# Capacity Assumptions and Recovery Procedures

## Capacity assumptions

Observable 0.1 is designed for evaluation and small non-critical deployments.
The following are rough capacity envelopes based on the default configuration.

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
- **Redpanda queue:** If the queue grows beyond Redpanda's configured retention,
  oldest messages are dropped. Observable does not apply application-level
  backpressure to Redpanda.
- **Stream processor:** Processes batches sequentially. If storage-writer is
  slow, batch processing blocks but does not crash. Redpanda consumer lag
  grows.
- **Storage writer:** Writes to ClickHouse synchronously per request. If
  ClickHouse is slow, the HTTP handler blocks and stream-processor batches
  queue up.

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
