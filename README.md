# Observable Platform

Full-stack observability platform specification.

## Documentation

The full specification is located in the [spec/](spec/) directory.

Implementation plans and iteration documents can be found in [docs/superpowers/plans/](docs/superpowers/plans/).

## AI Agent Instructions

Mandatory instructions for any AI agent interacting with this repository can be found in [AGENTS.md](AGENTS.md).
(The legacy [AGENT.md](AGENT.md), [GEMINI.md](GEMINI.md), and [CLAUDE.md](CLAUDE.md) files are now pointers to the canonical instructions).

## Development

### Quick start

The entire stack is started with a single command. All service images are built
locally from source — no registry login needed. The first run compiles the Rust
backend, which typically takes **3–10 minutes** depending on your machine; subsequent
starts reuse the Docker layer cache and are nearly instant.

```bash
# Recommended: build images then start (required on first run or after pulling new code)
make dev
# Equivalent:
docker compose up -d --build

# Start only (if images are already built from a previous run)
docker compose up -d

# Force a full rebuild from scratch
docker compose up -d --build --no-cache
```

A `Makefile` wraps the most common commands:

```bash
make dev            # remove setup containers, then docker compose up -d --build
make dev-down       # docker compose down
make db-migrate     # re-run postgres/clickhouse setup to apply any new migrations
make reset-volumes  # wipe all persistent volumes and start fresh
make smoke-test     # run end-to-end smoke tests
make lint           # cargo fmt/clippy + npm lint
make test           # cargo test + frontend typecheck
```

> **After pulling new code that adds migrations**, run `make db-migrate` to apply them
> without a full restart.

### Service URLs

| Service | URL |
|---------|-----|
| **Frontend** | http://localhost:5173 |
| **Ingest gateway** (HTTP/OTLP) | http://localhost:4318 |
| **Ingest gateway** (gRPC/OTLP) | grpc://localhost:4317 |
| **Query API** | http://localhost:8090 |
| **Auth service** | http://localhost:4319 |
| **Admin service** | http://localhost:4324 |
| **OpenFGA** (authorization) | http://localhost:8083 |
| **Zitadel** (identity provider) | http://localhost:8082 |
| **ClickHouse** (HTTP) | http://localhost:8123 |
| **Redpanda** (Kafka-compatible) | localhost:9092 |
| **Testbench shop API** | http://localhost:8000 |
| **Crypto demo backend** | http://localhost:3100 |
| **Crypto demo frontend** | http://localhost:3101 |

### Stopping and resetting

> **⚠️ Port conflict with the kind testbench cluster**
> The kind testbench (`bash scripts/testbench.sh`) binds host port **8080** for the
> Observable frontend ingress. Docker Compose binds **8083** for OpenFGA. These don't
> overlap, but if a stale kind cluster is running it may hold other ports too. Stop it
> before running `docker compose up -d`:
> ```bash
> kind delete cluster --name observable-test
> ```

```bash
# Stop all containers (keeps volumes)
docker compose down

# Stop and delete all data volumes (full reset)
docker compose down -v
# or
make reset-volumes
```

### Smoke tests

```bash
docker compose up smoke-test --abort-on-container-exit
# or
make smoke-test
```

### Kubernetes testbench (kind)

For end-to-end testing with real OTel traffic and the Observable frontend, use the kind testbench.

**First run — start the cluster once and keep it alive:**

```bash
# Builds images, deploys everything, keeps cluster after Ctrl+C
bash scripts/testbench.sh --keep-cluster
```

URLs once ready:
- Observable frontend: http://localhost:8080/
- Testbench shop: http://localhost:3000/

**Subsequent changes — hot-reload a single service in seconds:**

```bash
# Rebuild and redeploy only what changed
bash scripts/dev-reload.sh --service query-api
bash scripts/dev-reload.sh --service frontend
bash scripts/dev-reload.sh --service testbench-api

# Also run unit tests after reload
bash scripts/dev-reload.sh --service query-api --run-tests
```

The cluster keeps running between reloads. Docker layer caching makes rebuilds fast
(typically 5–30 s for Rust services, <10 s for Node.js testbench images).

Valid `--service` values: `query-api`, `stream-processor`, `ingest-gateway`,
`storage-writer`, `auth-service`, `alert-evaluator`, `backend` (all Rust),
`frontend`, `testbench-api`, `testbench-frontend`, `testbench-worker`,
`testbench-loadgen`, `testbench` (all testbench), `all` (default).

**Tear down when done:**

```bash
kind delete cluster --name observable-test
```

See [spec/10-process.md](spec/10-process.md) for the official development process and engineering standards.

## Demos

The `demos/` directory contains standalone example applications that showcase Observable in action.
Each demo is a self-contained app with its own README, Dockerfile entry in `docker-compose.yml`,
and OTel instrumentation that feeds pipeline-health metrics into the Observable platform.

| Demo | Description |
|------|-------------|
| [crypto-aggregator](demos/crypto-aggregator/README.md) | Real-time crypto live-data dashboard: ingests prices from DexPaprika and Coinbase, blockchain transactions from Blockchain.com, correlates them, and emits pipeline health as OTel metrics. |

## Related Projects

[Collectable](https://github.com/ktjn/collectable) is a standalone edge-pipeline tool that
compiles legacy log/metric sources (syslog, log4j2, MQTT, webhooks, etc.) into static Rust
binaries emitting OTLP. It has no runtime coupling to Observable and works with any
OTLP-compatible backend; it used to live in this repository but now has its own home.
