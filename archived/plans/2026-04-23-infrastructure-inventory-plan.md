# Infrastructure Inventory and Detail Views Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add tenant-scoped infrastructure inventory and detail views for host, cluster, namespace, pod, and container entities, with direct navigation to related services, logs, traces, and metrics.

**Architecture:** Keep the first slice narrow by extending the existing `query-api` discovery surface with one normalized infrastructure inventory response and one normalized infrastructure detail response. On the frontend, replace the current Infrastructure placeholder with a dedicated inventory page and a dedicated detail page, both backed by one shared `infrastructure.ts` API client and TanStack Router routes.

**Tech Stack:** Rust (`axum`, `serde`, `clickhouse`, `sqlx`), React 19, TanStack Router, TanStack Query, Vitest, Testing Library, npm workspaces

---

## File Map

| Action | Path | Purpose |
|--------|------|---------|
| Modify | `services/query-api/src/discovery.rs` | Add normalized infrastructure types, validation helpers, inventory/detail handlers, and unit tests |
| Modify | `services/query-api/src/main.rs` | Register new infrastructure routes |
| Create | `apps/frontend/src/api/infrastructure.ts` | Frontend types and fetch helpers for infrastructure inventory/detail |
| Create | `apps/frontend/src/pages/InfrastructureInventoryPage.tsx` | Shared infrastructure inventory UI |
| Create | `apps/frontend/src/pages/InfrastructureDetailPage.tsx` | Shared infrastructure detail UI |
| Modify | `apps/frontend/src/router.ts` | Route `/infrastructure` to the real inventory page and add `/infrastructure/$entityType/$entityId` |
| Modify | `apps/frontend/src/pages/ProductAreaPage.tsx` | Remove infrastructure placeholder responsibility so the file remains service-focused |
| Modify | `apps/frontend/src/App.test.tsx` | Cover the new infrastructure inventory/detail routes and navigation |
| Modify | `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md` | Mark `P3-S9` complete and record the checkpoint answer after implementation |

---

### Task 1: Add Backend Infrastructure Model and Helper Tests

**Files:**
- Modify: `services/query-api/src/discovery.rs`
- Test: `services/query-api/src/discovery.rs`

- [ ] **Step 1: Write the failing backend helper tests**

Add these tests at the bottom of `services/query-api/src/discovery.rs` inside `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn infrastructure_entity_type_attribute_keys_are_stable() {
        assert_eq!(InfrastructureEntityType::Host.attribute_key(), "host.name");
        assert_eq!(InfrastructureEntityType::Cluster.attribute_key(), "k8s.cluster.name");
        assert_eq!(InfrastructureEntityType::Namespace.attribute_key(), "k8s.namespace.name");
        assert_eq!(InfrastructureEntityType::Pod.attribute_key(), "k8s.pod.name");
        assert_eq!(InfrastructureEntityType::Container.attribute_key(), "container.name");
    }

    #[test]
    fn infrastructure_entity_type_rejects_unknown_values() {
        assert_eq!(
            InfrastructureEntityType::try_from("rack"),
            Err(StatusCode::BAD_REQUEST)
        );
    }

    #[test]
    fn infrastructure_health_state_prefers_errors_then_log_volume() {
        assert_eq!(infrastructure_health_state(0.0, 0.0), "healthy");
        assert_eq!(infrastructure_health_state(0.02, 0.0), "watch");
        assert_eq!(infrastructure_health_state(0.08, 0.0), "breach");
    }

    #[test]
    fn infrastructure_detail_link_uses_canonical_resource_attribute() {
        let links = infrastructure_links(
            InfrastructureEntityType::Pod,
            "checkout-pod-1",
            Some("checkout-api".into()),
        );

        assert_eq!(
            links.logs,
            "/logs?resource_attr=k8s.pod.name:checkout-pod-1"
        );
        assert_eq!(
            links.traces,
            "/traces?resource_attr=k8s.pod.name:checkout-pod-1"
        );
        assert_eq!(
            links.metrics,
            "/services/checkout-api/metrics?resource_attr=k8s.pod.name:checkout-pod-1"
        );
    }
```

- [ ] **Step 2: Run the targeted Rust test to verify it fails**

Run:

```bash
cargo test -p query-api infrastructure_entity_type_attribute_keys_are_stable
```

