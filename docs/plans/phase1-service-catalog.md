# Phase 1 detailed plan: Service model (`observable-catalog`)

**Status:** S1–S8 merged. S9 remains open while the closure slices and accountable inspections are completed.

**S9 review (2026-11-08):** [Exit review and inspection workbook](phase1-exit-review.md).
S1–S8 are merged. Phase exit remains blocked at 6/15 verified, pending six accountable
inspections and three platform requirements. The table below records the original planning
baseline; the gap register and S9 review contain current readiness.
**Inputs:** [ROADMAP.md](../../ROADMAP.md) phase 1, [business-spec-adoption.md](business-spec-adoption.md), [phase0-assurance-hardening.md](phase0-assurance-hardening.md), ADR-038 (catalog), ADR-041 (decide-access), ADR-042 (domains), [COMPONENT-ISOLATION.md](../../system-spec/COMPONENT-ISOLATION.md).
**Branch policy:** every slice is one PR into `2.0`, one component per PR where possible, `bash scripts/spec-gate.sh` green, change flow per `system-spec/FEATURE.md`.

## 1. Scope and exit

Phase 1 must verify 15 P0 requirements:

| ID | Holder | Gap today | Verification method |
|---|---|---|---|
| OBS-01-01 identify services and critical journeys | organization | missing | inspection, supported by catalog records and gap report |
| OBS-01-02 business outcome per in-scope service | organization | missing | inspection, supported by outcome records and exception list |
| OBS-01-03 production / non-production / business-critical contexts | platform | partial | test |
| OBS-01-04 assess within agreed expectations | platform | partial | test (see Q1: depends on phases 2 and 4) |
| OBS-02-01 find service by business name, operational name, domain, owner | platform | partial | test |
| OBS-02-02 business owner, operational owner, support route | platform | missing | test |
| OBS-02-03 visible coverage status | platform | missing | test |
| OBS-02-04 criticality and lifecycle | platform | missing | test |
| OBS-27-01 agreed minimum coverage per critical service | organization | missing | inspection, supported by coverage policy records |
| OBS-27-02 required vs demonstrated coverage | platform | missing | test |
| OBS-27-04 gaps have remediation owner, date, risk acceptance | organization | missing | inspection, supported by gap workflow tests |
| OBS-34-01 every alert, objective, dashboard, check, health rule has an owner | organization | partial | inspection, supported by asset registry tests |
| OBS-36-01 material providers with dependent services, functions, contracts | organization | missing | inspection, supported by provider records |
| `req-decide-access-fail-closed` | platform (security) | missing | test |
| `req-catalog-access-control` | platform (security) | missing | test |

Exit evidence (ROADMAP): catalog maps OTel services; coverage status visible; the 15 requirements `verified` under `scripts/spec-readiness.py --priority P0 --strict`; UX-01 negatives `ux-01-restricted-service` and `ux-01-search-service-restricted` pass in a browser test; `bash scripts/spec-gate.sh` green.

The closure slices in section 11 bring forward the health, objectives and catalog identity integration needed by the Phase 1 exit criteria. The rest of the Phase 2 health and Phase 4 objectives work stays in those phases. Unregistered-service detection beyond a simple list (OBS-02-07, P1), reconciliation with external CMDBs (OBS-02-08, P1) and dependency mapping (OBS-10, phase 7) remain out of scope.

## 2. Component placement

| Item | Decision |
|---|---|
| Component | `observable-catalog`, layer `domain`, domain `service-model`, new crate `services/catalog-service` (Rust, axum, sqlx), PostgreSQL schema `catalog`, role `observable_catalog_role` |
| Provides | `manage-service-catalog`, `govern-observability-assets`, `manage-providers` (`map-dependencies-and-impact` stays unprovided until phase 7) |
| Requires | `validate-credential@^0.1`, `decide-access@^0.1`, `lookup-principals@^0.1` (new, observable-auth), `store-telemetry@^0.1` (observed service names, signal presence), `manage-control-plane@^0.1` (environments and their class) |
| Allowed by ADR-042 | `service-model` uses identity, service-control, telemetry-storage. No call into `investigation` or `incident-response`. |
| Asset ownership flow | Asset owners (alerting, control, later health and response) **push** registrations to the catalog (`register-observability-asset`). The catalog never reads alerting state, which would break the domain rule. |
| Web | New frontend module `catalog` under ADR-036 module rules; consumes catalog API only through the runtime contract. |

