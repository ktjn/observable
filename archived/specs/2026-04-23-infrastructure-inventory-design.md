# Infrastructure Inventory and Detail Views Design

**Date:** 2026-04-23

**Plan slice:** `P3-S9` from `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`

**Source spec:**
- `spec/05-frontend.md` §9.2.1 Infrastructure and §9.4 Infrastructure Correlation
- `spec/09-api.md` Infrastructure Views

## Goal

Add tenant-scoped infrastructure inventory and detail views that cover hosts, Kubernetes
clusters, namespaces, pods, and containers using existing telemetry resource attributes
first. Users must be able to move from infrastructure entities to related services, logs,
metrics, and traces without manually rebuilding filters.

## Scope

### In scope

- Shared infrastructure inventory API in `services/query-api`
- Shared infrastructure detail API in `services/query-api`
- Shared Infrastructure inventory route in `apps/frontend`
- Shared Infrastructure detail route in `apps/frontend`
- Entity-type filtering across host, cluster, namespace, pod, and container
- Related service links plus prebuilt navigation into logs, metrics, and traces
- Empty states and tenant-scoped API/frontend tests

### Out of scope

- Persistent infrastructure asset catalog independent of telemetry
- New storage tables or background sync jobs
- Full infrastructure dashboard builder
- Deep per-entity utilization rollups when the source telemetry does not expose them yet
- `P3-S10` service-to-infrastructure and trace-to-infrastructure navigation beyond the links
  needed from the new infrastructure views themselves

## Recommended Shape

Implement one shared inventory/detail flow rather than five separate products.

The backend exposes one normalized entity shape with an `entity_type` discriminator. The
frontend uses one inventory page and one detail page that render entity-specific labels and
secondary fields from that shared model. This keeps the slice reviewable, satisfies the full
`P3-S9` outcome, and avoids duplicating query logic and tests across five separate routes.

## Existing Code Context

- `services/query-api/src/discovery.rs` already owns service discovery, service summary, and
  topology discovery endpoints.
- `services/query-api/src/main.rs` already mounts discovery endpoints and is the natural place
  to register infrastructure routes.
- `apps/frontend/src/router.ts` already defines `/infrastructure` as a placeholder route.
- `apps/frontend/src/pages/ProductAreaPage.tsx` currently renders a static placeholder for the
  Infrastructure area and should be replaced by real inventory behavior for `area === "infrastructure"`.

## Backend Design

### Endpoints

Add two tenant-scoped endpoints to `services/query-api`:

- `GET /v1/infrastructure`
- `GET /v1/infrastructure/:entityType/:entityId`

`entityType` is one of:

- `host`
- `cluster`
- `namespace`
- `pod`
- `container`

### Query parameters

`GET /v1/infrastructure`

- `entity_type` optional; when omitted, return all supported types
- `environment` optional
- `service` optional; filters inventory to entities related to one service
- `lookback_minutes` optional; defaults to `60`
- `search` optional; applied to the normalized display name / entity id

`GET /v1/infrastructure/:entityType/:entityId`

- `environment` optional
- `lookback_minutes` optional; defaults to `60`

### Response shape

Inventory rows use one normalized shape:

```json
{
  "items": [
    {
      "entity_type": "pod",
      "entity_id": "checkout-7d8b9c6f5d-4j2pk",
      "display_name": "checkout-7d8b9c6f5d-4j2pk",
      "parent_id": "payments",
      "parent_display_name": "payments",
      "environment": "prod",
      "health_state": "watch",
      "last_seen_unix_nano": 1713885600000000000,
      "related_services": ["checkout-api"],
      "log_rate_per_minute": 32.4,
      "error_rate": 0.02,
      "restart_count": null,
      "cpu_usage": null,
      "memory_usage": null,
      "disk_usage": null,
      "network_io": null
    }
  ]
}
```

Detail responses wrap the same core entity plus navigation context:

```json
{
  "entity": {
    "entity_type": "pod",
    "entity_id": "checkout-7d8b9c6f5d-4j2pk",
    "display_name": "checkout-7d8b9c6f5d-4j2pk",
    "parent_id": "payments",
    "parent_display_name": "payments",
    "environment": "prod",
    "health_state": "watch",
    "last_seen_unix_nano": 1713885600000000000,
    "related_services": ["checkout-api"],
    "log_rate_per_minute": 32.4,
    "error_rate": 0.02,
    "restart_count": null,
    "cpu_usage": null,
    "memory_usage": null,
    "disk_usage": null,
    "network_io": null
  },
  "links": {
    "logs": "/logs?resource_attr=k8s.pod.name:checkout-7d8b9c6f5d-4j2pk",
    "traces": "/traces?resource_attr=k8s.pod.name:checkout-7d8b9c6f5d-4j2pk",
    "metrics": "/services/checkout-api/metrics?resource_attr=k8s.pod.name:checkout-7d8b9c6f5d-4j2pk"
  }
}
```