Expected: FAIL because `InfrastructureEntityType`, `infrastructure_health_state`, and `infrastructure_links` do not exist yet.

- [ ] **Step 3: Write the minimal backend model and helpers**

Add these definitions near the other discovery types in `services/query-api/src/discovery.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfrastructureEntityType {
    Host,
    Cluster,
    Namespace,
    Pod,
    Container,
}

impl InfrastructureEntityType {
    pub fn attribute_key(self) -> &'static str {
        match self {
            Self::Host => "host.name",
            Self::Cluster => "k8s.cluster.name",
            Self::Namespace => "k8s.namespace.name",
            Self::Pod => "k8s.pod.name",
            Self::Container => "container.name",
        }
    }
}

impl TryFrom<&str> for InfrastructureEntityType {
    type Error = StatusCode;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "host" => Ok(Self::Host),
            "cluster" => Ok(Self::Cluster),
            "namespace" => Ok(Self::Namespace),
            "pod" => Ok(Self::Pod),
            "container" => Ok(Self::Container),
            _ => Err(StatusCode::BAD_REQUEST),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InfrastructureLinks {
    pub logs: String,
    pub traces: String,
    pub metrics: String,
}

fn infrastructure_health_state(error_rate: f64, _log_rate_per_minute: f64) -> &'static str {
    if error_rate >= 0.05 {
        "breach"
    } else if error_rate >= 0.01 {
        "watch"
    } else {
        "healthy"
    }
}

fn infrastructure_links(
    entity_type: InfrastructureEntityType,
    entity_id: &str,
    primary_service: Option<String>,
) -> InfrastructureLinks {
    let attr = entity_type.attribute_key();
    let encoded = format!("{attr}:{entity_id}");
    let metrics = primary_service
        .map(|service| format!("/services/{service}/metrics?resource_attr={encoded}"))
        .unwrap_or_else(|| format!("/metrics?resource_attr={encoded}"));

    InfrastructureLinks {
        logs: format!("/logs?resource_attr={encoded}"),
        traces: format!("/traces?resource_attr={encoded}"),
        metrics,
    }
}
```

- [ ] **Step 4: Run the targeted Rust test to verify it passes**

Run:

```bash
cargo test -p query-api infrastructure_entity_type_attribute_keys_are_stable infrastructure_detail_link_uses_canonical_resource_attribute
```

Expected: PASS for the new helper tests.

- [ ] **Step 5: Commit**

```bash
git add services/query-api/src/discovery.rs
git commit -m "feat(query-api): add infrastructure discovery helpers"
```

---

### Task 2: Add Backend Inventory and Detail Handlers

**Files:**
- Modify: `services/query-api/src/discovery.rs`
- Modify: `services/query-api/src/main.rs`
- Test: `services/query-api/src/discovery.rs`

- [ ] **Step 1: Write the failing backend response-shape tests**

Extend `services/query-api/src/discovery.rs` tests with:

```rust
    #[test]
    fn infrastructure_inventory_response_serializes_entity_rows() {
        let response = InfrastructureInventoryResponse {
            items: vec![InfrastructureEntitySummary {
                entity_type: InfrastructureEntityType::Pod,
                entity_id: "checkout-pod-1".into(),
                display_name: "checkout-pod-1".into(),
                parent_id: Some("payments".into()),
                parent_display_name: Some("payments".into()),
                environment: Some("prod".into()),
                health_state: "watch".into(),
                last_seen_unix_nano: 42,
                related_services: vec!["checkout-api".into()],
                log_rate_per_minute: Some(8.5),
                error_rate: Some(0.02),
                restart_count: None,
                cpu_usage: None,
                memory_usage: None,
                disk_usage: None,
                network_io: None,
            }],
        };

        let json = serde_json::to_value(response).unwrap();
        assert_eq!(json["items"][0]["entity_type"], "pod");
        assert_eq!(json["items"][0]["entity_id"], "checkout-pod-1");
        assert_eq!(json["items"][0]["related_services"][0], "checkout-api");
    }

    #[test]
    fn infrastructure_detail_response_embeds_links() {
        let response = InfrastructureDetailResponse {
            entity: InfrastructureEntitySummary {
                entity_type: InfrastructureEntityType::Host,
                entity_id: "ip-10-0-0-12".into(),
                display_name: "ip-10-0-0-12".into(),
                parent_id: None,
                parent_display_name: None,
                environment: Some("prod".into()),
                health_state: "healthy".into(),
                last_seen_unix_nano: 100,
                related_services: vec!["checkout-api".into()],
                log_rate_per_minute: Some(1.0),
                error_rate: Some(0.0),
                restart_count: None,
                cpu_usage: None,
                memory_usage: None,
                disk_usage: None,
                network_io: None,
            },
            links: infrastructure_links(
                InfrastructureEntityType::Host,
                "ip-10-0-0-12",
                Some("checkout-api".into()),
            ),
        };

        let json = serde_json::to_value(response).unwrap();
        assert_eq!(json["links"]["logs"], "/logs?resource_attr=host.name:ip-10-0-0-12");
    }
```

