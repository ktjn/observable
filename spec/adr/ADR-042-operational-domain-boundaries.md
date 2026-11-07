# ADR-042: Orthogonal operational domain boundaries

**Status:** Accepted for the 2.0 specification baseline (2026-11-07)

## Context

The existing system had implementation layers but no explicit system domains. A large codebase needs domain dependency rules orthogonal to deployment and layering. The business requirement areas are not necessarily software components or implementation domains.

## Decision

Declare eight initial implementation domains in `spec/system.yaml` and assign each existing runtime component:

| Domain | Existing component |
|---|---|
| operational-experience | observable-web |
| ingestion | observable-ingest |
| investigation | observable-query |
| service-control | observable-control |
| incident-response | observable-alerting |
| processing | observable-process |
| identity | observable-auth |
| telemetry-storage | observable-store-clickhouse |

Declare `uses` edges for current cross-domain capability dependencies, keeping the graph acyclic. New `observable-catalog`, `observable-health`, and `observable-response` components receive their own assignments and relationship review when designed; these names are planned, not implemented components.

Business capabilities and UX experiences may span any number of domains; domains do not duplicate business requirements. Domain changes follow the system-specification compatibility/impact process.

## Consequences

- Architectural coupling becomes expressible and reviewable at the metamodel level.
- Additional declarations alone do not prove runtime or source-code isolation; check-drift, provider/consumer contracts and tests remain separate controls.
- Do not create components merely to match a domain or business requirement area.
- Future splitting of incident and health functionality requires updating `uses` and bounded-context handoffs.

## Alternatives

- Keep unbounded component relationships: insufficient control as the system grows.
- Align domains to technical layers or all 37 business areas: confuses orthogonal classification and would incentivize artificial boundaries.
