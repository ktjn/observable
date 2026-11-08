# Phase 1 S9 exit review

**Reviewed:** 2026-11-08 against `d46a22740699c2c3f06a69806110b7f8fa3ffea3`
([S8 PR #852](https://github.com/ktjn/observable/pull/852)).
**Decision:** Phase 1 exit remains blocked. This is an evidence audit and inspection
workbook, not an accountable sign-off. No requirement is promoted to verified here.

## Work status

- [x] Audit merged S1–S8 requirement statuses, criteria and supporting tests.
- [x] Run strict readiness, the specification gate and catalog browser negatives.
- [x] Reconcile the stale OBS-01-02 gap classification and publish inspection steps.
- [x] Name the accountable inspector for all three roles (Q2).
- [ ] Confirm the deployment, tenant and approved inventory to inspect.
- [ ] Record actual observations and signed criterion-level results for six organization requirements.
- [x] Resolve sequencing by bringing the required health and objectives work into Phase 1 closeout.
- [ ] Implement and verify the three outstanding platform requirements.
- [ ] Rerun readiness and approve phase exit only after every exit obligation is satisfied.

## Readiness and verification

The initial `uv run --project models python scripts/spec-readiness.py --priority P0
--phase 1 --strict --json` returned 6 verified, 8 partial, 1 missing, with no policy
violations. Inspection of `api.rs::create_outcome`, `approve_outcome`, `exceptions_for`
and `catalog_records_integration.rs::critical_service_without_approved_outcome_is_an_exception`
shows that the OBS-01-02 entry saying no capability exists is stale. Correcting that
entry to partial yields **6 verified, 9 partial, 0 missing**. Partial means a related
capability exists, not that the acceptance or negative criteria have passed.

The six verified requirements are OBS-02-01, OBS-02-02, OBS-02-03, OBS-27-02,
`req-decide-access-fail-closed` and `req-catalog-access-control`.

`--strict` rejects invalid evidence claims. It does not require all Phase 1 requirements
to be verified. Also, `--fail-below` counts all P0 requirements even when `--phase 1`
filters the printed table. Review the Phase 1 row itself: exit requires verified = total
= 15, partial = missing = 0, plus the workflow and risk obligations in ROADMAP.md.

Validation for this documentation review:

- `bash scripts/spec-gate.sh`: passed, including verification stage, artifacts/drift,
  baseline integrity, UX contracts, readiness policy and its seven regression tests.
- From `apps/frontend`, `FRONTEND_PORT=5192 CI=1 npx playwright test
  e2e/catalog.spec.ts --reporter=list`: four tests passed. This includes
  `ux-01-restricted-service`, `ux-01-search-service-restricted`, retired/departed ownership
  and authorization outage. These browser tests intercept API calls. They verify UI
  behavior, not real deployed authorization or an organization's records.
- Supporting Rust tests below were inspected, not rerun in S9. Full local CI is exempt
  for this documentation-only change. No operational deployment or business inventory
  was inspected, and no organizational approval is inferred from the S8 merge.

## Outstanding platform criteria

| Requirement | Remaining acceptance work | Dependency |
|---|---|---|
| OBS-01-03 | Restrict views and reporting to approved contexts, and demonstrate that a non-production failure cannot change production health or reporting. Catalog context classification alone is insufficient. | Scoped health and reporting |
| OBS-01-04 | Demonstrate compliant, at-risk and breached status against agreed service expectations, with unknown when current evidence is absent. Outcome records alone are insufficient. | Approved objectives and current scoped evidence |
| OBS-02-04 | Demonstrate that retired services cannot appear live in health as well as search and reporting. The catalog lifecycle tests do not exercise health. | Health and reporting tied to catalog lifecycle |

The Phase 1 [closure slices](phase1-service-catalog.md#11-phase-1-closure-slices)
bring the required work forward. Phase 1 retains all 15 P0 requirements. The
2.0.0 release threshold remains 102 P0 requirements.

## Accountable inspection workbook

For every row, record the deployment revision, tenant, inventory source and timestamp,
reviewer identity and role, each criterion's observations, evidence reference and outcome,
and any exceptions with owner and next action. Compare catalog records to an independently
maintained business inventory so omitted services, journeys or providers can be detected.
An empty catalog, synthetic fixtures or a test suite alone cannot establish completeness.
Use a controlled test tenant for negative scenarios. Do not alter operational records
merely to collect evidence.

Tommy (`ktjn`) will sign as observability platform owner, SRE lead and vendor manager.
All six rows are **pending** while he verifies the deployment, tenant and inventory.

| Requirement / role | Acceptance inspection | Negative inspection | Supporting source (relative to repository root) |
|---|---|---|---|
| OBS-01-01 / observability-platform-owner | Reconcile every in-scope service and critical journey and its criticality against the approved business inventory using `/v1/catalog/scope`. | Identify a critical journey omitted from coverage and record the resulting coverage gap. | `services/catalog-service/tests/catalog_journeys_integration.rs::scope_lists_in_scope_services_and_journeys_with_criticality`, `::critical_journey_outside_scope_is_reported_as_a_gap` |
| OBS-01-02 / observability-platform-owner | Inspect every critical service for an approved outcome and accountable owner. Reconcile all in-scope services against the requirement statement as well. | Observe the exception for a critical service without an approved outcome or owner. Record both cases. | `services/catalog-service/tests/catalog_records_integration.rs::critical_service_without_approved_outcome_is_an_exception`, `::departed_owner_flags_service_as_lacking_accountable_ownership` |
| OBS-27-01 / sre-lead | Inspect approved coverage expectations and accountable ownership for critical services, taking criticality and journeys into account. | Observe the exception for a critical service without agreed minimum coverage. | `services/catalog-service/tests/catalog_coverage_integration.rs::critical_service_without_agreed_coverage_is_an_exception` |
| OBS-27-04 / sre-lead | Inspect critical gaps for remediation owner, due/review date and documented risk acceptance where deferred. Demonstrate that a missing journey assessment cannot close without remediation or approved exception. | Attempt closure without remediation or approved risk acceptance and record refusal. | `services/catalog-service/tests/catalog_coverage_integration.rs::missing_journey_coverage_cannot_be_closed_without_remediation`, `::critical_gap_closes_only_when_remediated_or_risk_accepted` |
| OBS-34-01 / observability-platform-owner | Reconcile alerts, objectives, dashboards, proactive checks and health rules with their accountable owners. Inspect current owner visibility for critical alerts. Explicitly inventory asset kinds whose producers are deferred. | Observe an asset with a departed owner in the ownerless report. | `services/alert-evaluator/tests/postgres_asset_owners_integration.rs::a_rule_is_owned_by_its_creator_or_an_explicit_principal`, `services/catalog-service/tests/catalog_coverage_integration.rs::claimed_requirement_without_evidence_is_unmet` |
| OBS-36-01 / vendor-manager | Reconcile material providers against the external inventory and inspect dependent critical functions, services, contract references and contract owners. | Observe the gap for a critical function depending on an unregistered provider. | `services/catalog-service/tests/catalog_providers_integration.rs::provider_record_lists_dependent_critical_functions_and_contract_owner`, `::critical_function_on_an_unregistered_provider_is_a_gap` |

After inspection, retain the signed observations in a stable artifact. Add evidence only
for actually observed criteria, copying the exact criterion strings from the corresponding
`spec/business/areas/` requirement. Each record needs `requirement_version`, `criteria`,
`verifier`, `method: inspection` (or justified analysis), result and stable
`path::inspection_name` provenance. Failed or incomplete observations stay open. Promote
the requirement and gap status only when all criteria are covered by current passing
evidence and the strict policy accepts it.

## Remaining exit work

1. Confirm the deployment revision, tenant, approved business inventory and location
   for the signed inspection records. For each criterion, capture the expected result,
   actual observation, evidence reference, result and sign-off date in that record.
2. Implement and verify the closure slices, then rerun the exit checks above.
