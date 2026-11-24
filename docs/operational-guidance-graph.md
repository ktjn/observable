# Operational guidance graph (W4, OBS-39-01..20): specification delta

Status: **approved** by Tommy on 2026-11-24 as recommended (D2 alerting ledger, D3 auth returns the API key id, D4 W4a then W4b). Approval scope: this document at this revision (system-spec/FEATURE.md, specification approval gate). Plan: [wire-unrouted-capabilities](plans/wire-unrouted-capabilities.md), slice W4.

## 1. Request and baseline

- Request: wire the unit-only OBS-39 requirements into real routes with integration evidence, following the system spec (Tommy, 2026-11-23: "Wire them in").
- Baseline: `services/alert-evaluator/src/guidance_graph.rs` holds pure helpers and unit tests only. Nothing is routed or stored. Evidence entries for OBS-39-01..20 point at those unit tests.
- Capability: `manage-operational-guidance`, provider `observable-alerting` (spec/system.yaml). All 20 requirements target it.
- Facts that shape the design:
  - Catalog keeps **current** owners only (`catalog.service_owners`, primary key `(service_id, role)`). There is no ownership history anywhere.
  - Journeys (`catalog.journey_services`), providers (`catalog.provider_dependencies`) and owners are catalog data. Deployments (`deployment_markers`) are control data. Incidents are alerting data. Service-to-service calls exist only as spans in telemetry.
  - Authorization of service records is the catalog's (decide-access behind the catalog's read routes). Alerting already forwards the caller's credential to the catalog for objectives.
  - The auth service's key validation returns tenant and role, not the key's identity, so alerting cannot tell two API keys apart.

## 2. Scope

In scope: stable entities, relationships with source/confidence/validity, conflicts, unknown versus absent, as-of reads, authorization on every read, purpose-scoped machine context for approved agents, and review of inferred relationships.

Out of scope: a UI, dependency impact scoring (`map-dependencies-and-impact`), and editing catalog data from the graph.

Prohibited behaviour, from the negative criteria:
- An absent edge is shown as absence of dependency.
- An expired relationship is shown as current.
- A later change rewrites what was recorded for an earlier time.
- A conflicting claim is overwritten.
- A hidden entity is revealed by a node, an edge, a count or a different error.
- An agent reads outside its approved purpose.
- A rejected relationship comes back on identical evidence.
- A correction is accepted without a reason.

## 3. Decisions (approval needed)

| # | Decision | Recommended | Alternative |
| --- | --- | --- | --- |
| D2 | Where relationship history lives | **Alerting keeps an append-only relationship ledger.** It is fed by pulling catalog relationships, control deployments, its own incidents and telemetry-inferred calls. Catalog stays current-state. | Catalog adds temporal owner, journey and provider history and an as-of read. Alerting composes from it. This is a catalog contract change, and history covers catalog relations only. |
| D3 | How an agent's approved purpose is known | **The auth service returns the API key id on validation (`validate-credential`, additive field).** A tenant admin registers an agent key in alerting with its approved purposes. | Purpose is only a stated request field checked by decide-access purpose rules per role. There is no per-agent approval, so OBS-39-16 holds per role, not per agent. |
| D4 | Delivery | **Two PRs: W4a (39-01..03, 06..14) then W4b (39-04, 05, 15..20).** | One PR. |

## 4. Behaviour

### 4.1 Entities (OBS-39-06)

`entity = (kind, canonical_key)` per tenant:
- `service`: catalog service id
- `owner`: principal kind and id
- `journey`: catalog journey id
- `provider`: catalog provider id
- `change`: deployment id
- `incident`: incident id

A telemetry `service_name` and environment resolve to a service only through catalog bindings. A name the catalog does not bind becomes its own `unregistered_service` entity. Names are never matched by similarity. The same service reported by catalog and telemetry is one entity; two services with similar names stay two.

### 4.2 Relationships (OBS-39-02, 09, 10, 11)

An assertion is `(subject, relation, object, source, source_ref, confidence, valid_from, valid_to?, recorded_at)`.

| Field | Values |
| --- | --- |
| `relation` | `owned_by`, `part_of_journey`, `provided_by`, `changed_by`, `affected`, `calls` |
| `source` | `catalog`, `control`, `alerting`, `telemetry` |
| `confidence` | `declared` (catalog), `recorded` (deployments, incidents), `inferred` (telemetry) |

