# WASM Full-Component Runtime Amendment

Date: 2026-08-24

## Status

This document amends `2026-08-21-github-pages-wasm-playground.md`.

The browser/WASM target is not a reduced or mocked implementation of Observable. The target is the same logical Observable component graph running inside one browser/WASM host, with infrastructure dependencies replaced by browser-compatible implementations.

The production architecture remains authoritative for production guarantees. Browser infrastructure provides weaker durability/process-isolation guarantees where the platform cannot reproduce them, but it must not bypass product components or replace product behavior with canned responses.

## Core Rule

**Reuse every Observable product component. Replace infrastructure, not components.**

The browser target must execute the same application/component logic for:

- ingest gateway;
- auth service;
- admin service;
- stream processor;
- storage writer;
- query API;
- alert evaluator;
- frontend;
- shared domain/query/processing/alert logic.

The browser may collapse process and network boundaries into in-process calls, but the logical component boundaries, validation, policies, contracts, and data ownership remain.

## Infrastructure Substitution Matrix

| Production infrastructure | Browser/WASM infrastructure | Requirement |
| --- | --- | --- |
| PostgreSQL | SQLite | All supported control-plane reads/writes use SQLite |
| ClickHouse | DuckDB | All telemetry reads/writes and analytical queries use DuckDB |
| Redpanda/Kafka | bounded in-memory transport | Preserve message contracts, ordering assumptions, fan-out and backpressure semantics needed by components |
| S3/object storage | browser-local blob/OPFS adapter | Only where a currently-used component requires object storage |
| OpenFGA | embedded/local authorization adapter | Auth service remains in the path; no frontend auth bypass |
| Zitadel/OIDC | embedded/local identity adapter | Auth service remains in the path; deterministic local principal is persisted/configured, not returned by a mock frontend handler |
| network HTTP/gRPC between local components | typed in-process transport | Preserve component API contracts; avoid serialization/network overhead when components share one WASM host |
| external webhook delivery | explicit local/unsupported delivery adapter | Alert evaluator still executes; never return fake delivery success |

SQLite and DuckDB may initially be in-memory databases. Persistence is a separate browser-storage concern and must not change component behavior.

## Required Browser Topology

```text
React frontend
      |
      v
Embedded service dispatcher
      |
      +--> auth-service ---------> SQLite
      |
      +--> admin-service --------> SQLite
      |
      +--> query-api ------------> DuckDB
      |          |
      |          +---------------> SQLite where production query behavior needs control-plane metadata
      |
      +--> ingest-gateway
                |
                v
          in-memory stream
                |
                v
          stream-processor
             /      \
            v        v
   in-memory stream  alert-evaluator
            |              |
            v              +----> SQLite for rule/SLO/config state
      storage-writer       +----> DuckDB for telemetry/query state as required
            |
            v
          DuckDB
```

The exact Rust call graph may differ from the diagram, but no supported frontend operation may skip its owning Observable component and read/write a database directly from the frontend runtime.

## No Mocked Observable Calls

Playground mode must not contain mocked Observable API responses.

Forbidden examples:

- `return demoUser` from a frontend auth adapter;
- returning static trace/log/metric fixtures from an API method;
- returning precomputed service summaries/topology;
- storing dashboards/alerts/SLOs only in React state;
- generating synthetic telemetry and passing it directly to the UI;
- bypassing the stream processor/storage writer when ingesting browser-generated data;
- returning fake success for webhook or external-provider operations.

Required behavior:

```text
frontend request
  -> owning Observable component
  -> infrastructure adapter(s)
  -> real local database/transport
  -> owning Observable component response shaping
  -> frontend
```

Unsupported capabilities must return an explicit capability/availability error.

## Data Ownership

### SQLite

Use SQLite as the browser replacement for PostgreSQL-owned state.

Examples include, according to existing production ownership:

- tenants/environments;
- users/local identity metadata;
- ingestion tokens/API keys;
- platform configuration;
- dashboards and saved views where currently control-plane owned;
- alert/SLO definitions;
- notification channel configuration;
- admin/member state;
- other relational metadata currently stored in PostgreSQL.

Use the same migrations/model semantics where practical. If SQL dialect differences require separate migrations, keep a shared logical schema/version and verify parity in tests.

### DuckDB