- [ ] **Step 2: Run the targeted Rust test to verify it fails**

Run:

```bash
cargo test -p query-api infrastructure_inventory_response_serializes_entity_rows
```

Expected: FAIL because `InfrastructureInventoryResponse`, `InfrastructureEntitySummary`, and `InfrastructureDetailResponse` do not exist yet.

- [ ] **Step 3: Add the minimal normalized responses and handler skeletons**

In `services/query-api/src/discovery.rs`, add:

```rust
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InfrastructureEntitySummary {
    pub entity_type: InfrastructureEntityType,
    pub entity_id: String,
    pub display_name: String,
    pub parent_id: Option<String>,
    pub parent_display_name: Option<String>,
    pub environment: Option<String>,
    pub health_state: String,
    pub last_seen_unix_nano: u64,
    pub related_services: Vec<String>,
    pub log_rate_per_minute: Option<f64>,
    pub error_rate: Option<f64>,
    pub restart_count: Option<u64>,
    pub cpu_usage: Option<f64>,
    pub memory_usage: Option<f64>,
    pub disk_usage: Option<f64>,
    pub network_io: Option<f64>,
}

#[derive(Serialize)]
pub struct InfrastructureInventoryResponse {
    pub items: Vec<InfrastructureEntitySummary>,
}

#[derive(Serialize)]
pub struct InfrastructureDetailResponse {
    pub entity: InfrastructureEntitySummary,
    pub links: InfrastructureLinks,
}

#[derive(Deserialize)]
pub struct InfrastructureInventoryParams {
    pub entity_type: Option<String>,
    pub environment: Option<String>,
    pub service: Option<String>,
    pub search: Option<String>,
    pub lookback_minutes: Option<u32>,
}

#[derive(Deserialize)]
pub struct InfrastructureDetailParams {
    pub environment: Option<String>,
    pub lookback_minutes: Option<u32>,
}

pub async fn list_infrastructure_inventory(
    Query(params): Query<InfrastructureInventoryParams>,
) -> Result<Json<InfrastructureInventoryResponse>, StatusCode> {
    if let Some(entity_type) = params.entity_type.as_deref() {
        let _ = InfrastructureEntityType::try_from(entity_type)?;
    }
    Ok(Json(InfrastructureInventoryResponse { items: Vec::new() }))
}

pub async fn get_infrastructure_detail(
    Path((entity_type, entity_id)): Path<(String, String)>,
) -> Result<Json<InfrastructureDetailResponse>, StatusCode> {
    let entity_type = InfrastructureEntityType::try_from(entity_type.as_str())?;
    let links = infrastructure_links(entity_type, &entity_id, None);
    Err(StatusCode::NOT_FOUND).map(|_| Json(InfrastructureDetailResponse {
        entity: InfrastructureEntitySummary {
            entity_type,
            entity_id: entity_id.clone(),
            display_name: entity_id,
            parent_id: None,
            parent_display_name: None,
            environment: None,
            health_state: "healthy".into(),
            last_seen_unix_nano: 0,
            related_services: Vec::new(),
            log_rate_per_minute: None,
            error_rate: None,
            restart_count: None,
            cpu_usage: None,
            memory_usage: None,
            disk_usage: None,
            network_io: None,
        },
        links,
    }))
}
```

Then register the routes in `services/query-api/src/main.rs`:

```rust
        .route("/v1/infrastructure", get(discovery::list_infrastructure_inventory))
        .route(
            "/v1/infrastructure/:entity_type/:entity_id",
            get(discovery::get_infrastructure_detail),
        )
```

After the skeleton passes compile, replace the `Vec::new()` and `NOT_FOUND` stub with the real query implementation:

- derive entities from `spans`, `logs`, and `metric_series`
- compute `last_seen_unix_nano`, `related_services`, `environment`, `log_rate_per_minute`, and `error_rate`
- apply tenant, environment, service, entity-type, and search filters
- return `404` when detail does not exist

- [ ] **Step 4: Run the targeted Rust tests to verify they pass**

Run:

```bash
cargo test -p query-api infrastructure_inventory_response_serializes_entity_rows infrastructure_detail_response_embeds_links
```

Expected: PASS for the new response-shape tests.

- [ ] **Step 5: Run broader query-api tests**

Run:

```bash
cargo test -p query-api
```

Expected: PASS, including existing `discovery`, `logs`, `metrics`, `traces`, `planner`, and `middleware` tests.

- [ ] **Step 6: Commit**

```bash
git add services/query-api/src/discovery.rs services/query-api/src/main.rs
git commit -m "feat(query-api): add infrastructure inventory endpoints"
```

---

### Task 3: Add Frontend Infrastructure API Client and Route Coverage

**Files:**
- Create: `apps/frontend/src/api/infrastructure.ts`
- Modify: `apps/frontend/src/router.ts`
- Modify: `apps/frontend/src/App.test.tsx`
- Test: `apps/frontend/src/App.test.tsx`

- [ ] **Step 1: Write the failing frontend route tests**

Add these tests to `apps/frontend/src/App.test.tsx`:

```tsx
test("renders infrastructure inventory rows from the infrastructure API", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);

      if (url.includes("/v1/infrastructure?")) {
        return new Response(
          JSON.stringify({
            items: [
              {
                entity_type: "pod",
                entity_id: "checkout-pod-1",
                display_name: "checkout-pod-1",
                parent_id: "payments",
                parent_display_name: "payments",
                environment: "prod",
                health_state: "watch",
                last_seen_unix_nano: 42,
                related_services: ["checkout-api"],
                log_rate_per_minute: 8.5,
                error_rate: 0.02,
                restart_count: null,
                cpu_usage: null,
                memory_usage: null,
                disk_usage: null,
                network_io: null,
              },
            ],
          }),
          { status: 200 },
        );
      }

      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: ["prod"] }), { status: 200 });
      }

      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/infrastructure");
  render(<App />);

  expect(await screen.findByRole("heading", { name: "Infrastructure" })).toBeInTheDocument();
  expect(await screen.findByText("checkout-pod-1")).toBeInTheDocument();
  expect(screen.getByText("checkout-api")).toBeInTheDocument();
});

test("renders infrastructure detail action links", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);

      if (url.includes("/v1/infrastructure/pod/checkout-pod-1")) {
        return new Response(
          JSON.stringify({
            entity: {
              entity_type: "pod",
              entity_id: "checkout-pod-1",
              display_name: "checkout-pod-1",
              parent_id: "payments",
              parent_display_name: "payments",
              environment: "prod",
              health_state: "watch",
              last_seen_unix_nano: 42,
              related_services: ["checkout-api"],
              log_rate_per_minute: 8.5,
              error_rate: 0.02,
              restart_count: null,
              cpu_usage: null,
              memory_usage: null,
              disk_usage: null,
              network_io: null,
            },
            links: {
              logs: "/logs?resource_attr=k8s.pod.name:checkout-pod-1",
              traces: "/traces?resource_attr=k8s.pod.name:checkout-pod-1",
              metrics: "/services/checkout-api/metrics?resource_attr=k8s.pod.name:checkout-pod-1",
            },
          }),
          { status: 200 },
        );
      }

      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/infrastructure/pod/checkout-pod-1");
  render(<App />);

  expect(await screen.findByRole("heading", { name: "checkout-pod-1" })).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "Logs" })).toHaveAttribute(
    "href",
    "/logs?resource_attr=k8s.pod.name:checkout-pod-1",
  );
});
```

- [ ] **Step 2: Run the targeted frontend test to verify it fails**

Run:

```bash
npm --workspace apps/frontend test -- src/App.test.tsx
```

Expected: FAIL because the infrastructure routes and infrastructure API client do not exist yet.

- [ ] **Step 3: Add the minimal frontend API client and route wiring**

Create `apps/frontend/src/api/infrastructure.ts`:

```ts
const DEV_TENANT_ID = "00000000-0000-0000-0000-000000000001";

function tenantHeaders(): HeadersInit {
  return { "X-Tenant-ID": DEV_TENANT_ID };
}

export type InfrastructureEntityType =
  | "host"
  | "cluster"
  | "namespace"
  | "pod"
  | "container";

export interface InfrastructureEntitySummary {
  entity_type: InfrastructureEntityType;
  entity_id: string;
  display_name: string;
  parent_id: string | null;
  parent_display_name: string | null;
  environment: string | null;
  health_state: "healthy" | "watch" | "breach";
  last_seen_unix_nano: number;
  related_services: string[];
  log_rate_per_minute: number | null;
  error_rate: number | null;
  restart_count: number | null;
  cpu_usage: number | null;
  memory_usage: number | null;
  disk_usage: number | null;
  network_io: number | null;
}

export interface InfrastructureInventoryResponse {
  items: InfrastructureEntitySummary[];
}

export interface InfrastructureDetailResponse {
  entity: InfrastructureEntitySummary;
  links: {
    logs: string;
    traces: string;
    metrics: string;
  };
}

export async function listInfrastructure(params: Record<string, string> = {}) {
  const url = new URL("/v1/infrastructure", window.location.origin);
  for (const [key, value] of Object.entries(params)) {
    if (value) url.searchParams.set(key, value);
  }
  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return (await res.json()) as InfrastructureInventoryResponse;
}

export async function getInfrastructureDetail(entityType: string, entityId: string) {
  const url = new URL(
    `/v1/infrastructure/${encodeURIComponent(entityType)}/${encodeURIComponent(entityId)}`,
    window.location.origin,
  );
  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return (await res.json()) as InfrastructureDetailResponse;
}
```

Update `apps/frontend/src/router.ts` imports and routes:

```ts
import InfrastructureInventoryPage from "./pages/InfrastructureInventoryPage";
import InfrastructureDetailPage from "./pages/InfrastructureDetailPage";

const infrastructureRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/infrastructure",
  component: InfrastructureInventoryPage,
});

const infrastructureDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/infrastructure/$entityType/$entityId",
  component: InfrastructureDetailPage,
});
```

And include `infrastructureDetailRoute` in `routeTree`.

- [ ] **Step 4: Run the targeted frontend test to verify it still fails for the right reason**

Run:

```bash
npm --workspace apps/frontend test -- src/App.test.tsx
```