### Data derivation rules

Use telemetry/resource attributes first. Do not add a persistent catalog in this slice.

Entity identity mapping:

- `host` from `host.name`, falling back to `host.id`
- `cluster` from `k8s.cluster.name`
- `namespace` from `k8s.namespace.name`
- `pod` from `k8s.pod.name`
- `container` from `container.name`, falling back to `container.id`

Entity rows are derived by unioning distinct identities observed in:

- `spans`
- `logs`
- `metric_series`

For each entity row:

- `last_seen_unix_nano` is the latest timestamp seen for that entity across the source union
- `related_services` is the distinct service list observed on those records
- `environment` is the most recent non-empty environment value for that entity when present
- `error_rate` and `log_rate_per_minute` are best-effort summaries derived from the lookback
  window
- `health_state` is derived from available error/log signal, not from a new health system
- utilization fields stay nullable unless the underlying signal already exposes them

This slice must prefer partial-but-correct data over synthetic rollups. If CPU, memory, disk,
network, or restart counts are not available in current telemetry, return `null` rather than
invented values.

### Detail-page link rules

Each entity type maps to one canonical resource attribute filter:

- `host` -> `host.name`
- `cluster` -> `k8s.cluster.name`
- `namespace` -> `k8s.namespace.name`
- `pod` -> `k8s.pod.name`
- `container` -> `container.name`

The detail response exposes link targets built from those canonical attributes so the frontend
can navigate without reconstructing filter syntax.

## Frontend Design

### Routes

Keep the existing `/infrastructure` route as the inventory page and add:

- `/infrastructure/$entityType/$entityId`

Do not create separate top-level routes per entity type in this slice.

### Inventory page

The Infrastructure inventory page replaces the current placeholder branch in
`apps/frontend/src/pages/ProductAreaPage.tsx` or moves the infrastructure rendering into a new
dedicated page component if that is cleaner during implementation.

The page shows:

- top summary tiles for total entities and counts by health state
- a shared table of infrastructure entities
- entity-type filter with `all`, `host`, `cluster`, `namespace`, `pod`, `container`
- existing environment filter
- free-text search against display name / entity id
- related-service column with links to service detail routes
- last-seen column
- empty state when no entities match

Recommended columns:

- Entity
- Type
- Environment
- Health
- Related services
- Log rate
- Error rate
- Last seen

### Detail page

The entity detail page shows:

- entity identity and type
- parent relationship when applicable
- environment and last-seen metadata
- related services with links to `/services/$serviceId`
- action links into logs, traces, and metrics using the API-provided link context
- nullable summary fields rendered as `Unavailable` when the data is absent

The detail page does not need charts in this slice. It is a navigation and context page.

## Error Handling

- Unknown `entity_type` returns `400`
- Missing entity returns `404`
- ClickHouse/query failure returns `500` and logs a backend error
- Frontend inventory and detail pages render explicit loading, empty, and failed states
- Missing optional summary fields are non-fatal and render as `Unavailable`

## Testing Strategy

### Backend

Add tenant-scoped tests in `services/query-api` that cover:

- inventory returns only the requesting tenant's entities
- inventory can filter by `entity_type`
- inventory aggregates related services for one entity
- detail returns `404` for unknown entity id
- detail returns navigation context based on the canonical resource attribute for the type

### Frontend

Add frontend tests in `apps/frontend` that cover:

- inventory renders mixed entity rows from the normalized API response
- entity-type filter narrows the table correctly
- clicking an inventory row opens the detail route
- detail renders related service links and action links
- empty inventory state renders when API returns no items

## Verification

Required checks for the implementation slice:

- targeted Rust tests for the new query-api infrastructure handlers
- targeted frontend Vitest coverage for inventory/detail rendering
- `bash scripts/local-ci.sh --skip-smoke` at minimum if the touched surface stays inside
  backend/frontend build and test checks
- full `bash scripts/local-ci.sh` before push because this is a code change

Performance smoke is not required for this slice unless the implementation changes query shape
for existing performance-sensitive endpoints outside the new infrastructure handlers.

## ADR and Spec Impact

No ADR change is required for this slice because it does not change architecture, storage
strategy, tenancy model, deployment model, or roadmap scope. It implements already-approved
spec behavior using existing telemetry-driven identity rules.

## Success Criteria

`P3-S9` is complete when:

- `/infrastructure` shows tenant-scoped inventory rows for all five entity types when their
  resource attributes exist
- an entity detail route exists and renders normalized metadata
- users can move from infrastructure detail to related services, logs, traces, and metrics
  without manually reconstructing resource filters
- backend and frontend tests cover inventory rendering, detail rendering, empty states, and
  tenant isolation