Use DuckDB as the browser replacement for ClickHouse-owned state.

Examples include:

- traces/spans;
- logs;
- metrics and derived metric samples;
- telemetry-derived service/topology queries;
- deployment/change telemetry where production ownership is ClickHouse;
- analytical materializations required by query/alert behavior.

`query-core` must produce semantic query plans that can be rendered for both ClickHouse and DuckDB. Do not translate arbitrary ClickHouse SQL with string rewriting.

### In-memory transport

The Redpanda replacement is an infrastructure adapter, not a shortcut.

The browser transport must preserve the component flow:

```text
ingest-gateway
  -> telemetry input topic/stream
  -> stream-processor
  -> downstream stream(s)
  -> storage-writer / alert-evaluator / other existing consumers
```

Use the existing message DTOs/envelopes where possible.

Required semantics:

- deterministic FIFO ordering where production code depends on partition ordering;
- bounded queues;
- explicit overflow/drop/backpressure policy;
- topic/channel fan-out matching current consumers;
- deterministic execution for tests and Scalable simulation;
- observability of queue depth/drop counters.

Do not call the stream processor or storage writer directly from the ingest source merely because all components share one process.

## Component Reuse Strategy

The preferred refactoring is **component core + host adapters**, not one new monolithic browser engine.

Conceptually:

```text
ingest-gateway-core
  native host: axum/tonic + Redpanda client
  wasm host: embedded ingest transport + in-memory stream

stream-processor-core
  native host: Redpanda consumer/producer
  wasm host: in-memory stream consumer/producer

storage-writer-core
  native host: Redpanda + ClickHouse
  wasm host: in-memory stream + DuckDB

query-api-core
  native host: Axum + ClickHouse/Postgres
  wasm host: embedded request dispatcher + DuckDB/SQLite

auth-service-core
  native host: Axum + Postgres/OpenFGA/Zitadel integration
  wasm host: embedded dispatcher + SQLite/local auth adapters

admin-service-core
  native host: Axum + Postgres
  wasm host: embedded dispatcher + SQLite

alert-evaluator-core
  native host: Redpanda/ClickHouse/Postgres + webhook adapters
  wasm host: in-memory stream/DuckDB/SQLite + local delivery adapter
```

Existing service crates may remain the ownership boundary. Prefer exposing portable library targets from the existing service crates over copying code into browser-specific crates.

`playground-wasm` should become a composition/binding layer only. It must not own duplicate query, ingest, alert, auth, or storage behavior.

## Embedded Request Boundary

The frontend should continue to call the same logical APIs.

Production:

```text
React -> HTTP -> service
```

Browser:

```text
React -> worker RPC -> embedded service dispatcher -> service core
```

The browser dispatcher may use typed Rust request/response DTOs instead of real HTTP frames. It must preserve:

- request validation;
- auth/tenant/environment context;
- endpoint ownership;
- response schema;
- error semantics;
- read/write side effects.

If an existing Axum router can be reused under WASM without pulling unsuitable native dependencies, in-process `tower::Service` invocation is acceptable. Otherwise extract the application handler below Axum and invoke that same handler from both hosts.

Do not implement a parallel set of browser handlers.

## Synthetic and Imported Telemetry

Synthetic data is allowed only as an input producer.

Correct path:

```text
generator/importer
  -> embedded ingest-gateway
  -> in-memory Redpanda adapter
  -> stream-processor
  -> storage-writer
  -> DuckDB
  -> query-api
  -> frontend
```

The UI must never receive generated fixtures directly.

The same applies to imported OTLP payloads and telemetry produced by an embedded application.

## Scalable Integration

`ktjn/scalable` should be the first external embedded producer.

The integration must not write directly to DuckDB and must not bypass Observable's ingest/processing pipeline.

Target path:

```text
Scalable runtime
  -> Scalable TelemetrySink
  -> scalable-observability-observable adapter
  -> Observable embedded ingest-gateway
  -> Observable in-memory stream
  -> Observable stream-processor
  -> Observable storage-writer
  -> DuckDB
  -> Observable query-api
  -> Observable React UI
```

For two Rust components linked into one WASM artifact, do not encode/decode OTLP protobuf merely to cross an in-process boundary. Map Scalable's typed telemetry into the canonical internal ingest envelope accepted by ingest-gateway after transport decoding, while still executing ingest-gateway authentication/context/validation/stamping logic that is relevant to embedded mode.