Expected: FAIL because the page components are still missing, not because routing or fetch wiring is broken.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/api/infrastructure.ts apps/frontend/src/router.ts apps/frontend/src/App.test.tsx
git commit -m "feat(frontend): add infrastructure API client and routes"
```

---

### Task 4: Implement the Infrastructure Inventory Page

**Files:**
- Create: `apps/frontend/src/pages/InfrastructureInventoryPage.tsx`
- Modify: `apps/frontend/src/pages/ProductAreaPage.tsx`
- Modify: `apps/frontend/src/App.test.tsx`
- Test: `apps/frontend/src/App.test.tsx`

- [ ] **Step 1: Write the failing inventory interaction test**

Add this test to `apps/frontend/src/App.test.tsx`:

```tsx
test("filters the infrastructure inventory by entity type", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);

      if (url.includes("/v1/infrastructure")) {
        return new Response(
          JSON.stringify({
            items: [
              {
                entity_type: "pod",
                entity_id: "checkout-pod-1",
                display_name: "checkout-pod-1",
                parent_id: "payments",
                parent_display_name: "payments",
                environment: "prod",
                health_state: "watch",
                last_seen_unix_nano: 42,
                related_services: ["checkout-api"],
                log_rate_per_minute: 8.5,
                error_rate: 0.02,
                restart_count: null,
                cpu_usage: null,
                memory_usage: null,
                disk_usage: null,
                network_io: null,
              },
              {
                entity_type: "host",
                entity_id: "ip-10-0-0-12",
                display_name: "ip-10-0-0-12",
                parent_id: null,
                parent_display_name: null,
                environment: "prod",
                health_state: "healthy",
                last_seen_unix_nano: 43,
                related_services: ["checkout-api"],
                log_rate_per_minute: 2.0,
                error_rate: 0.0,
                restart_count: null,
                cpu_usage: null,
                memory_usage: null,
                disk_usage: null,
                network_io: null,
              },
            ],
          }),
          { status: 200 },
        );
      }

      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: ["prod"] }), { status: 200 });
      }

      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/infrastructure");
  render(<App />);

  await screen.findByText("checkout-pod-1");
  fireEvent.change(screen.getByLabelText("Infrastructure type filter"), {
    target: { value: "host" },
  });

  expect(screen.queryByText("checkout-pod-1")).not.toBeInTheDocument();
  expect(screen.getByText("ip-10-0-0-12")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run the targeted frontend test to verify it fails**

Run:

```bash
npm --workspace apps/frontend test -- src/App.test.tsx
```

Expected: FAIL because the inventory page is not rendering real infrastructure filters or rows yet.

- [ ] **Step 3: Implement the inventory page with the smallest working UI**

Create `apps/frontend/src/pages/InfrastructureInventoryPage.tsx`:

```tsx
import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { listEnvironments } from "../api/services";
import { listInfrastructure, type InfrastructureEntitySummary, type InfrastructureEntityType } from "../api/infrastructure";

const entityTypeOptions: Array<"all" | InfrastructureEntityType> = [
  "all",
  "host",
  "cluster",
  "namespace",
  "pod",
  "container",
];

export default function InfrastructureInventoryPage() {
  const [environment, setEnvironment] = useState("all");
  const [entityType, setEntityType] = useState<"all" | InfrastructureEntityType>("all");
  const [search, setSearch] = useState("");

  const { data: environments } = useQuery({
    queryKey: ["environments"],
    queryFn: listEnvironments,
  });

  const { data, isLoading, error } = useQuery({
    queryKey: ["infrastructure", environment],
    queryFn: () =>
      listInfrastructure({
        environment: environment === "all" ? "" : environment,
      }),
  });

  const filteredItems = useMemo(() => {
    const items = data?.items ?? [];
    return items.filter((item) => {
      const matchesType = entityType === "all" || item.entity_type === entityType;
      const matchesSearch =
        search.length === 0 ||
        item.display_name.toLowerCase().includes(search.toLowerCase()) ||
        item.entity_id.toLowerCase().includes(search.toLowerCase());
      return matchesType && matchesSearch;
    });
  }, [data?.items, entityType, search]);

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="field-label">Inventory</div>
          <h1>Infrastructure</h1>
        </div>
      </div>
      <div className="toolbar-row">
        <input
          className="search-input"
          aria-label="Search infrastructure"
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
        <select
          className="select-input"
          aria-label="Infrastructure type filter"
          value={entityType}
          onChange={(event) => setEntityType(event.target.value as "all" | InfrastructureEntityType)}
        >
          {entityTypeOptions.map((option) => (
            <option key={option} value={option}>
              {option === "all" ? "All types" : option}
            </option>
          ))}
        </select>
        <select
          className="select-input"
          aria-label="Environment filter"
          value={environment}
          onChange={(event) => setEnvironment(event.target.value)}
        >
          <option value="all">All environments</option>
          {environments?.items.map((env) => (
            <option key={env} value={env}>
              {env}
            </option>
          ))}
        </select>
      </div>
      {isLoading ? <div className="loading-state">Loading infrastructure...</div> : null}
      {error ? <div className="empty-panel">Infrastructure query failed.</div> : null}
      {!isLoading && !error && filteredItems.length === 0 ? (
        <div className="empty-panel">No infrastructure entities matched the current filters.</div>
      ) : null}
      {!isLoading && !error && filteredItems.length > 0 ? (
        <div className="table-panel">
          <table>
            <thead>
              <tr>
                <th>Entity</th>
                <th>Type</th>
                <th>Environment</th>
                <th>Health</th>
                <th>Related services</th>
                <th>Log rate</th>
                <th>Error rate</th>
              </tr>
            </thead>
            <tbody>
              {filteredItems.map((item) => (
                <InfrastructureRow key={`${item.entity_type}:${item.entity_id}`} item={item} />
              ))}
            </tbody>
          </table>
        </div>
      ) : null}
    </section>
  );
}

function InfrastructureRow({ item }: { item: InfrastructureEntitySummary }) {
  return (
    <tr>
      <td className="strong-cell">
        <Link
          to="/infrastructure/$entityType/$entityId"
          params={{ entityType: item.entity_type, entityId: item.entity_id }}
        >
          {item.display_name}
        </Link>
      </td>
      <td>{item.entity_type}</td>
      <td>{item.environment ?? "n/a"}</td>
      <td>{item.health_state}</td>
      <td>{item.related_services.join(", ") || "n/a"}</td>
      <td>{item.log_rate_per_minute ?? "Unavailable"}</td>
      <td>{item.error_rate ?? "Unavailable"}</td>
    </tr>
  );
}
```

Then remove the Infrastructure placeholder branch from `apps/frontend/src/pages/ProductAreaPage.tsx` so that file stays focused on shared placeholder areas and the services page only.

- [ ] **Step 4: Run the targeted frontend test to verify it passes**

Run:

```bash
npm --workspace apps/frontend test -- src/App.test.tsx
```

Expected: PASS for the new inventory rendering and filtering tests.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/pages/InfrastructureInventoryPage.tsx apps/frontend/src/pages/ProductAreaPage.tsx apps/frontend/src/App.test.tsx
git commit -m "feat(frontend): add infrastructure inventory page"
```

---

### Task 5: Implement the Infrastructure Detail Page

**Files:**
- Create: `apps/frontend/src/pages/InfrastructureDetailPage.tsx`
- Modify: `apps/frontend/src/App.test.tsx`
- Test: `apps/frontend/src/App.test.tsx`

- [ ] **Step 1: Write the failing detail empty-state test**

Add this test to `apps/frontend/src/App.test.tsx`:

```tsx
test("renders unavailable placeholders for nullable infrastructure detail fields", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);

      if (url.includes("/v1/infrastructure/host/ip-10-0-0-12")) {
        return new Response(
          JSON.stringify({
            entity: {
              entity_type: "host",
              entity_id: "ip-10-0-0-12",
              display_name: "ip-10-0-0-12",
              parent_id: null,
              parent_display_name: null,
              environment: "prod",
              health_state: "healthy",
              last_seen_unix_nano: 43,
              related_services: ["checkout-api"],
              log_rate_per_minute: null,
              error_rate: null,
              restart_count: null,
              cpu_usage: null,
              memory_usage: null,
              disk_usage: null,
              network_io: null,
            },
            links: {
              logs: "/logs?resource_attr=host.name:ip-10-0-0-12",
              traces: "/traces?resource_attr=host.name:ip-10-0-0-12",
              metrics: "/services/checkout-api/metrics?resource_attr=host.name:ip-10-0-0-12",
            },
          }),
          { status: 200 },
        );
      }

      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/infrastructure/host/ip-10-0-0-12");
  render(<App />);

  await screen.findByRole("heading", { name: "ip-10-0-0-12" });
  expect(screen.getAllByText("Unavailable").length).toBeGreaterThan(0);
});
```

- [ ] **Step 2: Run the targeted frontend test to verify it fails**

Run:

```bash
npm --workspace apps/frontend test -- src/App.test.tsx
```

Expected: FAIL because the detail page component does not exist yet.

- [ ] **Step 3: Implement the detail page with link actions**

Create `apps/frontend/src/pages/InfrastructureDetailPage.tsx`:

```tsx
import { Link, useParams } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { getInfrastructureDetail } from "../api/infrastructure";

