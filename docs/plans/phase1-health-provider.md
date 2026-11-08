# Phase 1 C2 health provider contract

Review the contract before onboarding `observable-health` or exposing a health endpoint.
This contract specifies the provider boundary in [ADR-039](../../spec/adr/ADR-039-health-assessment-capability.md).
Phase 1 tests the join with reporting and approved objectives in C4. The evidence for
the three remaining platform requirements depends on that join.

## Scope and response

`GET /v1/health/services/{id}?environment_class=production|non_production` returns the
current health assessment for a catalog service and an explicit environment class.
The session selects the tenant. Missing or unsupported class values return 400.
Business-critical reporting selects catalog services by criticality within an explicit
environment class. Criticality does not select or widen the telemetry scope.

An authorized live service returns `service_id`, `environment_class`, `state`,
`evaluated_at`, `last_evidence_at`, `evidence_status`, and permitted `conditions`.
Times are UTC RFC 3339 or null when unavailable. States are `normal`, `degraded`,
`unavailable`, `planned_maintenance`, and `unknown`. Evidence status is one of `current`,
`missing`, `stale`, `restricted`, or `evaluation_unavailable`. A caller who can see the
service and whose complete assessment is withheld receives only the `unknown` state,
`restricted` evidence status, null times, and empty conditions. This avoids deriving
an observable state or timestamp from hidden contributors. An authorized service with
no assessment returns `unknown` and `missing`.

Missing, denied and non-live services share the same 404 response for current health.
The catalog remains the authoritative record of retirement. Policy outage returns 503
with no health or evidence fields. Run service authorization even when there is no stored
assessment. The HTTP contract must exercise both denied and missing IDs during a policy
outage so cached assessments or different failure responses cannot reveal existence.

## Evaluation and storage

- Read lifecycle, telemetry bindings and approved classes through the catalog capability.
  A telemetry observation must match tenant, `service_name` and an exact nonempty
  environment that belongs to the requested class. Unscoped and unclassified bindings
  contribute to neither class. Treat ambiguous or incomplete catalog inputs as unavailable.
- Persist rules and assessments in the health provider's own PostgreSQL schema. Key
  the current snapshot by tenant, catalog service ID and environment class. Store the
  rule revision, binding and class inputs, evidence times and references, and evaluation
  time. Reject an older evaluation completing after a newer one. Validate the current
  lifecycle and scope before disclosing a stored assessment. A changed rule or scope
  invalidates the old snapshot until reevaluation. Retiring a service blocks reads of
  current health even if a previous evaluation is still present.
- Rules supply a freshness bound and sufficient evidence conditions. Absence, stale
  evidence, failed evaluation, or unresolved required inputs yield `unknown`. Explicit
  affirmative evidence of loss of service is required for `unavailable`. Slow service
  alone yields `degraded`. `planned_maintenance` requires a current authorized maintenance
  record. `normal` requires fresh evidence for all required conditions. Evaluate staleness
  at read time so an evaluator outage cannot keep a previously normal snapshot current.
- Read scoped evidence through `store-telemetry` with a read-only credential. Store only
  the evidence needed to evaluate and explain the result. Derived evidence retains the
  strongest applicable source classification. A sampled observation can establish only
  what the rule permits for that sample; record the sampling limitation.

## Authorization boundaries

| Caller | Authorization and scope |
|---|---|
| User read | Authenticate with `validate-credential`. Use `decide-access` to authorize the catalog service and the health evidence under the user's verified tenant and request context. Preserve purpose and elevation where applicable. Filter before serializing. If a required contributor is denied, return the restricted unknown response. Never trust a caller-supplied evidence classification. |
| Scheduled evaluation | The worker needs authenticated, least-privilege access to one tenant's catalog inputs and telemetry evidence. C1 endpoints currently use caller credentials. Establish and test the tenant-bound worker credential and its grant in a provider slice before wiring evaluation. A general cluster token that accepts a caller-provided tenant or a tenant-admin session is insufficient for this purpose. |
| Rule changes | Only authorized writers in the rule's tenant may publish rules. Read authorization cannot approve or replace a rule. Define the write contract, scope and audit fields in the rule slice before adding write routes. |

## Delivery and acceptance

| Slice | Deliverable | Evidence to run |
|---|---|---|
| C2.1 | Onboard the `observable-health` runtime, distribution entries, probes and schema role. Follow the catalog S1 checklist. The domain is `service-assessment`. | Startup, ready and schema privilege checks on Testcontainers, component boundary and image checks, Compose and Helm packaging. |
| C2.2 | Establish the tenant-bound worker authorization and scoped catalog and telemetry ports. | Worker grant matrix, two tenants with identical binding names, same name in production and non-production, empty or unclassified binding, changed binding/class and catalog outage. Test real PostgreSQL/ClickHouse adapters where applicable. |
| C2.3 | Evaluate published rules and persist scope-bound snapshots. Implement the authorized read contract and provider mock. | Shared real/mock scenarios for fresh, absent and stale evidence, non-production failure, retirement after a stored evaluation, restricted service, restricted contributor, policy outage with existing and missing IDs, and concurrent evaluations. Run HTTP cases through the actual router with Testcontainers. |

Provide a standalone harness with injected catalog, telemetry and auth substitutes.
Add the component and its owned models to `spec/system.yaml` with the first onboarding
slice. Version the provider mock alongside the contract and run the same scenarios
against both. The public read is gated on C2.3 passing `req-health-access-control`.
OBS-01-03, OBS-01-04 and OBS-02-04 require the C4 consumer acceptance tests.
