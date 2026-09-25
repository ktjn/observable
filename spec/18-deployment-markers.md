# Deployment Markers

## 18. Deployment Markers Specification

Deployment markers provide a mechanism to annotate telemetry signals with versioned release events. They enable operators to correlate shifts in error rates, latency, or traffic patterns with specific code or configuration changes.

### 18.1 Data Model

The deployment marker entity extends the core `Deployment` entity defined in `spec/14-domain-model.md`.

| Field | Type | Required | Notes |
|---|---|---|---|
| deployment_id | UUID | yes | Unique identifier for the deployment event. |
| tenant_id | UUID | yes | Partitioning key for multi-tenancy. |
| project_id | UUID | yes | Project boundary for the deployment. |
| service_name | string | yes | The name of the service being deployed. |
| environment | string | yes | Target environment (e.g., `production`, `staging`). |
| service_version | string | yes | Semantic version or build identifier. |
| status | enum | yes | `in_progress`, `success`, `failed`, `rolled_back`. |
| started_at | timestamp | yes | Time when the deployment process began. |
| finished_at | timestamp | no | Time when the deployment process completed. |
| deployed_by | string | no | Identity of the user or system triggering the release. |
| commit_sha | string | no | VCS commit identifier for change tracking. |
| rollback_of | UUID | no | If status is `rolled_back`, refers to the failed deployment. |
| metadata | JSON | no | Key-value pairs for additional context (see Recommended Properties). |

### 18.2 Recommended Properties (Metadata)

To maximize the utility of deployment markers, the following properties are recommended in the `metadata` payload:

| Property | Description | Example |
|---|---|---|
| `git.repository.url` | URL to the source code repository. | `https://github.com/org/repo` |
| `git.branch` | The branch used for the deployment. | `main` |
| `ci.pipeline.url` | Link to the CI/CD pipeline run. | `https://ci.example.com/jobs/123` |
| `ci.pipeline.id` | Unique ID of the CI/CD run. | `build-9982` |
| `deployer.email` | Contact email of the responsible person. | `dev-on-call@example.com` |
| `change.description` | Brief summary of the changes included. | "Fix memory leak in ingest" |
| `k8s.namespace` | Kubernetes namespace if applicable. | `prod-services` |
| `k8s.cluster` | Kubernetes cluster name. | `us-east-1-main` |

### 18.3 API Requirements

Deployment markers are control-plane metadata owned by `observable-control` under ADR-035.
Public endpoint paths remain stable while internal routing changes.

#### Control API

1. **Start Deployment**: `POST /v1/deployments`
   - Creates a new marker with status `in_progress`.
   - Returns the `deployment_id`.
2. **Finish Deployment**: `PATCH /v1/deployments/{deployment_id}`
   - Updates `status`, `finished_at`, and final metadata.
   - Supports transition to `success`, `failed`, or `rolled_back`.
3. **List Deployments**: `GET /v1/deployments`
   - Filters: `service_name`, `environment`, `start_time`, `end_time`.
   - Used by the frontend and query/correlation surfaces.

The public gateway/distribution layer routes these endpoints to `observable-control`; clients do not
address component-specific ports.

**Migration note:** the current implementation serves writes from the ingest-gateway Platform API
and reads from query-api. That routing is a compatibility bridge during roadmap `0.2`, not the
target ownership boundary.

### 18.4 UI Visualization

*   **Timeline Overlay**: Deployment markers should appear as vertical lines on all service-specific charts (RED metrics).
*   **Status Indicators**: Markers should be color-coded by status (e.g., green for success, red for failure).
*   **Hover Context**: Hovering over a marker should display version, committer, and a link to the CI/CD pipeline.

### 18.5 Deployment Correlation Enrichment

Telemetry components must not query control-plane PostgreSQL tables directly.

The target correlation path is:

1. **Control publication:** `observable-control` publishes a versioned
   `DeploymentChanged.v1` event when deployment state changes.
2. **Processor cache:** `observable-process` maintains a bounded local projection keyed by
   `(tenant_id, service_name, environment, service_version)`.
3. **Disambiguation:** when `service.version` is present, processing matches that version first;
   otherwise it may use the latest applicable successful/in-progress deployment.