export default function InfrastructureDetailPage() {
  const { entityType, entityId } = useParams({
    from: "/infrastructure/$entityType/$entityId",
  });

  const { data, isLoading, error } = useQuery({
    queryKey: ["infrastructure-detail", entityType, entityId],
    queryFn: () => getInfrastructureDetail(entityType, entityId),
  });

  if (isLoading) return <section className="page-stack"><div className="loading-state">Loading infrastructure detail...</div></section>;
  if (error || !data) return <section className="page-stack"><div className="empty-panel">Infrastructure detail query failed.</div></section>;

  const { entity, links } = data;

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="field-label">{entity.entity_type}</div>
          <h1>{entity.display_name}</h1>
        </div>
      </div>
      <div className="metric-grid" aria-label="Infrastructure detail summary">
        <DetailTile label="Environment" value={entity.environment ?? "Unavailable"} />
        <DetailTile label="Health" value={entity.health_state} />
        <DetailTile label="Log rate" value={valueOrUnavailable(entity.log_rate_per_minute)} />
        <DetailTile label="Error rate" value={valueOrUnavailable(entity.error_rate)} />
      </div>
      <div className="table-panel" aria-label="Related services">
        {entity.related_services.map((service) => (
          <Link key={service} to="/services/$serviceId" params={{ serviceId: service }}>
            {service}
          </Link>
        ))}
      </div>
      <div className="table-panel" aria-label="Infrastructure actions">
        <a href={links.logs}>Logs</a>
        <a href={links.traces}>Traces</a>
        <a href={links.metrics}>Metrics</a>
      </div>
      <div className="table-panel" aria-label="Infrastructure metadata">
        <div>Parent: {entity.parent_display_name ?? "Unavailable"}</div>
        <div>Restart count: {valueOrUnavailable(entity.restart_count)}</div>
        <div>CPU usage: {valueOrUnavailable(entity.cpu_usage)}</div>
        <div>Memory usage: {valueOrUnavailable(entity.memory_usage)}</div>
        <div>Disk usage: {valueOrUnavailable(entity.disk_usage)}</div>
        <div>Network I/O: {valueOrUnavailable(entity.network_io)}</div>
      </div>
    </section>
  );
}

