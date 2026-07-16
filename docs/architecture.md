# Architecture, Configuration, and Troubleshooting

## Architecture overview

Observable is a multi-service observability platform that ingests, stores, and
queries traces, logs, and metrics. It uses a write-path/read-path split with
asynchronous processing.

```
Clients (OTLP gRPC/HTTP, Prom Remote Write)
    │
    ▼
┌──────────────────┐
│  ingest-gateway  │ ── API-key auth, rate limiting, validation
└──────────────────┘
    │ (Redpanda topics: traces, logs, metrics)
    ▼
┌──────────────────┐
│ stream-processor │ ── Batching, span-metrics aggregation
└──────────────────┘
    │
    ▼
┌──────────────────┐
│  storage-writer  │ ── Writes to ClickHouse
└──────────────────┘

┌──────────────────┐
│    query-api     │ ── Reads from ClickHouse, serves trace/log/metric queries
└──────────────────┘

┌──────────────────┐
│   auth-service   │ ── OIDC login (Zitadel), session management, API-key validation
└──────────────────┘

┌──────────────────┐
│  admin-service   │ ── Tenant/member/token/dashboard/alert CRUD (PostgreSQL + OpenFGA)
└──────────────────┘

┌──────────────────┐
│ alert-evaluator  │ ── Periodic alert rule evaluation, webhook notifications
└──────────────────┘

┌──────────────────┐
│    frontend      │ ── React SPA served by nginx
└──────────────────┘
```

### Data flow

1. **Ingestion**: Clients send OTLP or Prometheus Remote Write to
   `ingest-gateway`. The gateway validates API keys, enforces rate limits, and
   publishes validated envelopes to Redpanda topics.

2. **Processing**: `stream-processor` consumes from Redpanda, batches records,
   computes span-derived metrics, and forwards batches to `storage-writer`.

3. **Storage**: `storage-writer` inserts batches into ClickHouse tables
   (`traces`, `logs`, `metrics`). Schema migrations run on service startup.

4. **Query**: `query-api` reads from ClickHouse and serves the frontend and
   external consumers. It handles trace search, log filtering, metric queries,
   and cross-signal correlation.

5. **Administration**: `admin-service` manages tenants, members, API keys,
   dashboards, alert rules, SLOs, and notification channels through PostgreSQL.
   Fine-grained authorization checks go through OpenFGA.

6. **Alerting**: `alert-evaluator` periodically evaluates alert rules by
   querying ClickHouse, updates rule state in PostgreSQL, and delivers
   notifications to configured webhook channels.

### Storage layout

| Store | Contents |
|-------|----------|
| **ClickHouse** | Spans, logs, metric data points. Column-oriented storage optimized for time-range queries. Retention is configurable per table. |
| **PostgreSQL** | Tenant configuration, user accounts, API keys, dashboards, alert rules, SLOs, notification channels, schema annotations. |
| **Redpanda** | Transient ingest queue. Data is consumed by stream-processor and not retained long-term. |

### Authentication and authorization

- **Browser users**: OIDC flow through Zitadel. Sessions are stored as signed
  cookies. `SESSION_SECRET` must be set in non-development environments.
- **API clients**: Bearer token authentication using API keys created through
  the admin service or the onboarding wizard.
- **Tenant isolation**: Every API request carries a tenant context
  (`X-Tenant-Id` header). Queries are scoped to the authenticated tenant.
- **Authorization**: OpenFGA enforces fine-grained access control for
  administrative operations (member management, role changes).

## Configuration reference

All services are configured via environment variables. Required variables cause
startup failure with an actionable error when missing.

### Common variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `SESSION_SECRET` | Yes (non-dev) | — | Secret for signing session cookies |
| `DATABASE_URL` | Yes | — | PostgreSQL connection string |
| `CLICKHOUSE_URL` | Yes | — | ClickHouse HTTP endpoint |
| `REDPANDA_BROKERS` | Yes | — | Comma-separated Kafka broker addresses |
| `ZITADEL_URL` | Yes | — | Zitadel OIDC issuer URL |
| `OPENFGA_API_URL` | Yes | — | OpenFGA API endpoint |

### Ingestion tuning

| Variable | Default | Description |
|----------|---------|-------------|
| `TRACE_INGEST_RATE_LIMIT_PER_SECOND` | 100 | Per-tenant trace request rate limit |
| `LOG_INGEST_RATE_LIMIT_PER_SECOND` | 100 | Per-tenant log request rate limit |
| `METRIC_INGEST_RATE_LIMIT_PER_SECOND` | 100 | Per-tenant metric request rate limit |
| `METRIC_SERIES_BUDGET_PER_TENANT` | 10000 | Max active metric series per tenant |
| `INGEST_GRPC_MAX_MESSAGE_BYTES` | 4194304 | Max gRPC message size (bytes) |

### Processing tuning