OTLP remains the external/native interoperability path:

```text
native Scalable -> OTLP -> remote Observable
embedded Scalable -> typed in-process ingest -> embedded Observable
```

Add parity tests proving that equivalent native-OTLP and embedded-ingest inputs normalize to equivalent Observable telemetry.

## Browser Composition Root

The final WASM artifact should instantiate the real components once:

```text
ObservableWasmRuntime
  sqlite
  duckdb
  in_memory_stream
  local_identity
  local_authorizer
  ingest_gateway
  auth_service
  admin_service
  stream_processor
  storage_writer
  query_api
  alert_evaluator
```

All dependencies are injected explicitly.

No component accesses global browser state directly. Browser APIs belong in host adapters.

The standalone Observable playground and the Scalable playground should use the same `ObservableWasmRuntime` composition code. Scalable adds a producer/runtime beside it; it does not fork Observable.

## Testing Requirements

### Component parity

For every service component, run the same core tests against native and embedded adapters where meaningful.

### Infrastructure contract tests

At minimum:

```text
Postgres repository contract <-> SQLite repository contract
ClickHouse query/storage contract <-> DuckDB query/storage contract
Redpanda message-flow contract <-> in-memory transport contract
```

The contract suite should compare semantic behavior, not vendor-specific metadata.

### End-to-end browser gate

The browser E2E test must prove data traverses all components.

Example trace test:

1. reset SQLite/DuckDB;
2. ingest a deterministic trace through ingest-gateway;
3. assert input stream receives it;
4. run stream processor;
5. assert storage-writer consumes processed output;
6. query DuckDB directly in the test and verify persisted rows;
7. call query-api through the embedded request boundary;
8. render the existing Traces UI;
9. assert the trace is visible;
10. assert no network call to an Observable backend occurred.

Equivalent tests are required for logs, metrics and one mutable control-plane feature.

### No-mock gate

Add a static/test guard preventing playground production code from importing test fixtures or mock API modules.

Playwright route interception remains valid in frontend tests, but the deployed playground runtime itself must not contain a mock transport.

## Implementation Order

1. Define infrastructure ports for Postgres-owned storage, ClickHouse-owned storage and Redpanda messaging at existing component boundaries.
2. Keep native PostgreSQL/ClickHouse/Redpanda adapters unchanged behind those ports.
3. Implement SQLite repository adapters and contract tests.
4. Implement DuckDB storage/query adapters and contract tests.
5. Implement deterministic bounded in-memory stream adapter and contract tests.
6. Extract/confirm portable component cores for ingest-gateway, stream-processor and storage-writer.
7. Build the first full telemetry slice: ingest-gateway -> stream-processor -> storage-writer -> DuckDB -> query-api.
8. Move auth-service/admin-service to SQLite-backed embedded adapters; remove any playground auth/config mocks.
9. Move alert-evaluator to the same embedded graph.
10. Route all frontend playground calls through the embedded service dispatcher.
11. Route synthetic/imported telemetry through ingest-gateway only.
12. Integrate Scalable through a typed producer adapter into ingest-gateway.
13. Add parity/E2E/no-mock gates.
14. Add optional browser persistence only after the in-memory full-stack runtime is stable.

## Definition of Done

The WASM/browser target is complete when:

- every Observable product component required by the existing frontend is instantiated and executes its real application logic;
- PostgreSQL-owned state is backed by SQLite;
- ClickHouse-owned telemetry/query state is backed by DuckDB;
- Redpanda-owned messaging is backed by deterministic bounded in-memory transport;
- no deployed playground operation returns mocked/canned Observable data;
- generated/imported/Scalable telemetry traverses ingest-gateway, stream-processor and storage-writer before becoming queryable;
- query-api reads DuckDB/SQLite rather than fixture state;
- admin/auth/control-plane operations read/write SQLite rather than React memory or mocks;
- alert evaluation runs against the embedded databases/message flow;
- the standalone Observable playground and Scalable embedding use the same Observable WASM composition/runtime;
- production and embedded infrastructure adapters pass shared semantic contract tests;
- browser E2E tests prove the complete path without an Observable backend server.
