# Observable Platform

Full-stack observability platform specification.

## Documentation

The full specification is located in the [spec/](spec/) directory.

Implementation plans and iteration documents can be found in [docs/superpowers/plans/](docs/superpowers/plans/).
Historical plans are archived in [archived/plans/](archived/plans/).

## AI Agent Instructions

Mandatory instructions for any AI agent interacting with this repository can be found in [AGENTS.md](AGENTS.md).
(The legacy [AGENT.md](AGENT.md), [GEMINI.md](GEMINI.md), and [CLAUDE.md](CLAUDE.md) files are now pointers to the canonical instructions).

## Development

The entire stack can be started with Docker Compose. This will build the services, run migrations, and start the system.

```bash
# Start the full local stack
docker compose up -d

# Open the frontend
# http://localhost:5173

# Run smoke tests
docker compose up smoke-test --abort-on-container-exit
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