### New-component onboarding checklist (S1)

Adding a runtime component touches these enumerations; S1 updates all of them and the policy tests that pin them:

- `Cargo.toml` workspace members; `services/catalog-service/{Cargo.toml,CHANGELOG.md}`.
- `distribution/manifest.yaml` (`catalog: 0.1.0`), `scripts/check-distribution-manifest.sh`.
- `Dockerfile` target, `scripts/build-component-images.sh`, `scripts/check-component-images.sh`, `.github/workflows/release-artifacts.yml`.
- `docker-compose.yml`, `scripts/local-stack.sh`, `scripts/render-pinned-compose.sh`, `scripts/dev-reload.sh`.
- `charts/observable` templates and values; `scripts/kind-test.sh`, `scripts/collect-kind-logs.sh`.
- `scripts/check-component-boundaries.sh`, `scripts/ci_changed_components.py`, `scripts/check-component-changelogs.sh`.
- `scripts/tests/test_system_spec.py` (`MANIFEST_TO_SPEC_COMPONENT`), `test_component_versions.py`, `test_component_changelogs.py`, other `test_component_*` that enumerate components.
- `migrations/postgres/0NN_create_catalog_schema.sql` with schema-scoped role and login (pattern of 040-042).
- `tests/e2e/smoke_test.sh`, `scripts/release-candidate-suites.sh`, `dashboards/observable-self-health.json`.
- `spec/system.yaml` component block with `source_paths`, `test_paths`, `fakes`, `provided_mocks`, `standalone_harness`.

## 3. Data model (`catalog` schema, all rows carry `tenant_id`)

| Model | Key fields | Requirements |
|---|---|---|
| `business-service` | id, business_name, operational_name, aliases (active/retired), domain, description, criticality (`critical`, `high`, `standard`, `low`), lifecycle (`planned`, `live`, `deprecated`, `retired`), classification (data classification for access decisions) | 01-01, 02-01, 02-04, catalog access |
| `service-binding` | service_id, telemetry `service.name`, environment, first/last observed | catalog maps OTel services; 02-03 |
| `service-owner` | service_id, role (`business_owner`, `operational_owner`), principal ref (user or team), support route (channel/rota text), valid_from/to | 02-02 |
| `service-outcome` | service_id, statement, owner principal, approval (approver, approved_at), expectation text | 01-02, input to 01-04 |
| `critical-journey` | id, name, criticality, services[], owner, in_scope flag | 01-01 |
| `coverage-policy` | criticality → required coverage items (signal types, alert, objective, runbook, health rule), approved_by/at | 27-01 |
| `coverage-assessment` | service_id, item, required, demonstrated, evidence ref, assessed_at | 02-03, 27-02 |
| `coverage-gap` | service_id or journey_id, item, state (`open`, `remediating`, `accepted`, `closed`), remediation owner, due_date or review_date, risk acceptance (approver, rationale, expires) | 27-04 |
| `observability-asset` | kind (`alert_rule`, `objective`, `dashboard`, `proactive_check`, `health_rule`), external id, owning component, service_id, owner principal, registered/updated | 34-01 |
| `provider` | id, name, kind, contract ref, contract owner, materiality; `provider-dependency` (provider_id, service_id or journey_id, function) | 36-01 |

Operating contexts (OBS-01-03): environments stay owned by `observable-control` (`tenant-config`). Phase 1 adds an environment class (`production`, `non_production`) to control, and business-critical comes from service criticality. The catalog reads classes through `manage-control-plane`.

Data models go in `models/catalog.mdl` (Modelable, ADR-032) with generated Rust/TS; never hand-edit `generated/`.

## 4. Contracts

