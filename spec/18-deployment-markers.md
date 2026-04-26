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

#### Ingestion API

The platform MUST provide endpoints for lifecycle management of deployments.
These endpoints are served on the **Platform API port (4321)** of the ingest-gateway,
separate from the OTLP ports (4317/4318). CI/CD pipelines and tooling MUST target
port 4321 (env var `OBSERVABLE_URL=http://<host>:4321`).

1.  **Start Deployment**: `POST /v1/deployments`
    *   Creates a new marker with status `in_progress`.
    *   Returns the `deployment_id`.
2.  **Finish Deployment**: `PATCH /v1/deployments/{deployment_id}`
    *   Updates the `status`, `finished_at`, and adds any final metadata.
    *   Transition to `success`, `failed`, or `rolled_back`.

#### Query API

The Query API MUST support retrieving deployment markers to enable UI overlays.

1.  **List Deployments**: `GET /v1/deployments`
    *   Filters: `service_name`, `environment`, `start_time`, `end_time`.
    *   Used by the frontend to render vertical markers on time-series charts.

### 18.4 UI Visualization

*   **Timeline Overlay**: Deployment markers should appear as vertical lines on all service-specific charts (RED metrics).
*   **Status Indicators**: Markers should be color-coded by status (e.g., green for success, red for failure).
*   **Hover Context**: Hovering over a marker should display version, committer, and a link to the CI/CD pipeline.

### 18.5 Ingest Enrichment Logic

To enable correlation without requiring agents to be aware of `deployment_id`, the ingestion pipeline MUST enrich incoming signals with the active `deployment_id`.

1.  **Registry Lookup**: The ingestor maintains a cache of active deployments (status = `in_progress` or the latest `success`) indexed by `(tenant_id, service_name, environment, service_version)`.
2.  **Disambiguation**:
    *   If a signal carries `service.version`, the ingestor MUST match against the deployment record for that specific version.
    *   If `service.version` is missing or no version-specific match is found, the ingestor SHOULD match against the latest `success` or `in_progress` deployment for that `(service_name, environment)`.
3.  **Injection**: The resolved `deployment_id` is added to the internal signal representation before it is written to the storage layer (ClickHouse/VictoriaMetrics).

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

1.  **CI/CD Integration**: Pipelines (GitHub Actions, Argo CD hooks) SHOULD call `POST /v1/deployments` at the start of a rollout and `PATCH /v1/deployments/{id}` upon completion or failure.
2.  **Canary Support**: The `scripts/canary-promote.sh` utility (see `spec/12-deployment.md`) SHOULD be updated to create a deployment marker when a canary is initiated and update it when promoted or reverted.
3.  **Automatic Rollback Detection**: If a deployment is rolled back (either manually or via automated gates), a new deployment record with status `rolled_back` and `rollback_of` set to the failed deployment ID MUST be created.