function DetailTile({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric-tile info">
      <div className="metric-label">{label}</div>
      <div className="metric-value">{value}</div>
    </div>
  );
}

function valueOrUnavailable(value: number | null): string {
  return value == null ? "Unavailable" : String(value);
}
```

- [ ] **Step 4: Run the targeted frontend test to verify it passes**

Run:

```bash
npm --workspace apps/frontend test -- src/App.test.tsx
```

Expected: PASS for the new detail-route and nullable-field coverage.

- [ ] **Step 5: Run the full frontend test/typecheck/lint set**

Run:

```bash
npm --workspace apps/frontend run typecheck
npm --workspace apps/frontend run lint
npm --workspace apps/frontend test
```

Expected: all frontend checks pass.

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/pages/InfrastructureDetailPage.tsx apps/frontend/src/App.test.tsx
git commit -m "feat(frontend): add infrastructure detail page"
```

---

### Task 6: Sync the Phase Plan and Run Required Verification

**Files:**
- Modify: `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`
- Test: repository verification commands

- [ ] **Step 1: Update the phase plan state**

In `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`, change the `P3-S9` checkbox and fill in the outcome/checkpoint block:

```md
- [x] **P3-S9: Add Infrastructure inventory and detail views**
  - Outcome: Infrastructure now provides host, cluster, namespace, pod, and container inventory/detail views backed by telemetry-derived resource attributes. The frontend exposes a shared inventory page and a shared detail page with links to related services, logs, traces, and metrics.
  - Checkpoint: can users move from infrastructure to related services, logs, metrics, and traces without manually reconstructing filters? Answer: yes. The detail response returns canonical link targets built from the entity's resource attribute, and the frontend renders those actions directly.
```

- [ ] **Step 2: Run documentation diff review**

Run:

```bash
git diff -- docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md
```

Expected: only the `P3-S9` block is updated.

- [ ] **Step 3: Run the narrow verification commands first**

Run:

```bash
cargo test -p query-api
npm --workspace apps/frontend run typecheck
npm --workspace apps/frontend run lint
npm --workspace apps/frontend test
```

Expected: all targeted backend and frontend checks pass before the full repo gate.

- [ ] **Step 4: Run the mandatory local gate**

Run:

```bash
bash scripts/local-ci.sh
```

Expected: PASS. This is required before push because the slice changes code.

- [ ] **Step 5: Run doc/spec review for the updated plan file**

Review the changed file under the `doc-review` process and capture:

```md
## Doc/Spec Review Report

### Phase 1: Structural Validation — PASS
### Phase 2: Cross-Reference Consistency — PASS
### Phase 3: Coverage Completeness — PASS
### Phase 4: Quality Gates — PASS

### Summary
Overall: PASS
Warnings requiring PR acknowledgement: 0
Blockers requiring fix before PR: 0
```

Expected: PASS because the plan update only records the completed slice state and checkpoint answer.

- [ ] **Step 6: Commit**

```bash
git add docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md
git commit -m "docs(plan): mark infrastructure inventory slice complete"
```

---

## Verification Plan for This Plan Document

This planning iteration is documentation-only. Required checks:

```bash
git diff --check
```

Expected:
- No whitespace errors.

ADR/spec synchronization:
- No ADR update is required for this implementation plan because it implements already-approved
  infrastructure inventory behavior from `spec/05-frontend.md` and `spec/09-api.md` without
  changing architecture, storage, tenancy, deployment, or roadmap scope.