4. **Injection:** the resolved `deployment_id` is stamped into normalized telemetry before storage.
5. **Fallback:** telemetry remains valid if no deployment mapping is available. Query-time
   correlation may reconstruct relationships from service/environment/version and event time.

The current ingest-gateway PostgreSQL deployment registry is transitional and is removed when the
control/event projection path is proven.

### 18.6 Security and RBAC

Access to the Deployment API is controlled by the project-level roles defined in `spec/14-domain-model.md`:

| Operation | Required Role |
|---|---|
| `POST /v1/deployments` | `Member`, `ProjectAdmin`, `TenantAdmin` |
| `PATCH /v1/deployments/*` | `Member`, `ProjectAdmin`, `TenantAdmin` |
| `GET /v1/deployments` | `Viewer`, `Member`, `ProjectAdmin`, `TenantAdmin` |

**API Authentication**: CI/CD pipelines SHOULD use Service Accounts or Project-scoped API keys to interact with the Deployment API.

### 18.7 Retention and Storage

*   **Marker Records**: Deployment entities (metadata) MUST be stored in the **Warm** retention tier (default 60 days) to match other Event types, but SHOULD be archived to **Cold** storage for up to 1 year to support long-term trend analysis.
*   **Signal Correlation**: The `deployment_id` dimension on Spans, Logs, and Metrics follows the retention policy of the parent signal.

### 18.8 Automation and Tooling

To ensure markers are consistent and accurate, deployment tooling MUST automate interaction with the Deployment API.

1.  **CI/CD Integration**: Pipelines (GitHub Actions, Argo CD hooks) SHOULD call the stable public
    `POST /v1/deployments` endpoint at the start of a rollout and
    `PATCH /v1/deployments/{id}` upon completion or failure. Internal service routing is owned by
    the distribution/gateway layer.
2.  **Canary Support**: The `scripts/canary-promote.sh` utility (see `spec/12-deployment.md`) SHOULD be updated to create a deployment marker when a canary is initiated and update it when promoted or reverted.
3.  **Automatic Rollback Detection**: If a deployment is rolled back (either manually or via automated gates), a new deployment record with status `rolled_back` and `rollback_of` set to the failed deployment ID MUST be created.

### 18.9 Generic Change Events

Not every operationally-relevant change is a deployment. The `change_events` table
(distinct from `deployment_markers`) covers config changes, feature-flag toggles,
schema migrations, and ad-hoc incident annotations — anything teams want correlated
against telemetry that isn't a service deploy.

| Field | Type | Required | Notes |
|---|---|---|---|
| change_event_id | UUID | yes | Unique identifier for the event. |
| tenant_id | UUID | yes | Partitioning key for multi-tenancy. |
| project_id | UUID | no | Project boundary, if applicable. |
| event_type | enum | yes | `config_change`, `feature_flag`, `migration`, `incident`, `other`. |
| service_name | string | no | Omitted for tenant/environment-wide events. |
| environment | string | yes | Target environment. |
| title | string | yes | Short human-readable summary. |
| description | string | no | Longer free-text detail. |
| occurred_at | timestamp | yes | When the change took effect. |
| source | string | no | Originating system, e.g. `launchdarkly`, `ci`, `manual`. |
| created_by | string | no | Identity of the user or system that recorded the event. |
| metadata | JSON | no | Arbitrary key-value context. |

**Control API**: `POST /v1/events/changes` and `GET /v1/events/changes` are owned by
`observable-control`. The list endpoint filters `service_name`, `environment`, `event_type`,
`start_time`, and `end_time`; `limit` defaults to 50 and is capped at 200. Query/correlation
surfaces consume this control-plane contract or a versioned change-event stream rather than reading
the control database directly.

**UI Visualization**: change events render as dashed vertical markers alongside
deployment markers on the same service-level time-series chart
(`apps/frontend/src/components/ui/time-series-graph.tsx`), distinguished by a
diamond marker shape (vs. deployments' triangle) and per-`event_type` color, with
a hover tooltip showing `title`, `event_type`, and `source`. A dedicated
`/change-events` explorer page lists and filters events independent of any chart.

**Retention**: follows the same Warm/Cold policy as deployment markers (§18.7).

**RBAC**: same roles as the Deployment API (§18.6) — `Member`/`ProjectAdmin`/`TenantAdmin`
for `POST`, all roles including `Viewer` for `GET`.
