# 22 — Formal System Specification & Brownfield Onboarding

This document records the reverse-discovery onboarding of `observable` (`0.1.0`) into the machine-checkable System Specification metamodel ([spec/system.yaml](file:///c:/git/observable/spec/system.yaml)).

---

## 1. Machine-Checkable Specification

- **Canonical File**: [spec/system.yaml](file:///c:/git/observable/spec/system.yaml)
- **Validator**: `metamodel.py spec/system.yaml --stage release`
- **Bounded Context Slice**: `metamodel.py spec/system.yaml --context <component-id>`
- **Change Impact Analysis**: `metamodel.py spec/system.yaml --impact <entity-id>`

---

## 2. Reverse-Discovery Architecture Mapping

| Component | Layer | Consistency | Provides | Requires | Owns Data | Source Paths |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `observable-web` | `experience` | `strong` | `explore-telemetry-ui` | `validate-credential@^0.1`, `manage-control-plane@^0.1`, `query-telemetry@^0.1`, `evaluate-alerts@^0.1` | — | `apps/frontend` |
| `observable-ingest` | `application` | `eventual` | `ingest-otlp` | `validate-credential@^0.1`, `normalize-telemetry@^0.1`, `publish-deployment-markers@^0.1` | — | `services/ingest-gateway` |
| `observable-query` | `application` | `strong` | `query-telemetry` | `validate-credential@^0.1`, `manage-control-plane@^0.1`, `evaluate-alerts@^0.1`, `store-telemetry@^0.1` | — | `services/query-api`, `libs/query-core`, `libs/domain-core` |
| `observable-control` | `domain` | `strong` | `manage-control-plane`, `publish-deployment-markers` | `validate-credential@^0.1` | `tenant-config`, `dashboard-model`, `deployment-marker`, `query-audit-entry` | `services/admin-service` |
| `observable-alerting` | `domain` | `strong` | `evaluate-alerts` | `validate-credential@^0.1`, `store-telemetry@^0.1` | `alert-rule`, `slo-definition`, `incident-record` | `services/alert-evaluator` |
| `observable-process` | `domain` | `eventual` | `normalize-telemetry` | `store-telemetry@^0.1` | — | `services/stream-processor`, `libs/observable-process` |
| `observable-auth` | `infrastructure` | `strong` | `validate-credential` | — | `api-key-credential`, `user-session` | `services/auth-service`, `libs/observable-auth` |
| `observable-store-clickhouse` | `infrastructure` | `eventual` | `store-telemetry` | — | `span-row`, `log-row`, `metric-point` | `services/storage-writer`, `migrations/clickhouse`, `libs/observable-storage-contracts` |

---

## 3. Architectural Findings & Model Feedback

Reverse discovery against the metamodel invariants uncovered three concrete architectural hazards in the brownfield decomposition:

1. **`COMP-05` / `COMP-06` Hazard in Phase 6 (`observable-query` ↔ `observable-alerting`)**:
   - `observable-query` (`application`) currently calls `observable-alerting` (`domain`) over `/internal/alerting-enrichment` and `/internal/alerting-correlation` (`requires: [evaluate-alerts@^0.1]`), while `observable-alerting` reads ClickHouse directly (`requires: [store-telemetry@^0.1]`).
   - Rewiring `observable-alerting` to require `query-telemetry` while `observable-query` still requires `evaluate-alerts` would introduce a **two-node component dependency cycle (`COMP-06`)** and a **layer violation (`COMP-05`: `domain -> application`)**.
   - Tracked via `risk-query-alerting-cycle-hazard` and `req-alerting-query-inversion`: alert enrichment must be inverted (pushed or queried by the caller) before `observable-alerting` can depend on `query-telemetry`.
2. **At-Least-Once Crash Window Duplicate Delivery (`risk-at-least-once-duplicates`, issue #768)**:
   - `observable-store-clickhouse` commits Kafka offsets after ClickHouse insert; a crash between insert and offset commit redelivers batches into `MergeTree` tables (`spans`, `logs`, `metric_points`), and `observable-process` still pushes span-derived metrics over HTTP `/internal/metrics`.
   - Tracked via `req-dedup-idempotency`.
3. **PostgreSQL Cross-Schema Foreign Keys (`risk-pg-cross-schema-fks`)**:
   - `auth.*` and `control.*` retain legacy cross-schema foreign keys (`auth.* -> control.tenants`, `control.* -> auth.users`) and `public.query_audit_log` lacks an explicit component schema prefix.
   - Tracked via `req-pg-schema-isolation`.

---

## 4. Gate Decision Record (Levels 0 → 3)

| Gate | Stage | Status | Evidence Summary |
| :--- | :--- | :--- | :--- |
| Level 0 | `inventory` | **PASS** | All 8 components, 9 capabilities, 12 data models, 15 requirements, 3 risks, and 15 evidence records resolve cleanly (`REF-01`–`REF-13`, `DATA-01`, `EV-01`–`EV-03`, `RISK-01`–`RISK-02`). |
| Level 1 | `discovery` | **PASS** | All 9 capabilities define explicit preconditions, postconditions, invariants, and failure modes (`STAT-01`, `CAP-01`). |
| Level 2 | `architecture` | **PASS** | Strict 4-layer DAG (`experience -> application -> domain -> infrastructure`), single data-model ownership (`DATA-02`, `DATA-03`), PII declassification (`DATA-04` via `req-pii-scrubbing`), and full interface contracts (`CAP-02`, `CAP-03`, `COMP-01`–`COMP-07`). |
| Level 2.5 | `verification` | **PASS** | All 10 verification categories covered (`REQ-01`), category-kind alignment verified (`REQ-07`), acyclic requirement graph (`REQ-02`). |
| Level 3 | `release` | **PASS** | All obligated and risk-mitigating requirements backed by passing evidence with matching `subject_revision: "0.1.0"`, `timestamp`, and `verifier` (`EV-04`, `REQ-04`–`REQ-06`, `RISK-03`–`RISK-08`, `STAT-04`). |

---

## 5. Remediation Roadmap to Full Runtime Alignment

1. **Milestone 1 — Spec-Driven CI & Drift Enforcement** (Completed):
   - `scripts/tests/test_system_spec.py` enforces parity between `spec/system.yaml`, `VERSION`, `distribution/manifest.yaml`, `source_paths`, and `models/*.mdl` (`schema_path`).
2. **Milestone 2 — Telemetry Pipeline Idempotency & Single-Boundary Flow** (Completed):
   - Closed `risk-at-least-once-duplicates` (`req-dedup-idempotency`, issue `#768`) by attaching deterministic ClickHouse `insert_deduplication_token` / `non_replicated_deduplication_window` to all telemetry inserts (`store-telemetry.interface.idempotent: true`), routing span-derived metrics through `telemetry.normalized.v1` and removing `/internal/metrics`, and verifying `NoDuplicateVisibleRows` in `formal/tla/TelemetryDelivery.tla`.
3. **Milestone 3 — PostgreSQL State Ownership & Schema Isolation** (Completed):
   - Closed `risk-pg-cross-schema-fks` (`req-pg-schema-isolation`) by migrating `query_audit_log` into `control.query_audit_log` (`query-audit-entry` owned by `observable-control`), dropping all cross-schema foreign keys between `auth.*` and `control.*`, and provisioning schema-scoped PostgreSQL roles (`observable_auth_role`, `observable_control_role`, `observable_alerting_role`) in `migrations/postgres/041_drop_cross_schema_fks_and_assign_query_audit_log.sql`, verified via Testcontainers Postgres integration tests.
4. **Milestone 4 — Resolve `observable-query` ↔ `observable-alerting` Direction** (Completed):
   - Closed `risk-query-alerting-cycle-hazard` (`req-alerting-query-inversion`) by codifying the acyclic `observable-query (application) -> observable-alerting (domain) -> observable-store-clickhouse (infrastructure)` boundary (`COMP-05` & `COMP-06`) in `docs/component-decomposition.md`, adding `services/alert-evaluator/clickhouse-schema-support.yaml` enforced by `scripts/check-clickhouse-schema-compat.sh`, and testing `COMP-05` / `COMP-06` in `scripts/tests/test_system_spec.py`.
5. **Milestone 5 — Contract Completeness & Boundary Allowlist Zeroing** (Completed):
   - Emptied `scripts/component-boundary-allowlist.txt` (0 allowlisted entries), moved API-key audit tests directly into `services/auth-service/tests/http_api_integration.rs`, removed `auth-service` dev-dependency from `services/query-api/Cargo.toml`, and verified zero boundary violations with `scripts/check-component-boundaries.sh`.
6. **Milestone 6 — Automated Evidence Provenance & Release Gate**:
   - Automate `Evidence` digest/timestamp stamping from `scripts/local-ci.sh` and bind `subject_revision` to immutable per-component OCI release artifacts.