Rules:
- Assertions are append-only. The only permitted update sets `valid_to` once, from null, when the source stops asserting it. A database trigger refuses any other update, and the alerting role gets no DELETE except through tenant purge.
- `valid_from` is the source's own time where it has one (`assigned_at`, `started_at`, `triggered_at`). Otherwise it is the time of the sync that first saw the assertion; the response says which.
- A relationship without a source is never stored, so it is never shown as established.
- Two claims for the same single-valued relation over overlapping intervals (for example the catalog owner and the incident's recorded owner) are both returned with `conflict: true`. Neither replaces the other.

### 4.3 Coverage, unknown versus absent (OBS-39-01, 08, 12)

Each pull run records `(tenant, source, started_at, completed_at, complete)`. For an entity and relation the read reports:
- `known` with edges, or
- `unknown`: no complete run of the responsible source covers the time, or the relation's source is telemetry-only.

There is no "none" state. A time before the first complete run of every source returns `no_recorded_context` rather than current data.

### 4.4 History (OBS-39-03, 12, 13)

`?at=` selects assertions with `valid_from <= at < coalesce(valid_to, +inf)` and `recorded_at <= at`. A later change can only close an interval or add a new one, so what was visible at incident time stays visible as it was. Without `at`, a response marks each edge `current: true` only if `valid_to` is null.

Implementation notes (W4a):
- A sync closes an interval at its own time, never earlier. A closure therefore never changes a read as of an earlier time.
- An as-of read renders each edge as it was recorded then, without `valid_to` or `current`. Edges of that snapshot closed since are listed under `since_then` as `{assertion_id, valid_to, current: false}`. The snapshot stays byte-identical; `since_then` says what changed.
- A resolved incident's `affected` edges close at the first sync that sees the resolution. The resolution time stays in the incident record.
- An incident's recorded owner is stored as `incident owned_by owner` (source `alerting`, qualifier `incident`). Service ownership conflicts come from the catalog and the authoritative directory (`catalog_authoritative`, the catalog's open sync conflicts).
- A failed source pull records an incomplete run and closes nothing.

### 4.5 Reads (OBS-39-01, 07, 14)

| Route | Returns |
| --- | --- |
| `GET /v1/guidance/entities/{entity_id}?at=` | Entity, its relationships grouped by relation, each with source, confidence, validity, `current`, `conflict`; per-relation `known` or `unknown`. |
| `GET /v1/guidance/graph?from={entity_id}&depth=1..3&relations=&at=` | Reachable nodes and edges. Supports the responder path journey → service → recent change → owner. |
| `GET /v1/guidance/entities?kind=&key=` | Lookup by canonical key (for example a catalog service id or incident id). |

Every service, journey and provider node is authorized by forwarding the caller's credential to the catalog (`manage-service-catalog`), the same as objectives.
- A hidden node is dropped along with every edge to it, and nothing counts it.
- A hidden or nonexistent start entity is the same `404 not_found`.
- Changes and incidents inherit visibility from their service.
- History uses the requester's current authorization.

### 4.6 Machine context for approved agents (W4b; OBS-39-04, 15, 16, 17)

- `PUT /v1/guidance/consumers/{api_key_id}`: a tenant admin sets `{approved_purposes: [...]}`. Purposes and what each one releases:
  - `incident_response`: owners, changes, incidents, journeys
  - `dependency_analysis`: calls, providers, journeys
- `GET /v1/guidance/context?service_id=&purpose=&at=`: API key only.
  - The key must be registered and `purpose` must be one of its approved purposes; otherwise `403 purpose_not_approved`. The 403 is the same whether the service exists or not, and is checked before any lookup.
  - A restricted or missing service returns the same `404 not_found`, with an identical body.
  - The response is JSON. Every element carries `provenance {source, source_ref, confidence, valid_from, recorded_at}` and `evidence_refs` (incident id, deployment id, catalog record, trace ids for inferred calls).
  - The server never emits an element without provenance. If one cannot be built, the element is dropped and the response is marked `complete: false`.

### 4.7 Review of inferred relationships (W4b; OBS-39-05, 18, 19, 20)

- `POST /v1/guidance/relationships/{assertion_id}/review` with `{decision: "reject" | "confirm", reason}`.
  - Only a signed-in tenant admin, or an operational owner of the subject service, may review. Anyone else gets 403.
  - `reason` is required, 1-1000 characters; without it the request is 400.
  - Only `inferred` assertions are reviewable; others get 409.
  - The record keeps reviewer, time, reason and the `evidence_fingerprint` the inference was built from.
- A rejected relationship is not returned as established. It is listed under `rejected` with its reason and reviewer.
- When ingestion infers the same relationship again:
  - Identical evidence (same fingerprint, for example the same trace pattern re-read) leaves it rejected.
  - New evidence (calls observed after the rejection, with a different fingerprint) moves it to `reopened`, which needs review again. It is never reinstated automatically.

