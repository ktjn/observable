# Crypto Live-Data Demo

A real-time dashboard that ingests live cryptocurrency prices and blockchain
transactions, correlates them, and visualises the result in a modern web UI.
Pipeline health is emitted as OpenTelemetry metrics to the Observable
[ingest-gateway](../../services/ingest-gateway) so the Observable platform
itself can observe how well the demo pipeline is performing.

---

## Architecture

```
DexPaprika REST   ──┐
Coinbase WS       ──┤──► Normalizer ──► Correlator ──► SSE /events ──► Frontend UI
Blockchain.com WS ──┘                       │
                                           └──► OTel Metrics ──► ingest-gateway:4317 ──► Observable
```

| Layer | Responsibility |
|-------|---------------|
| **Frontend UI** | Shows live prices, transactions, correlations, and lineage |
| **Observable** | Shows pipeline health via OTel metrics from `crypto-demo-pipeline` |

---

## Data Sources

| Source | Protocol | What it provides |
|--------|----------|-----------------|
| [DexPaprika](https://dexpaprika.com) | REST (polling) | On-chain DEX token prices (WBTC, WETH, SOL) |
| [Coinbase Advanced Trade](https://docs.cdp.coinbase.com/advanced-trade/docs/ws-overview) | WebSocket | Spot prices for BTC, ETH, SOL |
| [Blockchain.com](https://www.blockchain.com/api/api_websocket) | WebSocket | Unconfirmed BTC transactions |

---

## Data Models (modelable)

Models are defined in [`../../models/crypto.mdl`](../../models/crypto.mdl) using
the [modelable](https://github.com/ktjn/modelable) compiler.

| Model | Description |
|-------|-------------|
| `PriceEvent` | A price tick from DexPaprika or Coinbase |
| `TxEvent` | An unconfirmed blockchain transaction |
| `CorrelatedEvent` | A paired price+tx event produced by the Correlator |
| `PipelineMetrics` | Pipeline health snapshot (ingest rate, lag, buffer, errors) |

Generated TypeScript types live in `backend/src/generated/` and
`frontend/src/generated/`. Regenerate after editing `crypto.mdl`:

```bash
cd ../../models
uv run modelable compile crypto.mdl --target typescript --out ../demos/crypto-aggregator/backend/src/generated
uv run modelable compile crypto.mdl --target typescript --out ../demos/crypto-aggregator/frontend/src/generated
uv run modelable compile crypto.mdl --target markdown   --out ../demos/crypto-aggregator/frontend/public
```

---

## OTel Instrumentation

The backend emits these metrics to Observable under service name
`crypto-demo-pipeline` and tenant `crypto-demo`:

| Metric | Unit | Description |
|--------|------|-------------|
| `pipeline.ingest_rate` | events/s | Events received across all adapters |
| `pipeline.correlation_lag_ms` | ms | Latest price→tx lag |
| `pipeline.buffer_fill_ratio` | 0–1 | Correlator window fill level |
| `pipeline.exporter_latency_ms` | ms | OTel export round-trip |
| `pipeline.error_count` | count | Cumulative normalisation errors |

The tenant and API key are seeded by
[`migrations/postgres/033_add_crypto_demo_tenant.sql`](../../migrations/postgres/033_add_crypto_demo_tenant.sql).

---

## Running Locally

### With Docker Compose (recommended)

The image is built locally from source — no registry login required. The first run
builds the image automatically; subsequent starts reuse the Docker layer cache.

```bash
# From the repository root — starts only the crypto-demo and its dependencies
docker compose up crypto-demo -d

# Force a rebuild after pulling new code
docker compose up crypto-demo -d --build

# Frontend: http://localhost:3101
# Backend API: http://localhost:3100
```

The full stack (including Observable and all other services) is started with:

```bash
docker compose up -d
```

### Without Docker (development)

```bash
# Backend
cd backend
npm install
npm run dev          # starts on :3100

# Frontend (separate terminal)
cd frontend
npm install
npm run dev          # starts on :3101, proxies /events and /metrics to :3100
```

---

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `3100` | Backend HTTP port |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | `http://localhost:4317` | OTel Collector gRPC endpoint |
| `OTEL_EXPORTER_OTLP_HEADERS` | *(empty)* | Auth header, e.g. `authorization=Bearer <key>` |
| `OTEL_SERVICE_NAME` | `crypto-demo-pipeline` | OTel service name |
| `DEXPAPRIKA_BASE_URL` | `https://api.dexpaprika.com` | Override DexPaprika REST base URL |
| `COINBASE_WS_URL` | Coinbase public WSS URL | Override WebSocket endpoint |
| `BLOCKCHAIN_WS_URL` | Blockchain.com public WSS URL | Override WebSocket endpoint |

---

## Testing

### Frontend Playwright E2E

```bash
cd frontend
npx playwright install chromium   # first time only
npm run test:e2e
```

Tests use mocked SSE and `/metrics` responses — no running backend required.

### Backend Type Checking

```bash
cd backend
npm run typecheck
```

---

## Project Structure

```
demos/crypto-aggregator/
├── backend/               # Node.js/TypeScript ingest pipeline
│   └── src/
│       ├── ingest/        # DexPaprika, Coinbase, Blockchain.com adapters
│       ├── normalize/     # Maps raw events → typed PriceEvent | TxEvent
│       ├── correlate/     # 5-second sliding-window correlator
│       ├── otel/          # OpenTelemetry metrics setup
│       ├── server.ts      # Express SSE + /metrics + /health
│       ├── generated/     # modelable-generated TypeScript types
│       └── index.ts       # Bootstrap
├── frontend/              # React + Vite + TailwindCSS + D3
│   └── src/
│       ├── components/    # PriceTicker, TxList, CorrelationScatter,
│       │                  #   LineageDiagram, PipelineHealth
│       ├── hooks/         # useEventStream (SSE subscription)
│       ├── generated/     # modelable-generated TypeScript types
│       └── App.tsx        # 4-section responsive dashboard
│   ├── public/            # modelable markdown schema docs
│   ├── tests/e2e/         # Playwright E2E tests
│   └── playwright.config.ts
├── Dockerfile             # Multi-stage: Node backend + nginx frontend
├── nginx.conf             # Nginx config: static + SSE proxy
├── start.sh               # Container entrypoint
└── README.md              # This file
```
