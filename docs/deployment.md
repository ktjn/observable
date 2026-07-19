# Deployment Guide

Observable ships as a set of container images plus external infrastructure
(ClickHouse, PostgreSQL, Redpanda, OpenFGA, Zitadel). Two supported topologies
are documented below.

## Services

| Service | Default Port | Protocol | Description |
|---------|-------------|----------|-------------|
| **ingest-gateway** | 4317 (gRPC), 4318 (HTTP), 4321 (platform) | OTLP gRPC/HTTP, Prometheus Remote Write | Receives telemetry, authenticates API keys, enqueues to Redpanda |
| **stream-processor** | 4323 (platform) | Internal | Consumes from Redpanda, batches and forwards to storage-writer |
| **storage-writer** | 4320 | Internal HTTP | Writes spans, logs, and metrics to ClickHouse |
| **query-api** | 8090 | HTTP/JSON | Read-path API for traces, logs, metrics, dashboards, alerts |
| **auth-service** | 4319 | HTTP/JSON | OIDC login, session management, API-key validation |
| **admin-service** | 4324 | HTTP/JSON | Member management, token CRUD, alert rule CRUD, config |
| **alert-evaluator** | 4322 (platform) | Internal | Polls alert rules on a timer, fires notifications |
| **frontend** | 80 | HTTP | React SPA served by nginx |

### Infrastructure dependencies

| Component | Purpose | Required |
|-----------|---------|----------|
| **ClickHouse** | Span, log, and metric storage | Yes |
| **PostgreSQL** | Tenant config, API keys, dashboards, alerts, SLOs | Yes |
| **Redpanda** (Kafka-compatible) | Ingest queue between gateway and processor | Yes |
| **Zitadel** | OIDC identity provider | Yes (browser auth) |
| **OpenFGA** | Fine-grained authorization | Yes (admin operations) |

## Trust boundaries

```
Internet / Clients
    |
    v
+-----------------+
| ingest-gateway  |  <-- API-key authenticated
+-----------------+
    |
    v (Redpanda)
+-----------------+
| stream-processor|  <-- Internal only
+-----------------+
    |
    v (HTTP)
+-----------------+
| storage-writer  |  <-- Internal only, no auth
+-----------------+
    |
    v
+-----------------+
| ClickHouse      |
+-----------------+

Browser
    |
    v
+-----------------+
| frontend (SPA)  |
+-----------------+
    |
    v (HTTP)
+-----------------+     +-----------------+
| query-api       |<--->| auth-service    |  <-- Session/cookie authenticated
+-----------------+     +-----------------+
| admin-service   |          |
+-----------------+          v
                     +-----------------+
                     | Zitadel (OIDC)  |
                     +-----------------+
```

**Internal services** (storage-writer, stream-processor, alert-evaluator) have no
authentication on their HTTP endpoints. They must not be exposed outside the
cluster or Compose network.

**Ingestion** is API-key authenticated. Keys are validated against auth-service
on every request.

**Browser-facing APIs** (query-api, admin-service) require a session cookie
obtained through the OIDC login flow. The `require_tenant` middleware validates
the session by calling auth-service.

## TLS

Observable 0.1 does not terminate TLS internally. All inter-service
communication is plaintext HTTP/gRPC within the trusted network boundary.

For production deployments, terminate TLS at a reverse proxy or ingress
controller in front of ingest-gateway (ports 4317/4318) and frontend (port 80).
Internal services should remain on a private network segment.

ClickHouse and PostgreSQL connections are plaintext by default. Configure TLS at
the infrastructure level if the database runs on a separate network segment.

## Environment variables

Every service validates its required configuration at startup using
`domain::config::require_env()`. When `OBSERVABLE_ENV` is not set to `"dev"`,
missing required variables cause the service to exit immediately with an
actionable error message.

Set `OBSERVABLE_ENV=dev` only for local development. The Docker Compose file
sets this automatically. Helm deployments should set `identity.env` to an empty
string or omit it for non-development environments.

### Required variables (all services)

| Variable | Services | Description |
|----------|----------|-------------|
| `DATABASE_URL` | auth, query-api, admin, ingest-gateway, alert-evaluator | PostgreSQL connection string |
| `CLICKHOUSE_URL` | query-api, admin, storage-writer, alert-evaluator | ClickHouse HTTP endpoint |
| `CLICKHOUSE_USER` | query-api, admin, storage-writer, alert-evaluator | ClickHouse username |
| `AUTH_SERVICE_URL` | query-api, admin, ingest-gateway | Internal URL of auth-service |
| `REDPANDA_BROKERS` | ingest-gateway, stream-processor | Kafka-compatible broker addresses |
| `INGEST_TOPIC` | ingest-gateway, stream-processor | Kafka topic for raw telemetry |
| `STORAGE_WRITER_URL` | stream-processor | Internal URL of storage-writer |
| `SESSION_SECRET` | auth-service | Session-signing secret (fail-closed if missing) |
| `ZITADEL_ISSUER` | auth-service | Browser-reachable Zitadel URL |
| `ZITADEL_API_BASE` | auth-service | In-cluster Zitadel URL for server-side calls |
| `ZITADEL_CLIENT_ID` | auth-service | OIDC client ID |
| `ZITADEL_REDIRECT_URI` | auth-service | OIDC callback URL |

### Optional variables

| Variable | Default | Description |
|----------|---------|-------------|
| `CLICKHOUSE_PASSWORD` | `""` | ClickHouse password |
| `OBSERVABLE_ENV` | (none) | Set to `dev` for development defaults |
| `LLM_API_KEY` | (none) | OpenAI-compatible API key for NLQ |
| `QUERY_API_PORT` | `8090` | Query API listen port |
| `*_PORT` variables | See services table | Override default listen ports |
| `ALERT_EVAL_INTERVAL_SECONDS` | `60` | Alert evaluation polling interval |

## Docker Compose (evaluation)

The default `docker compose up -d --build` starts the core platform without demo
applications. This is the supported evaluation topology.

```bash
# Core platform only (evaluation)
docker compose up -d --build

# Include the OpenTelemetry Demo for realistic traffic
docker compose -f docker-compose.yml -f docker-compose.demos.yml up -d --build

# Run smoke tests
docker compose --profile verification run --rm smoke-test
```

The evaluation topology includes:
- All 7 platform services (auth, admin, query-api, ingest-gateway, stream-processor, storage-writer, alert-evaluator)
- Frontend
- ClickHouse, PostgreSQL, Redpanda, Zitadel, OpenFGA
- Migration and bootstrap init containers

## Helm (Kubernetes)

See `charts/observable/values.yaml` for the full configuration reference.

```bash
helm install observable ./charts/observable \
  -f values.production.yaml \
  --set identity.sessionSecret="$(openssl rand -base64 32)"
```

For production, override at minimum:
- `identity.env` — set to `""` (empty) or remove to disable dev defaults
- `identity.sessionSecret` — unique random secret
- `postgres.url` — production PostgreSQL connection string
- `clickhouse.url`, `clickhouse.user`, `clickhouse.password` — production ClickHouse
- `redpanda.brokers` — production Kafka-compatible broker
- `identity.issuer`, `identity.apiBase`, `identity.clientId` — Zitadel/OIDC configuration

See `charts/observable/values.production-example.yaml` for a complete example.
