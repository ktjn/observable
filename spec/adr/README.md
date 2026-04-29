# Architecture Decision Records

Architecture Decision Records (ADRs) capture significant technology and design choices made in the Observable platform. Each ADR documents the context, the decision, its consequences, and alternatives considered.

**To create a new ADR:** copy `ADR-000-template.md`, assign the next available number, fill in all sections, and open a PR. Update the status column and the decision summary below when merged.

**Agent guidance:** scan the Decision column below to identify ADRs relevant to your task, then read those files in full before writing code.

| ADR | Title | Status | Decision (one line) |
|-----|-------|--------|---------------------|
| [ADR-001](ADR-001-otel-external-contract.md) | OTel as External Contract | Accepted | OTLP is the only ingest format; spec floor 1.0.0, target latest stable; quarterly crate update cadence |
| [ADR-002](ADR-002-polyglot-storage.md) | Polyglot Storage vs Single Engine | Accepted | ClickHouse for telemetry, PostgreSQL for control-plane metadata; no single unified store |
| [ADR-003](ADR-003-clickhouse-boundary.md) | ClickHouse Adoption Boundary | Accepted | ClickHouse is used only for append-only telemetry reads/writes; no OLTP or auth data |
| [ADR-004](ADR-004-rust-data-plane.md) | Rust for Data Plane Services | Accepted | All ingest, processing, storage, and query services are written in Rust |
| [ADR-005](ADR-005-arrow-datafusion.md) | Arrow/DataFusion Query Layer | Accepted | Arrow/DataFusion is the query execution layer for analytics; ClickHouse is the storage backend |
| [ADR-006](ADR-006-react-vite-frontend.md) | React/Vite Frontend | Accepted | React 19 + Vite + TanStack Query/Router for the frontend; no Next.js or SSR framework |
| [ADR-007](ADR-007-multi-tenant-isolation.md) | Multi-Tenant Isolation Strategy | Accepted | tenant_id column on every telemetry table; enforced at query layer, not DB-level separation |
| [ADR-008](ADR-008-authorization-model.md) | Authorization Model | Accepted | OpenFGA (Zanzibar-model) for RBAC; PostgreSQL-backed API key store hashed with SHA-256 |
| [ADR-009](ADR-009-queue-stream-backbone.md) | Queue/Stream Backbone | Accepted | Redpanda is the only inter-service queue; no direct service-to-service HTTP for telemetry data |
| [ADR-010](ADR-010-deployment-model.md) | Deployment Model (k8s-first) | Accepted | Kubernetes is the target deployment platform; Docker Compose is for local dev only |
| [ADR-011](ADR-011-sampling-strategy.md) | Sampling Strategy | Accepted | Tail-based sampling at the ingest gateway; head-based sampling is the client's responsibility |
| [ADR-012](ADR-012-retention-tiering.md) | Retention and Tiering | Accepted | ClickHouse TTL for hot tier; S3-compatible object storage for cold tier |
| [ADR-013](ADR-013-schema-governance.md) | Schema Governance | Accepted | Versioned SQL migration files under `migrations/`; no ORM-generated schema changes |
| [ADR-014](ADR-014-ai-feature-boundaries.md) | AI Feature Boundaries | Accepted | AI features are read-only assistants; no AI-initiated writes or alerts without human approval |
| [ADR-015](ADR-015-build-vs-buy.md) | Build vs Buy (Incident/Auth/Billing) | Accepted | Buy/integrate for incident management, billing, and IdP; build only the observability core |
| [ADR-016](ADR-016-grafana-visualization-strategy.md) | Grafana Visualization Strategy | Accepted | Grafana is supported as an optional visualization layer via datasource plugins; not the default UI |
| [ADR-017](ADR-017-prometheus-remote-write.md) | Prometheus remote_write Compatibility | Accepted | Ingest gateway accepts Prometheus remote_write in addition to OTLP |
| [ADR-018](ADR-018-rum-browser-mobile-ingestion.md) | RUM / Browser and Mobile Ingestion | Accepted | Browser/mobile SDKs send to a dedicated RUM endpoint; not the same path as backend OTLP |
| [ADR-019](ADR-019-ci-scripts-runnable-locally.md) | CI Scripts Runnable Locally | Accepted | All non-trivial CI logic lives in `scripts/`; migrations use `docker compose exec` (no host DB clients) |
| [ADR-020](ADR-020-helm-chart-strategy.md) | Helm Chart Strategy | Accepted | Helm v3 with library + umbrella chart pattern; kind for local k8s integration testing |
| [ADR-021](ADR-021-nl-query-layer.md) | LLM Natural Language Query Layer | Proposed | LLM advisory query layer on top of existing query substrate; cross-signal triangulation; provenance required |
| [ADR-022](ADR-022-collectable-mediator.md) | Collectable — Compiled-Mediator Tool | Accepted | Build Collectable as an independent compiled-mediator tool; Observable ingest gateway remains OTLP-only |
| [ADR-023](ADR-023-standard-otlp-ports.md) | Standard OTLP Port Conformance | Proposed | Align ingest-gateway to OTLP standard ports: 4317 gRPC, 4318 HTTP/JSON |
| [ADR-024](ADR-024-deployment-marker-routing.md) | Deployment Marker Write Path in Ingest Gateway | Accepted | POST/PATCH /v1/deployments in ingest-gateway; GET in query-api; future Redpanda stream enables SSE push |
| [ADR-025](ADR-025-testcontainers-integration-tests.md) | Testcontainers for Service Integration Tests | Proposed | Use Testcontainers for isolated real-dependency service integration tests; Compose/kind remain full-stack gates |
| [ADR-026](ADR-026-no-proprietary-query-dsl.md) | No Proprietary Query DSL | Accepted | Observable will never introduce a proprietary DSL; SQL is the canonical IR; NLQ is the operator UX; PromQL is an optional metrics-only façade |
| [ADR-027](ADR-027-local-llm-backend.md) | Local LLM Backend (vLLM) | Proposed | vLLM as opt-in local backend for NLQ; Phi-3 Mini default, Llama-3 8B alternative; reuses async-openai with no auth key; OpenAI API remains default |