## 5. Storage (alerting schema, one migration)

| Table | Contents |
| --- | --- |
| `alerting.guidance_entities` | `(entity_id, tenant_id, kind, canonical_key, display_name)`, unique `(tenant_id, kind, canonical_key)` |
| `alerting.guidance_assertions` | as 4.2, plus `evidence_fingerprint`, review state |
| `alerting.guidance_source_runs` | as 4.3 |
| `alerting.guidance_reviews` | `(assertion_id, reviewer, decision, reason, evidence_fingerprint, decided_at)`, append-only (W4b) |
| `alerting.guidance_consumers` | `(tenant_id, api_key_id, approved_purposes, approved_by, updated_at)` (W4b) |

Every table is schema-qualified and granted to `observable_alerting_role`, and included in the alerting tenant purge.

Data model: a new `operational-relationship` model (owner observable-alerting, `derived_from` `business-service`, `critical-journey`, `provider-record`, `incident-record`, `span-row`; classification `confidential`).

## 6. Contract changes outside alerting

| Component | Change |
| --- | --- |
| observable-catalog | `GET /internal/catalog/relationships?tenant_id=` (internal token): current owners with `assigned_at`, journey members, provider dependencies, open authoritative owner conflicts, in one repeatable-read snapshot. Added to the `manage-service-catalog` operation list. The provider mock covers only the public reads, as for `/internal/catalog/health-inputs`. |
| observable-control | `GET /internal/deployments?tenant_id=&since=` (internal token), oldest first. It is the read side of `publish-deployment-markers`, so alerting adds `requires: publish-deployment-markers@^0.1` (W4a; `track-changes` has no interface). |
| observable-auth (D3) | `validate-credential` key validation adds `key_id`. This is additive; existing consumers ignore it. |
| query (telemetry) | none; alerting already reads spans through `store-telemetry`. |

## 7. Verification specification

All of these are integration tests through the routes, on real PostgreSQL as the `observable_alerting` login and real ClickHouse, with the catalog substitute for authorization.

| Req | Test (precondition → action → expected; negative) |
| --- | --- |
| 39-01, 39-07 | Journey J with service S, deployment D on S an hour ago, owner O → graph from J depth 3 → path J→S→D, S→O. Negative: an S without telemetry or a declared dependency has `calls` state `unknown`, never empty-and-known. A hidden service S2 in J is absent, with no edge and no count. |
| 39-02, 39-10 | Owner changes from O1 to O2 after incident I. At I's time the read shows O1 (`current: false`, interval closed). Now it shows O2. Negative: the closed O1 edge is never `current: true`. |
| 39-03, 39-12 | Topology as of an earlier incident equals what was recorded then. Negative: `at` before the first complete run returns `no_recorded_context`. |
| 39-13 | After the owner change and a journey edit, the as-of read for the incident time is byte-identical to the read taken at that time. Negative: a direct UPDATE of a closed assertion is refused by the trigger. |
| 39-06 | Catalog service S and telemetry `checkout`/`prod` bound to S resolve to one entity. Negative: unbound `checkout-v2` is a separate `unregistered_service`. |
| 39-08 | A service with no complete telemetry run reports `calls: unknown`. |
| 39-09 | Every edge names its source and source_ref. Negative: an ingestion item without a source is rejected and not stored. |
| 39-11 | Catalog owner O1 and the authoritative directory's owner O3 for the same role overlap → both returned with `conflict: true`, each with source and time. |
| 39-14 | User without access to S gets 404 for S's entity and graph, now and `at=` past. Negative: graph from a visible journey omits S. |
| 39-04, 39-15 | Registered agent with `incident_response` → context for S has owners, changes and incidents, each with provenance and evidence refs. Negative: no element lacks provenance (schema-checked in the test). |
| 39-16 | Same agent asks with `dependency_analysis` → 403 `purpose_not_approved`. An unregistered key gets 403. |
| 39-17 | Restricted S and a random id return identical status and body for an agent and for a user. |
| 39-05, 39-18, 39-19 | Tenant admin rejects inferred S→T with a reason → listed under `rejected` with reviewer, time and reason. Negative: a viewer or a non-owner member gets 403. No reason gives 400. |
| 39-20 | Re-ingest the same trace pattern → still rejected. New calls after the rejection → `reopened`, not established. |

## 8. Risks

- Pull-based history has sync-period resolution where a source gives no time of its own. The response labels such times `observed_at_sync`.
- Ledger growth: assertions are per change, not per sync. Unchanged relationships add nothing.
- Telemetry inference cost: one aggregated ClickHouse query per tenant per run, bounded by the lookback.