| Capability | Operations (sketch) | Failure modes |
|---|---|---|
| `manage-service-catalog` 0.1.0 | `GET/POST/PATCH /v1/catalog/services`, `GET /v1/catalog/services/{id}`, `GET /v1/catalog/search?q=` (business name, operational name, alias, domain, owner), journeys CRUD, `GET /v1/catalog/exceptions` (no outcome, no owner, owner left, uncovered) | existing set in `spec/business/capabilities.yaml` plus `access_denied` (rendered as not found) |
| `govern-observability-assets` 0.1.0 | `PUT /internal/catalog/assets/{kind}/{id}` (register/update, owner required), `DELETE` (retire), `GET /v1/catalog/assets?ownerless=true`, coverage policy CRUD, `GET /v1/catalog/services/{id}/coverage`, gaps CRUD with state machine | `ownerless_asset`, `gap_without_remediation`, `unsupported_evidence` |
| `manage-providers` 0.1.0 | providers CRUD, dependencies, `GET /v1/catalog/providers/gaps` | `unregistered_provider` |
| `decide-access` 0.1.0 (extended in S2, never released) | add `POST /internal/authz/filter` (batch: resource refs → permitted subset, no counts of denied) to satisfy the list invariant | unchanged |
| `lookup-principals` 0.1.0 (new, observable-auth) | `POST /internal/principals/resolve` → active/departed per user and team | `principal_unknown` |

Contract versions follow `--compat`; interface blocks are added to the capabilities before implementation (FEATURE.md step 2).

## 5. Authorization (S2, prerequisite for any catalog read)

- `observable-auth` implements `decide-access` with a PostgreSQL policy store in the `auth` schema: role grants per resource type and classification, purpose policies per resource type, and time-limited elevation grants. OpenFGA stays the phase 8 decision (ADR-041).
- Fail closed: store unavailable, unknown resource type, missing principal or tenant → deny; denied and non-existent produce identical responses (status, body, timing class).
- Every decision with purpose or elevation writes an audit record (`auth.access_decision_log`).
- Catalog calls `filter` before counting or paginating; single-record reads call `decide` and map deny to the same 404 as a missing record.
- `lookup-principals` reads `auth.users` and `auth.user_tenant_roles`; a user with no role in the tenant is "departed".

## 6. Coverage semantics (OBS-02-03, 27-01, 27-02)

- Required coverage = `coverage-policy` for the service's criticality, plus per-service overrides with approval.
- Demonstrated coverage per item:
  - telemetry signals: recent data for any binding in `store-telemetry` within the policy window (no data → unmet, never assumed);
  - alert, objective, dashboard, health rule, runbook: a registered asset for the service with an active owner.
- Coverage status per service: `covered`, `partial`, `uncovered`, `unknown` (assessment failed or stale). It is never "healthy"; health is phase 2.
- A requirement claimed met without evidence is reported unmet (27-02 negative): coverage items carry an evidence ref, and manual overrides cannot set `demonstrated`.

## 7. Isolation obligations (ISO-01..09)

- `services/catalog-service` runs standalone with in-memory fakes for `decide-access`, `lookup-principals`, `store-telemetry`, `manage-control-plane` (`standalone_harness`).
- Provider-owned programmable mocks for `manage-service-catalog`, `govern-observability-assets`, `manage-providers` (scenarios: success, access denied as not-found, ownerless, stale assessment, delay, timeout), and for `decide-access` and `lookup-principals` from `observable-auth`.
- One contract suite per capability, run against the real provider (Testcontainers PostgreSQL) and its mock.
- Web `catalog` module runs against the catalog mock through the runtime contract (playground and Playwright).

## 8. Slices

Each slice lists the requirements it verifies; evidence is `path::test_name` per criterion.

