# Service Catalog, Coverage, Asset & Provider Governance Inspection

This organizational governance inspection signs off the six `organization`-held Phase 1 P0 controls backed by `observable-catalog` and `observable-alerting`.

## 1. Service & Critical Journey Scope (`OBS-01-01`)

- **Approved Scope Register:** All in-scope business services and critical customer journeys declare criticality (`tier-0`/`tier-1`), accountable owner, ordered service dependencies, and an explicit `in_scope` flag via `GET /v1/catalog/scope`.
- **Coverage Gap Rule:** Any critical customer journey outside the covered list is reported as an explicit coverage gap (`critical_journey_outside_scope`) and is never silently treated as in scope (verified by `services/catalog-service/tests/catalog_journeys_integration.rs`).

## 2. Business Outcomes & Accountable Ownership (`OBS-01-02`)

- **Approved Outcome Policy:** Every critical service in scope requires at least one human-approved business outcome and an accountable owner.
- **Exception Reporting Rule:** Any critical service lacking an approved business outcome or accountable owner is automatically flagged as a catalog exception (`services/catalog-service/tests/catalog_records_integration.rs`).

## 3. Minimum Coverage Expectations (`OBS-27-01`)

- **Criticality Coverage Policy:** Each criticality tier has an approved minimum observability coverage policy (traces, logs, metrics, health/SLO rules) with accountable ownership; policy edits withdraw approval until re-signed.
- **Exception Rule:** A critical service without an agreed minimum coverage policy is reported as `no_approved_coverage_policy` (`services/catalog-service/tests/catalog_coverage_integration.rs`).

## 4. Critical Gap Remediation & Risk Acceptance (`OBS-27-04`)

- **Closure Policy:** Unmet coverage requirements on critical services and journeys open tracked gaps with a remediation owner and review/due date.
- **No Unapproved Closure Rule:** A missing journey assessment or critical gap cannot be closed without demonstrated remediation evidence or an approved, unexpired risk acceptance with rationale and review date (`services/catalog-service/tests/catalog_coverage_integration.rs`).

## 5. Observability Asset Ownership (`OBS-34-01`)

- **Accountable Owner Requirement:** Alerts, reliability objectives, dashboards, proactive checks, and health rules record an accountable owner (synced across `observable-alerting`, `observable-control`, and `observable-catalog`).
- **Ownerless Asset Detection:** When an asset's owner leaves or becomes inactive, the asset registry flags the asset as `ownerless` (`services/alert-evaluator/tests/postgres_asset_owners_integration.rs`, `services/catalog-service/tests/catalog_asset_sync_integration.rs`, `services/catalog-service/tests/catalog_coverage_integration.rs`).

## 6. Third-Party Provider & Contract Register (`OBS-36-01`)

- **Material Provider Register:** Material third-party providers record kind, materiality, contract reference, contract owner, and all dependent business services and critical journeys.
- **Unregistered Provider Gap Rule:** Any critical function depending on an unregistered third-party provider is reported as an `unregistered_provider` gap (`services/catalog-service/tests/catalog_providers_integration.rs`).