| Variable | Default | Description |
|----------|---------|-------------|
| `STREAM_PROCESSOR_BATCH_SIZE` | 500 | Envelopes per batch |
| `STREAM_PROCESSOR_BATCH_INTERVAL_MS` | 200 | Max batch wait time (ms) |
| `ALERT_EVAL_INTERVAL_SECONDS` | 60 | Alert evaluation period |

### Self-observability

| Variable | Default | Description |
|----------|---------|-------------|
| `OBSERVABLE_SELF_OBSERVABILITY_MODE` | `self` | `self` to send telemetry to own ingest, `off` to disable |
| `OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT` | `http://ingest-gateway:4317` | OTLP endpoint for self-telemetry |
| `OBSERVABLE_SELF_OBSERVABILITY_BEARER_TOKEN` | — | API key for self-telemetry ingestion |

## Security hardening

### Required for any non-evaluation deployment

1. **Set `SESSION_SECRET`** to a random 64+ character string. Observable refuses
   to start without it outside development mode.

2. **Use unique database passwords.** Override `CH_PASSWORD`, `PG_PASSWORD`, and
   Zitadel credentials from the evaluation defaults.

3. **TLS termination.** Place a reverse proxy (nginx, Traefik, cloud LB) in
   front of all public-facing ports. Observable services communicate in
   plaintext on the internal network.

4. **Network isolation.** Only `ingest-gateway` (ports 4317/4318) and
   `frontend` (port 80/443) should be reachable from untrusted networks. All
   other services should be internal-only.

5. **API key rotation.** Create scoped API keys per application. Revoke
   compromised keys through the admin UI or API.

### Recommended

- Enable Redpanda authentication if the Redpanda cluster is shared.
- Set ClickHouse user passwords and restrict network access.
- Use PostgreSQL SSL connections in production.
- Review OpenFGA authorization model for custom role requirements.

## Troubleshooting

### Services fail to start

**Symptom**: A service exits immediately with a configuration error.

**Cause**: Required environment variables are missing. Observable validates
configuration at startup and fails closed.

**Fix**: Check the service logs (`docker compose logs <service>`) for the
specific missing variable. Set it in your `.env` file or environment.

### No data appears after sending telemetry

**Symptom**: `curl` to the OTLP endpoint returns 200 but no traces/logs appear.

**Check each stage**:

1. **Ingest gateway**: Check `docker compose logs ingest-gateway` for rate
   limiting (429) or validation errors. Verify the API key is valid.

2. **Redpanda**: Check that topics exist and have data:
   ```bash
   docker compose exec redpanda rpk topic list
   docker compose exec redpanda rpk topic consume traces --num 1
   ```

3. **Stream processor**: Check `docker compose logs stream-processor` for
   deserialization or batch errors.

4. **Storage writer**: Check `docker compose logs storage-writer` for
   ClickHouse connection or insert errors.

5. **ClickHouse**: Verify data exists:
   ```bash
   docker compose exec clickhouse clickhouse-client \
     --user default --password observable \
     --query "SELECT count() FROM observable.traces"
   ```

### High memory or CPU usage

**Symptom**: A service consumes more resources than expected.

**Check**:

- `ingest-gateway`: High ingestion rate. Check `/metrics` for
  `ingest_requests_total` and rate limit counters.
- `stream-processor`: Large batch backlog. Check `/metrics` for queue depth.
- `storage-writer`: ClickHouse insert latency. Check ClickHouse system tables.
- `alert-evaluator`: Complex or many alert rules. Check evaluation duration in
  `/metrics`.

See [Resource Requirements](resource-requirements.md) for expected baselines
and [Capacity and Recovery](capacity-recovery.md) for tuning guidance.

### Alert rules not firing

1. Verify the rule is in `active` state (not silenced) via the API or UI.
2. Check `alert-evaluator` logs for evaluation errors.
3. Verify the metric name in the rule matches data in ClickHouse.
4. Check notification channel webhook URLs are reachable.

### OIDC login failures

1. Verify Zitadel is healthy: `curl http://localhost:8080/healthz`.
2. Check `auth-service` logs for token exchange errors.
3. Verify the OIDC redirect URI matches the frontend URL.
4. Clear browser cookies and retry.

### Dependency recovery

Observable is designed to recover automatically when dependencies come back:

- **Redpanda down**: Ingestion fails (503). Data buffered in client retries is
  accepted when Redpanda recovers.
- **ClickHouse down**: Writes queue in storage-writer. Queries return errors.
  Both resume when ClickHouse recovers.
- **PostgreSQL down**: Admin operations and auth fail. Ingestion continues if
  API keys are cached.
- **OpenFGA down**: Authorization checks fail closed. Admin operations return
  500 until OpenFGA recovers.

See [Capacity and Recovery](capacity-recovery.md) for detailed recovery
procedures and tested failure scenarios.