| # | Slice | Component(s) | Verifies | Depends on |
|---|---|---|---|---|
| S1 | Component skeleton and onboarding: crate, schema, role, image, compose, helm, manifest, policy tests, spec component block, `/health` `/readyz` `/metrics` | catalog, spec | — | — |
| S2 | `decide-access` (+ `filter`) and `lookup-principals` in auth, with mocks and contract suite | auth | `req-decide-access-fail-closed` | — |
| S3 | Service records: create/update, criticality, lifecycle, aliases, owners, support route, outcomes, search, exceptions; all reads through `filter`/`decide` | catalog | OBS-02-01, 02-02, 02-04, `req-catalog-access-control`; records for 01-02 | S1, S2 |
| S4 | Operating contexts and journeys: environment class in control, journeys in catalog | control, catalog | OBS-01-03; records for 01-01 | S3, Q3 |
| S5 | Coverage: policies, telemetry-backed assessment, status, gaps workflow | catalog | OBS-02-03, 27-02; records for 27-01, 27-04 | S3, S6 |
| S6 | Asset registry and producers: alerting registers rules and SLOs, control registers dashboards; owner required; ownerless report. Shipped as one PR: alerting pushes (`POST /internal/catalog/assets/sync`), the catalog pulls control's dashboards (`GET /internal/observability-assets`) because control -> catalog would close a cycle with environment classes; owners default to the creator and API-key-created assets are reported ownerless rather than refused | catalog, alerting, control | records for 34-01 | S3 |
| S7 | Providers and dependencies, unregistered-provider gaps. Shipped with dependencies on services or journeys, by provider id or by name (unregistered), contract-owner and unmapped-provider gaps, and `unregistered_provider` on the exception list; `observability-asset` folded into `coverage-record` for COMP-03 | catalog | records for 36-01 | S3 |
| S8 | Web `catalog` module: list, search, service record, coverage, gaps, providers, ownerless assets; restricted-service and restricted-search negatives in Playwright | web | UX-01 `ux-01-restricted-service`, `ux-01-search-service-restricted`; UI criteria of 02-01..04 | S3-S7 |
| S9 | Phase exit: inspection evidence for organization-held requirements, criterion evidence for outstanding platform requirements, readiness `--strict`, roadmap/gap update | spec | OBS-01-01, 01-02, 01-03, 01-04, 02-04, 27-01, 27-04, 34-01, 36-01 | S3–S8 and section 11 |

S6 and S7 can run in parallel after S3. S5 waits for S6 because demonstrated coverage counts registered assets.

## 9. Risks

- **Critical: onboarding breadth.** A new runtime component touches about 30 files and many policy tests (section 2). S1 is large and must land before any behavior.
- **Authorization latency.** Every list goes through `filter`; measure p95 under the perf smoke and keep it inside `req-query-latency`.
- **Owner semantics.** "Owner has left" needs a principal directory; without teams, ownership by individuals is brittle (Q4).
- **Health and objectives dependencies.** Context isolation, agreed expectations and retired-service health require the integration in section 11. OBS-02-03 coverage status is already verified.
- **Organization obligations.** Six P0s require accountable inspection of an approved inventory and deployed records. See Q2 and the S9 workbook.
- **Consumers keep `service_name`.** Alert rules, SLOs and incidents still key on strings until their phases; the catalog binding is the bridge, so binding changes must not orphan assets.

## 10. Open questions

- **Q1. Resolved 2026-11-08.** Close Phase 1 first. Keep OBS-01-04 and the other two platform requirements in Phase 1 and bring forward the work in section 11.
- **Q2. Inspector named 2026-11-08.** Tommy (`ktjn`) will sign as observability platform owner, SRE lead and vendor manager. Deployment, tenant, approved inventory and criterion results are pending his verification.
- **Q3.** Environment class lives in `observable-control` (recommended) or in the catalog?
- **Q4.** Owners: users and teams (teams stored in the catalog, members resolved through auth; recommended), or users only?
- **Q5.** Catalog entry: API and UI only in phase 1 (recommended), or also YAML import (Backstage-style `catalog-info.yaml`)?

## 11. Phase 1 closure slices

Run the inspection track as soon as the deployment and inventory are confirmed. On
the platform track, land provider contracts and their tests before wiring consumers.
Use the existing Phase 1 requirements as the acceptance gate for this work.

