# Test Strategy

## 18. Test Strategy

### 18.1 Testing Layers

**Unit**
- parsers
- query builders
- auth policies
- retention logic
- alert evaluators
- UI state reducers/hooks

**Contract**
- OTLP compatibility
- API schema compatibility
- SDK compatibility
- auth provider compatibility

**Integration**
- ingest → queue → storage
- query → storage pushdown
- alert → notification
- tenant isolation
- collector/agent interoperability

**End-to-end**
- service emits telemetry
- telemetry visible in UI
- alert fires
- incident created
- deep links resolve
- dashboard renders
- RBAC enforced

**Performance**
- ingest throughput
- query latency
- high-cardinality stress
- compaction impact
- backpressure behavior
- cold-tier restore

**Chaos/resilience**
- broker outage
- storage node loss
- region loss
- partial auth outage
- queue lag surge
- clock skew
- duplicate ingest

**Security**
- auth bypass
- tenant escape
- injection on query DSL
- SSRF in integrations
- PII leak tests
- secret exposure
- supply chain gates

### 18.2 Test Data Strategy

- synthetic telemetry generators
- replay corpus from known workloads
- golden traces/log bundles
- high-cardinality torture datasets
- malformed payload corpus

### 18.3 Non-Functional Acceptance Targets

| Metric | Target |
|--------|--------|
| ingest availability | 99.95% |
| query API availability | 99.9% |
| P50 query latency (hot path) | < 1s |
| P95 common dashboard load | < 3s |
| alert detection latency | < 60s |
| tenant cross-read | zero in all policy tests |
| data loss | zero for committed buffered ingest under single-node failure |
