# Architecture

## 3. Reference Architecture

### 3.1 Top-Level Domains

**Control plane**
- tenant/account management
- authn/authz
- billing/quotas
- schema/catalog
- configuration
- deployment/orchestration metadata
- alert definitions
- SLO definitions
- feature flags
- audit logs

**Data plane**
- ingest gateways
- stream processing
- enrichment
- storage writers
- query APIs
- alert evaluation
- correlation engine
- ML/anomaly jobs
- RUM edge collectors

**UX plane**
- React web app
- query workbench
- dashboards
- admin console
- setup/onboarding
- incident console

---

## 4. Core Technical Architecture

### 4.1 Ingestion

**Required interfaces**
- OTLP/gRPC
- OTLP/HTTP
- Prometheus remote_write receiver
- OpenTelemetry Collector-compatible export target
- log shipper integrations
- browser beacon intake
- mobile SDK intake
- synthetic check intake

**Ingestion pipeline stages**
1. authn/authz
2. tenant routing
3. validation
4. normalization
5. sampling/filtering
6. enrichment
7. cardinality enforcement
8. durable buffering
9. fan-out to storage/index/materialization
10. ingest acknowledgements and metrics

**Design rules**
- Stateless ingest edge.
- Durable queue before expensive transforms.
- Exactly-once is not mandatory; at-least-once + idempotent write design is acceptable.
- Backpressure must degrade gracefully:
  - drop debug logs before traces
  - reduce exemplars before metrics
  - apply tail-sampling policy changes under load

### 4.2 Processing

**Hot path** (sub-second to a few seconds)
- validation
- enrichment
- routing
- indexing
- alert materialization

**Warm path** (seconds to minutes)
- service topology
- RED/USE aggregates
- trace-derived metrics
- deployment correlation
- cardinality analysis

**Cold path** (minutes to hours)
- long-range downsampling
- anomaly models
- retention tiering
- compaction
- cost reports

### 4.3 Query

Expose separate logical APIs:
- trace query API
- metric query API
- log query API
- profile query API
- cross-signal query API
- topology API
- alert/SLO API
- configuration API

Provide a unified query facade in the UI and SDK.