| Order | Slice | Required verification |
|---|---|---|
| C1 | Specify the catalog inputs for service assessment and reporting. Resolve catalog service ID, lifecycle and telemetry bindings with approved environment classification. Reuse the existing authorized catalog and control operations where their response and permissions suffice. | Exercise production and non-production bindings with the same telemetry name, business-critical filtering, a retired service and a denied service. Verify an unclassified binding cannot enter production. |
| C2 | Add the `observable-health` provider under ADR-039. Evaluate and persist health for catalog service and approved context, using scoped telemetry, rules and evidence age. See the [health provider contract](phase1-health-provider.md). | Real/mock provider contracts cover isolation by tenant and context, retired services, absent or stale evidence, authorization denial and authorization outage. |
| C3 | Bind an approved measurable objective to a catalog service and its relevant context. Evaluate compliance against current scoped evidence. Keep the approved outcome statement separate from the measurable objective definition. | Test compliant, at-risk and breached results for a selected service. Missing or stale evidence yields unknown. Unapproved expectations cannot produce a compliance result. |
| C4 | Integrate the query and web health, reporting and review paths with those provider contracts. Replace the legacy derived health on the affected paths. | Use the same service in production and non-production. Inject a failure only in non-production and verify production health and reporting are unchanged while the non-production views reflect the failure (OBS-01-03). Retire the service and verify search, health and reporting (OBS-02-04). Exercise all four expectation states through stakeholder review (OBS-01-04). |
| C5 | Record current evidence for all criteria and run the S9 exit checks. | Six signed organization inspections, strict readiness with 15/15 verified, both UX-01 negatives in the browser, and a passing specification gate. |

C1 uses `GET /v1/catalog/contexts` for environment classes and service detail for
lifecycle. `GET /v1/catalog/services/{id}/bindings` uses the service read policy and
returns `{"bindings": [{"service_name": "checkout", "environment": "prod"}]}` with
stored values ordered by service name and environment. Authorized reads include
retired services and return an empty list for an unbound service. Missing and denied
services return the same 404. Policy outages fail closed for existing services.
The read accesses catalog metadata only. An empty environment remains an unscoped
binding. Consumers must select explicitly classified environments for production
and exclude retired services from live health.

C3 places objectives in `observable-alerting`, the provider of
`manage-reliability-objectives`. `alerting.service_objectives` (migration 063) binds an
availability target, window, at-risk budget share and freshness bound to a catalog service
id and one environment class. It does not reference the catalog's approved outcome
statement: the outcome states the business expectation, the objective its measure. A
tenant admin approves the revision they read, and any change clears the approval.
`GET /v1/objectives/{id}/compliance` forwards the caller's credential to the catalog
(service detail, bindings and contexts), counts spans of the bindings whose exact
environment is in the class over the window, and returns `breached` below target,
`at_risk` at or above the budget share, otherwise `compliant`. No spans, or none within
the freshness bound, yields `unknown`. An unapproved objective returns 409 with no result.
The legacy `slo_definitions` and their burn-rate alerts are unchanged.

C4 ships in three PRs. C4.1 adds the service review to `observable-query`, which now requires
`manage-service-catalog`, `assess-service-health` and `manage-reliability-objectives` (web
stays within its requires budget by reading the review through `query-telemetry`).
`GET /v1/review/services/{id}?environment_class=` reads the catalog record, then health and the
class's objectives with their compliance, all with the caller's credential. A hidden or missing
service is one 404 and no provider is asked; a catalog outage is 503. Health or objectives that
are withheld (404) or unavailable yield `unknown` with `restricted` or `evaluation_unavailable`.
The expectation combines approved objectives: any breach, then any risk, then any unknown,
otherwise compliant; no approved objective is `unknown` with `no_approved_objective`.
`GET /v1/review/services?environment_class=&context=business_critical|all` pages the same
review over catalog records; `business_critical` delegates the live and criticality filter to
the catalog, and `all` lists retired services as not live. C4.2 moves the web catalog record and
a business-critical report onto the review. C4.3 retires the legacy derived `health_state`
(`services/query-api/src/discovery.rs`, `libs/query-core/src/service_query.rs` and the web
pills). Run impact analysis for each touched
capability and deliver one component per PR where possible. Every new
provider needs an independent harness, a versioned mock and real/mock parity. Run
the local CI gate before pushing code and keep Phase 2 and Phase 4 acceptance gates
open for their remaining scope.
