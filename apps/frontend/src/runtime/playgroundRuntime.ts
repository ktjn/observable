import type { Span, TraceHistogramResponse, TraceListResponse, TraceResponse } from "../api/traces";
import type { LogRecord, LogHistogramResponse, LogListResponse } from "../api/logs";
import type { TenantListResponse, EnvironmentListResponse } from "../api/tenants";
import type { NlqRequest, NlqResponse, NlqIr, VisualizationFrame } from "../api/nlq";
import type {
  Dashboard,
  DashboardPanel,
  CreateDashboardRequest,
  UpdateDashboardRequest,
  DashboardListResponse,
  DashboardExport,
} from "../api/dashboards";
import type {
  ServiceSummaryResponse,
  DiscoveryResponse,
  TopologyResponse,
  ServiceDetailResponse,
  ResponseTimeHistoryResponse,
} from "../api/services";
import type { MetricCatalogResponse, MetricPointsResponse, MetricCatalogEntry } from "../api/metrics";
import type { ListChangeEventsResponse, ListChangeEventsParams } from "../api/changeEvents";
import type {
  AlertRuleListResponse,
  AlertRuleItem,
  AlertRuleDetailResponse,
  CreateRuleRequest,
  CreateRuleResponse,
} from "../api/alerts";
import type { IncidentDetailResponse, IncidentListResponse, IncidentItem } from "../api/incidents";
import type { ListDeploymentsParams, ListDeploymentsResponse, DeploymentMarker } from "../api/deployments";
import type { CreateSloRequest, SloDefinitionItem, SloListResponse } from "../api/slos";
import type { CreateChannelRequest, NotificationChannelItem } from "../api/notifications";
import type {
  CreateSavedViewRequest,
  GrantItem,
  GrantListResponse,
  SavedView,
  SavedViewListResponse,
  SignalKind,
  UpdateSavedViewRequest,
} from "../api/savedViews";
import type {
  InfrastructureDetailResponse,
  InfrastructureEntitySummary,
  InfrastructureEntityType,
  InfrastructureInventoryResponse,
} from "../api/infrastructure";
import type {
  RuntimeApi,
  TraceHistogramParams,
  LogHistogramParams,
  LogSearchParams,
  LogTailParams,
  ServiceSummaryParams,
  TopologyParams,
} from "./types";
// Type-only import (erased at compile time, so the Worker-URL concern in the
// comment below does not apply).
import type { NlqLogRow } from "../playground/engineClient";
// Dynamically imported (not a static import): engineClient statically
// references a Worker URL, which Vite's dep scanner eagerly resolves at
// import time. A static import here would drag that into every module that
// imports playgroundRuntime.ts, including production (non-playground)
// tests — see the identical issue with PlaygroundSpike in router.ts.

/**
 * In-memory stub for most operations; `nlq.execute` and `*.histogram` are
 * wired to the real playground engine (Rust-planned DuckDB-WASM query) for
 * the Traces and Logs pages' page-load/filter-pill shapes (no free-text
 * question, operation="table"). Free-text NLQ questions and every other
 * operation/signal still fall back to fixture data. See Phase 3/4 follow-up
 * in docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md.
 */

// Must match useTenantContext.tsx's DEFAULT_TENANT_ID / DEFAULT_TENANT_NAME and
// useAuth.ts's PLAYGROUND_USER tenant id — the seeded "observable" tenant.
const DEMO_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const DEMO_TENANT_NAME = "observable";

const STUB_SPAN: Span = {
  span_id: "span-1",
  trace_id: "playground-trace-1",
  tenant_id: DEMO_TENANT_ID,
  service_name: "checkout",
  service_namespace: "observable-playground",
  service_version: "0.0.0",
  operation_name: "POST /checkout",
  span_kind: "SERVER",
  start_time_unix_nano: 0,
  end_time_unix_nano: 12_000_000,
  duration_ns: 12_000_000,
  status_code: "OK",
  status_message: "",
  attributes: {},
  resource_attributes: {},
  environment: "production",
  host_id: "playground-host",
  workload: "checkout",
  deployment_id: "playground-deployment",
};

const STUB_TRACE: TraceResponse = {
  trace_id: STUB_SPAN.trace_id,
  spans: [STUB_SPAN],
  events: [],
};

interface NlqTraceRow {
  trace_id: string;
  root_service: string;
  root_operation: string;
  duration_ms: number;
  status_code: string;
  environment?: string;
  start_time_unix_nano: number | string;
}

const NOW_NS = () => BigInt(Date.now()) * 1_000_000n;

const STUB_NLQ_TRACE_ROWS: NlqTraceRow[] = [
  {
    trace_id: "playground-trace-1",
    root_service: "checkout",
    root_operation: "POST /checkout",
    duration_ms: 12,
    status_code: "OK",
    environment: "production",
    start_time_unix_nano: String(NOW_NS()),
  },
  {
    trace_id: "playground-trace-2",
    root_service: "payment",
    root_operation: "POST /charge",
    duration_ms: 340,
    status_code: "ERROR",
    environment: "production",
    start_time_unix_nano: String(NOW_NS() - 30_000_000_000n),
  },
];

const STUB_NLQ_IR: NlqIr = {
  operation: "table",
  signals: ["traces"],
  filters: [],
  group_by: [],
  time_range: { from: "now-1h", to: "now" },
  metric: null,
  window: null,
  resolution: null,
  visualization_hint: "table",
};

const STUB_NLQ_FRAME: VisualizationFrame = {
  frame_type: "table",
  x_field: null,
  y_field: null,
  series_field: null,
  unit: null,
  suggested_visualization: "table",
  field_roles: [],
  data: STUB_NLQ_TRACE_ROWS as unknown as Record<string, unknown>[],
  nlq_ir: STUB_NLQ_IR,
  source_sql: "-- playground fixture data, not executed",
  time_range: { from: "now-1h", to: "now" },
  signal_types: ["traces"],
  sample_rate: null,
  approximation_statement: "",
};

const STUB_LOG_RECORDS: LogRecord[] = [
  {
    tenant_id: DEMO_TENANT_ID,
    log_id: "playground-log-1",
    timestamp_unix_nano: Number(NOW_NS()),
    observed_timestamp_unix_nano: Number(NOW_NS()),
    severity_number: 9,
    severity_text: "INFO",
    body: "checkout request completed",
    attributes: {},
    resource_attributes: {},
    service_name: "checkout",
    environment: "production",
    host_id: "playground-host",
  },
  {
    tenant_id: DEMO_TENANT_ID,
    log_id: "playground-log-2",
    timestamp_unix_nano: Number(NOW_NS() - 30_000_000_000n),
    observed_timestamp_unix_nano: Number(NOW_NS() - 30_000_000_000n),
    severity_number: 17,
    severity_text: "ERROR",
    body: "payment charge failed: card declined",
    attributes: {},
    resource_attributes: {},
    service_name: "payment",
    environment: "production",
    host_id: "playground-host",
  },
];

const STUB_LOG_IR: NlqIr = {
  operation: "table",
  signals: ["logs"],
  filters: [],
  group_by: [],
  time_range: { from: "now-1h", to: "now" },
  metric: null,
  window: null,
  resolution: null,
  visualization_hint: "table",
};

const STUB_LOG_FRAME: VisualizationFrame = {
  frame_type: "table",
  x_field: null,
  y_field: null,
  series_field: null,
  unit: null,
  suggested_visualization: "table",
  field_roles: [],
  data: STUB_LOG_RECORDS as unknown as Record<string, unknown>[],
  nlq_ir: STUB_LOG_IR,
  source_sql: "-- playground fixture data, not executed",
  time_range: { from: "now-1h", to: "now" },
  signal_types: ["logs"],
  sample_rate: null,
  approximation_statement: "",
};

/**
 * Dashboards are user-created content, not generated analytical data, so
 * they live in a plain in-memory `Map` rather than a DuckDB table — no SQL
 * shape to plan, just CRUD over JSON. Cleared on a full page reload, same
 * lifetime as the rest of the playground's demo data, but *not* touched by
 * "Reset playground" (that only regenerates spans/logs/metrics/change
 * events — a user's saved dashboards aren't demo data to discard).
 */
const dashboardStore = new Map<string, Dashboard>();
let nextDashboardId = 1;
let nextPanelId = 1;

/**
 * Alert-rule / incident / deployment fixtures. Like the dashboards store,
 * these are control-plane content with no DuckDB table to plan against, so
 * they are static demo rows shaped exactly like the production API
 * responses (Phase 5 contract: response shapes match production frontend
 * types). Timestamps are derived at call time so the rows always look
 * recent relative to a page load.
 */
function alertRulesFixture(): AlertRuleItem[] {
  const nowIso = new Date().toISOString();
  const tenMinAgoIso = new Date(Date.now() - 10 * 60_000).toISOString();
  return [
    {
      rule_id: "playground-rule-1",
      name: "payment error rate > 5%",
      metric_name: "http.server.errors.rate",
      operator: "gt",
      threshold: 5,
      severity: "critical",
      silenced: false,
      state: "active",
      firing: true,
      last_fired_at: tenMinAgoIso,
      notification_channels: [],
      auto_trigger_incident: true,
      service_name: "payment",
      suppressed: false,
    },
    {
      rule_id: "playground-rule-2",
      name: "checkout p95 latency > 500ms",
      metric_name: "http.server.duration",
      operator: "gt",
      threshold: 500,
      severity: "warning",
      silenced: false,
      state: "ok",
      firing: false,
      notification_channels: [],
      auto_trigger_incident: false,
      service_name: "checkout",
      suppressed: false,
    },
    {
      rule_id: "playground-rule-3",
      name: "web request rate drop",
      metric_name: "http.server.request.rate",
      operator: "lt",
      threshold: 1,
      severity: "warning",
      silenced: false,
      state: "ok",
      firing: false,
      last_fired_at: nowIso,
      notification_channels: [],
      auto_trigger_incident: false,
      service_name: "web",
      suppressed: false,
    },
  ];
}

/**
 * Mutable in-memory stores for the control-plane fixtures. Unlike the
 * analytical data (regenerated by Reset), these behave like dashboards:
 * mutations (silence/create) persist for the page's lifetime.
 */
const alertRuleStore: AlertRuleItem[] = alertRulesFixture();

const sloStore: SloDefinitionItem[] = [
  {
    slo_id: "playground-slo-1",
    service_name: "checkout",
    environment: "production",
    sli_type: "availability",
    target: 99.9,
    window_days: 30,
    burn_rate_fast_threshold: 14.4,
    burn_rate_slow_threshold: 6,
    description: "checkout availability SLO",
    firing: false,
    created_at: new Date(Date.now() - 7 * 86_400_000).toISOString(),
    updated_at: new Date(Date.now() - 7 * 86_400_000).toISOString(),
  },
];

const notificationChannelStore: NotificationChannelItem[] = [
  {
    channel_id: "playground-channel-1",
    name: "demo webhook",
    channel_type: "webhook",
    config: { url: "https://example.invalid/hook" },
  },
];

/**
 * Saved views live in a plain in-memory store (same lifetime/lifecycle as
 * dashboards — user content, not generated analytical data), keyed by
 * signal kind. Seeded with one demo logs view; grants are tracked per view
 * with the playground user as owner.
 */
const savedViewStore = new Map<string, SavedView>();
const savedViewGrantStore = new Map<string, GrantItem[]>();
let nextSavedViewId = 1;

function seedSavedViews(): void {
  const nowIso = new Date().toISOString();
  const demoView: SavedView = {
    saved_view_id: "playground-view-1",
    name: "Errors only (demo)",
    signal_kind: "logs",
    visibility: "private",
    config: {
      query: null,
      severity_filter: "ERROR",
      time_range: { mode: "preset", preset: "1h" },
      visible_columns: ["timestamp", "severity", "service", "body"],
    },
    created_at: nowIso,
    updated_at: nowIso,
  };
  savedViewStore.set(demoView.saved_view_id, demoView);
  savedViewGrantStore.set(demoView.saved_view_id, [
    { user_id: "playground-user", relation: "owner", granted_at: nowIso },
  ]);
}
seedSavedViews();

/**
 * Infrastructure inventory fixtures: a small deterministic entity tree
 * (host -> cluster -> namespace -> pods -> containers) covering the demo
 * services, shaped exactly like the production `InfrastructureEntitySummary`.
 */
function infrastructureFixture(): InfrastructureEntitySummary[] {
  const nowNs = Number(NOW_NS());
  const fiveMinAgo = nowNs - 5 * 60_000_000_000;
  const base = {
    environment: "production" as const,
    health_state: "healthy" as const,
    last_seen_unix_nano: nowNs,
  };
  return [
    {
      ...base,
      entity_type: "host" as const,
      entity_id: "playground-host-1",
      display_name: "demo-node-1",
      parent_id: null,
      parent_display_name: null,
      related_services: ["checkout", "payment", "web", "api-gateway"],
      log_rate_per_minute: 42,
      error_rate: 0.02,
      restart_count: 0,
      cpu_usage: 38,
      memory_usage: 61,
      disk_usage: 47,
      network_io: 1_204
    },
    {
      ...base,
      entity_type: "cluster" as const,
      entity_id: "playground-cluster-1",
      display_name: "demo-cluster",
      parent_id: "playground-host-1",
      parent_display_name: "demo-node-1",
      related_services: ["checkout", "payment"],
      log_rate_per_minute: 30,
      error_rate: 0.03,
      restart_count: 0,
      cpu_usage: 45,
      memory_usage: 58,
      disk_usage: 40,
      network_io: 980
    },
    {
      ...base,
      entity_type: "namespace" as const,
      entity_id: "playground-ns-demo",
      display_name: "demo",
      parent_id: "playground-cluster-1",
      parent_display_name: "demo-cluster",
      related_services: ["checkout", "payment"],
      log_rate_per_minute: 25,
      error_rate: 0.03,
      restart_count: 1,
      cpu_usage: 41,
      memory_usage: 55,
      disk_usage: 39,
      network_io: 860
    },
    {
      ...base,
      entity_type: "pod" as const,
      entity_id: "playground-pod-checkout",
      display_name: "checkout-7f9d8b6c5-x2p4k",
      parent_id: "playground-ns-demo",
      parent_display_name: "demo",
      related_services: ["checkout"],
      log_rate_per_minute: 12,
      error_rate: 0.01,
      restart_count: 0,
      cpu_usage: 33,
      memory_usage: 48,
      disk_usage: null,
      network_io: 310
    },
    {
      ...base,
      entity_type: "container" as const,
      entity_id: "playground-container-checkout",
      display_name: "checkout",
      parent_id: "playground-pod-checkout",
      parent_display_name: "checkout-7f9d8b6c5-x2p4k",
      related_services: ["checkout"],
      log_rate_per_minute: 11,
      error_rate: 0.01,
      restart_count: 0,
      cpu_usage: 31,
      memory_usage: 46,
      disk_usage: null,
      network_io: 300
    },
    {
      environment: "production" as const,
      entity_type: "pod" as const,
      entity_id: "playground-pod-payment",
      display_name: "payment-6c4d7a9b8-m8wqz",
      parent_id: "playground-ns-demo",
      parent_display_name: "demo",
      related_services: ["payment"],
      health_state: "breach" as const,
      last_seen_unix_nano: fiveMinAgo,
      log_rate_per_minute: 18,
      error_rate: 0.14,
      restart_count: 2,
      cpu_usage: 72,
      memory_usage: 81,
      disk_usage: null,
      network_io: 420
    },
    {
      environment: "production" as const,
      entity_type: "container" as const,
      entity_id: "playground-container-payment",
      display_name: "payment",
      parent_id: "playground-pod-payment",
      parent_display_name: "payment-6c4d7a9b8-m8wqz",
      related_services: ["payment"],
      health_state: "watch" as const,
      last_seen_unix_nano: fiveMinAgo,
      log_rate_per_minute: 17,
      error_rate: 0.13,
      restart_count: 2,
      cpu_usage: 70,
      memory_usage: 79,
      disk_usage: null,
      network_io: 410
    },
  ];
}

function infrastructureDetailFixture(
  entityType: InfrastructureEntityType,
  entityId: string
): InfrastructureDetailResponse | null {
  const entity = infrastructureFixture().find(
    (e) => e.entity_type === entityType && e.entity_id === entityId
  );
  if (!entity) return null;
  return {
    entity,
    links: {
      logs: `/logs?service=${encodeURIComponent(entity.related_services[0] ?? "")}`,
      traces: `/traces?service=${encodeURIComponent(entity.related_services[0] ?? "")}`,
      metrics: `/metrics?service=${encodeURIComponent(entity.related_services[0] ?? "")}`,
    },
  };
}

const incidentsData: IncidentItem[] = (() => {
  const fortyMinAgoIso = new Date(Date.now() - 40 * 60_000).toISOString();
  return [
    {
      incident_id: "playground-incident-1",
      title: "High error rate on payment",
      severity: "critical",
      status: "triggered",
      triggered_at: fortyMinAgoIso,
      triggered_by_rule_id: "playground-rule-1",
    },
  ];
})();

function incidentDetailFixture(incidentId: string): IncidentDetailResponse | null {
  const incident = incidentsData.find((i) => i.incident_id === incidentId);
  if (!incident) return null;
  return {
    ...incident,
    dedup_key: `dedup-${incident.incident_id}`,
    resolved_at: incident.resolved_at ?? null,
    triggered_by_rule_id: incident.triggered_by_rule_id ?? null,
    runbook_url: null,
    rule_name:
      alertRuleStore.find((r) => r.rule_id === incident.triggered_by_rule_id)?.name ?? null,
    timeline: [
      {
        event_time: incident.triggered_at,
        event_type: "triggered",
        actor: "alert-evaluator",
        message: `Incident triggered: ${incident.title}`,
      },
    ],
    impacted_service:
      alertRuleStore.find((r) => r.rule_id === incident.triggered_by_rule_id)?.service_name ??
      null,
  };
}

function deploymentsFixture(params: ListDeploymentsParams): DeploymentMarker[] {  const nowMs = Date.now();
  const all: DeploymentMarker[] = [
    {
      deployment_id: "playground-deployment-3",
      tenant_id: DEMO_TENANT_ID,
      project_id: null,
      service_name: "payment",
      environment: "production",
      service_version: "2.4.0",
      status: "in_progress",
      started_at: new Date(nowMs - 15 * 60_000).toISOString(),
      finished_at: null,
      deployed_by: "playground-user",
      commit_sha: "a1b2c3d",
      rollback_of: null,
      metadata: null,
    },
    {
      deployment_id: "playground-deployment-2",
      tenant_id: DEMO_TENANT_ID,
      project_id: null,
      service_name: "payment",
      environment: "production",
      service_version: "2.3.1",
      status: "success",
      started_at: new Date(nowMs - 26 * 3_600_000).toISOString(),
      finished_at: new Date(nowMs - 26 * 3_600_000 + 4 * 60_000).toISOString(),
      deployed_by: "playground-user",
      commit_sha: "d4e5f6a",
      rollback_of: null,
      metadata: null,
    },
    {
      deployment_id: "playground-deployment-1",
      tenant_id: DEMO_TENANT_ID,
      project_id: null,
      service_name: "checkout",
      environment: "production",
      service_version: "1.18.0",
      status: "success",
      started_at: new Date(nowMs - 50 * 3_600_000).toISOString(),
      finished_at: new Date(nowMs - 50 * 3_600_000 + 7 * 60_000).toISOString(),
      deployed_by: "playground-user",
      commit_sha: "b8c9d0e",
      rollback_of: null,
      metadata: null,
    },
  ];
  return all.filter(
    (d) =>
      (!params.service_name || d.service_name === params.service_name) &&
      (!params.environment || d.environment === params.environment),
  );
}

function makeDashboardId(): string {
  return `playground-dashboard-${nextDashboardId++}`;
}

function makePanelId(): string {
  return `playground-panel-${nextPanelId++}`;
}

function panelFromCreateRequest(panel: CreateDashboardRequest["panels"][number]): DashboardPanel {
  return {
    panel_id: makePanelId(),
    title: panel.title,
    panel_kind: panel.panel_kind ?? "query",
    query_kind: panel.query_kind ?? undefined,
    service: panel.service,
    preset: panel.preset ?? undefined,
    filters: panel.filters,
    query_text: panel.query_text ?? undefined,
    content: panel.content ?? undefined,
    layout: panel.layout ?? { x: 0, y: 0, w: 6, h: 4 },
    time_range: panel.time_range ?? { mode: "global" },
  };
}

function panelFromUpdateRequest(panel: UpdateDashboardRequest["panels"][number]): DashboardPanel {
  return {
    panel_id: panel.panel_id ?? makePanelId(),
    title: panel.title,
    panel_kind: panel.panel_kind,
    query_kind: panel.query_kind ?? undefined,
    service: panel.service ?? undefined,
    preset: panel.preset ?? undefined,
    filters: panel.filters,
    query_text: panel.query_text ?? undefined,
    content: panel.content ?? undefined,
    layout: panel.layout,
    time_range: panel.time_range,
  };
}

function requireDashboard(dashboardId: string): Dashboard {
  const dashboard = dashboardStore.get(dashboardId);
  if (!dashboard) {
    throw new Error(`Dashboard not found: ${dashboardId}`);
  }
  return dashboard;
}

/**
 * `question` doubles as either free-text NLQ or a raw JSON-encoded `NlqIr`
 * — the "Simple IR Shorthand" power-user bypass (ADR-034), which
 * production's `parse_user_query_input` recognizes by a leading `{`. Locked-
 * service views (LogSearch.tsx/TraceSearch.tsx's `serviceName` prop) rely on
 * this to pre-set a service filter without going through the LLM. Mirrored
 * here so those views get real DuckDB-backed rows instead of falling
 * through to the free-text stub path below.
 */
function parseRawIrQuestion(question: string | undefined): NlqIr | null {
  if (!question) return null;
  const trimmed = question.trim();
  if (!trimmed.startsWith("{")) return null;
  try {
    const parsed = JSON.parse(trimmed);
    if (parsed && typeof parsed === "object" && typeof parsed.operation === "string" && Array.isArray(parsed.signals)) {
      return parsed as NlqIr;
    }
  } catch {
    // Not JSON — a genuine free-text question, handled by the caller.
  }
  return null;
}

/** Mirrors `llm_adapter.rs::merge_irs` (operation/signals always come from `base`). */
function mergeIrs(base: NlqIr, user: NlqIr): NlqIr {
  const userFilterFields = new Set(user.filters.map((f) => f.field.toLowerCase()));
  const mergedFilters = [
    ...base.filters.filter((f) => !userFilterFields.has(f.field.toLowerCase())),
    ...user.filters,
  ];
  return {
    ...base,
    filters: mergedFilters,
    time_range: user.time_range?.from ? user.time_range : base.time_range,
    query: user.query,
  };
}

/**
 * Resolves a "now"/"now-Xh" relative time expression to an absolute
 * nanosecond-epoch decimal string. `query-core::sql_templates::parse_time_expr`
 * (shared, ClickHouse-flavored production code reused as-is by the DuckDB
 * dialect renderers — see trace_query.rs/log_query.rs) emits
 * `toUnixTimestamp64Nano(now64())`-style ClickHouse SQL for these, which
 * DuckDB doesn't understand. Page-load `base_ir` always arrives pre-resolved
 * to absolute ns already (a plain digit string passes through unchanged
 * here), but `mergeIrs` can surface a raw-IR question's still-relative
 * `LOG_BASE_IR`/`TRACE_BASE_IR`-style time_range, so resolve it locally
 * before it ever reaches the Rust query planner.
 */
function resolveTimeExpr(expr: string, nowNs: bigint): string {
  const trimmed = expr.trim();
  if (trimmed === "now") return nowNs.toString();
  const match = /^now-(\d+)([smhd])$/.exec(trimmed);
  if (!match) return trimmed;
  const nanosPerUnit: Record<string, bigint> = {
    s: 1_000_000_000n,
    m: 60_000_000_000n,
    h: 3_600_000_000_000n,
    d: 86_400_000_000_000n,
  };
  return (nowNs - BigInt(match[1]) * nanosPerUnit[match[2]]).toString();
}

function resolveTimeRange(timeRange: NlqIr["time_range"]): NlqIr["time_range"] {
  const nowNs = BigInt(Date.now()) * 1_000_000n;
  return {
    from: resolveTimeExpr(timeRange.from, nowNs),
    to: resolveTimeExpr(timeRange.to, nowNs),
  };
}

/**
 * The engine worker returns log timestamps as decimal strings (avoiding JS
 * Number precision loss at ns scale); the production `LogRecord` wire shape
 * carries them as numbers, matching what the HTTP API returns. Convert at
 * the seam so both runtimes satisfy one contract.
 */
function nlqRowToLogRecord(row: NlqLogRow): LogRecord {
  return {
    tenant_id: row.tenant_id,
    log_id: row.log_id,
    timestamp_unix_nano: Number(row.timestamp_unix_nano),
    observed_timestamp_unix_nano: Number(row.observed_timestamp_unix_nano),
    severity_number: row.severity_number,
    severity_text: row.severity_text,
    body: row.body,
    trace_id: row.trace_id,
    span_id: row.span_id,
    attributes: row.attributes,
    resource_attributes: row.resource_attributes,
    service_name: row.service_name,
    environment: row.environment,
    host_id: row.host_id,
  };
}

export const playgroundRuntime: RuntimeApi = {
  mode: "playground",
  tenants: {
    async list(): Promise<TenantListResponse> {
      return { tenants: [{ id: DEMO_TENANT_ID, name: DEMO_TENANT_NAME }] };
    },
    async listEnvironments(): Promise<EnvironmentListResponse> {
      return { environments: [{ environment: "production" }, { environment: "staging" }] };
    },
  },
  traces: {
    async search(): Promise<TraceListResponse> {
      return {
        traces: [STUB_TRACE],
        total: 1,
        facets: { service_name: [{ value: "checkout", count: 1 }] },
      };
    },
    async get(_tenantId: string, traceId: string): Promise<TraceResponse> {
      const { executeTraceDetail } = await import("../playground/engineClient");
      const { spans } = await executeTraceDetail(traceId);
      if (spans.length === 0) {
        // Mirror production's 404 so the page renders "Trace not found."
        throw new Error(`Not found: ${traceId}`);
      }
      return { trace_id: traceId, spans, events: [] };
    },
    async histogram(_tenantId: string, params: TraceHistogramParams): Promise<TraceHistogramResponse> {
      if (params.from && params.to) {
        const { executeTraceHistogram } = await import("../playground/engineClient");
        const { buckets } = await executeTraceHistogram({
          fromNs: params.from,
          toNs: params.to,
          bucketCount: params.buckets ?? 30,
          service: params.service,
        });
        return { buckets };
      }
      return { buckets: [{ start_ms: 0, end_ms: 60_000, count: 1 }] };
    },
  },
  logs: {
    async histogram(_tenantId: string, params: LogHistogramParams): Promise<LogHistogramResponse> {
      if (params.from && params.to) {
        const { executeLogHistogram } = await import("../playground/engineClient");
        const { buckets } = await executeLogHistogram({
          fromNs: params.from,
          toNs: params.to,
          bucketCount: params.buckets ?? 30,
          service: params.service,
        });
        return { buckets };
      }
      return { buckets: [{ start_ms: 0, end_ms: 60_000, counts: { "9": 1, "17": 1 } }] };
    },
    async search(_tenantId: string, params: LogSearchParams): Promise<LogListResponse> {
      const { executeLogsSearch } = await import("../playground/engineClient");
      const { logs } = await executeLogsSearch({
        traceId: params.trace_id,
        service: params.service,
        limit: params.limit ?? 100,
      });
      return { logs: logs.map(nlqRowToLogRecord), total: logs.length, facets: {} };
    },
    async context(_tenantId: string, logId: string, params?: { window?: number }): Promise<LogListResponse> {
      const { executeLogsContext } = await import("../playground/engineClient");
      const { logs } = await executeLogsContext({ logId, window: params?.window });
      return { logs: logs.map(nlqRowToLogRecord), total: logs.length, facets: {} };
    },
    async tail(_tenantId: string, params: LogTailParams): Promise<LogListResponse> {
      const { executeLogsTail } = await import("../playground/engineClient");
      const { logs } = await executeLogsTail({
        service: params.service,
        severity: params.severity,
        sinceUnixNano: params.since_unix_nano,
        limit: params.limit ?? 100,
      });
      return { logs: logs.map(nlqRowToLogRecord), total: logs.length, facets: {} };
    },
  },
  services: {
    async list(_tenantId: string, params: ServiceSummaryParams): Promise<ServiceSummaryResponse> {
      const nowMs = Date.now();
      const fromMs = params.from ?? nowMs - 3_600_000;
      const toMs = params.to ?? nowMs;
      const { executeServiceSummaries } = await import("../playground/engineClient");
      const { items } = await executeServiceSummaries({
        fromNs: String(BigInt(Math.floor(fromMs)) * 1_000_000n),
        toNs: String(BigInt(Math.floor(toMs)) * 1_000_000n),
        environment: params.environment,
      });
      return { items };
    },
    async listNames(): Promise<DiscoveryResponse> {
      const { executeServiceNames } = await import("../playground/engineClient");
      const { names } = await executeServiceNames();
      return { items: names };
    },
    async summary(
      _tenantId: string,
      serviceName: string,
      params: ServiceSummaryParams
    ): Promise<ServiceDetailResponse> {
      const nowMs = Date.now();
      const fromMs = params.from ?? nowMs - 3_600_000;
      const toMs = params.to ?? nowMs;
      const { executeServiceSummaries } = await import("../playground/engineClient");
      const { items } = await executeServiceSummaries({
        fromNs: String(BigInt(Math.floor(fromMs)) * 1_000_000n),
        toNs: String(BigInt(Math.floor(toMs)) * 1_000_000n),
        environment: params.environment,
      });
      const service = items.find((item) => item.service_name === serviceName);
      if (!service) {
        throw new Error(`Service not found: ${serviceName}`);
      }
      return { service };
    },
    async responseTimeHistory(
      _tenantId: string,
      serviceName: string,
      params: { from?: number; to?: number; buckets?: number }
    ): Promise<ResponseTimeHistoryResponse> {
      const nowMs = Date.now();
      const fromMs = params.from ?? nowMs - 3_600_000;
      const toMs = params.to ?? nowMs;
      const { executeResponseTimeHistogram } = await import("../playground/engineClient");
      const { buckets } = await executeResponseTimeHistogram({
        fromNs: String(BigInt(Math.floor(fromMs)) * 1_000_000n),
        toNs: String(BigInt(Math.floor(toMs)) * 1_000_000n),
        bucketCount: params.buckets ?? 60,
        serviceName,
      });
      return { buckets };
    },
  },
  topology: {
    async get(_tenantId: string, params: TopologyParams): Promise<TopologyResponse> {
      const nowMs = Date.now();
      const fromMs = params.from ?? nowMs - 3_600_000;
      const toMs = params.to ?? nowMs;
      const { executeTopology } = await import("../playground/engineClient");
      const { edges } = await executeTopology({
        fromNs: String(BigInt(Math.floor(fromMs)) * 1_000_000n),
        toNs: String(BigInt(Math.floor(toMs)) * 1_000_000n),
        environment: params.environment,
        service: params.service,
      });
      return { edges };
    },
  },
  metrics: {
    async list(_tenantId: string, params: { service?: string }): Promise<MetricCatalogResponse> {
      const { executeMetricCatalog } = await import("../playground/engineClient");
      const { metrics } = await executeMetricCatalog(params.service);
      return { metrics };
    },
    async points(_tenantId: string, metric: MetricCatalogEntry): Promise<MetricPointsResponse> {
      const { executeMetricGroupPoints } = await import("../playground/engineClient");
      const { points } = await executeMetricGroupPoints(metric);
      return { points };
    },
  },
  changeEvents: {
    async list(_tenantId: string, params: ListChangeEventsParams): Promise<ListChangeEventsResponse> {
      const nowMs = Date.now();
      const fromMs = params.start_time ? new Date(params.start_time).getTime() : nowMs - 3_600_000;
      const toMs = params.end_time ? new Date(params.end_time).getTime() : nowMs;
      const { executeChangeEvents } = await import("../playground/engineClient");
      const { changeEvents } = await executeChangeEvents({
        fromNs: String(BigInt(Math.floor(fromMs)) * 1_000_000n),
        toNs: String(BigInt(Math.floor(toMs)) * 1_000_000n),
        service: params.service_name,
        environment: params.environment,
        eventType: params.event_type,
        limit: params.limit ?? 50,
      });
      return { items: changeEvents };
    },
  },
  alerts: {
    async list(): Promise<AlertRuleListResponse> {
      return { items: alertRuleStore };
    },
    async get(_tenantId: string, ruleId: string): Promise<AlertRuleDetailResponse> {
      const rule = alertRuleStore.find((r) => r.rule_id === ruleId);
      if (!rule) {
        throw new Error(`Failed to get alert rule: 404`);
      }
      return {
        rule_id: rule.rule_id,
        name: rule.name,
        severity: rule.severity,
        alert_type: "threshold",
        condition: {
          metric_name: rule.metric_name,
          operator: rule.operator,
          threshold: rule.threshold,
        },
        silenced: rule.silenced,
        firing: rule.firing,
        firings:
          rule.firing && rule.last_fired_at
            ? [
                {
                  firing_id: `${rule.rule_id}-firing-1`,
                  state: "active" as const,
                  value: rule.threshold * 2,
                  occurred_at: rule.last_fired_at,
                },
              ]
            : [],
        runbook_url: null,
      };
    },
    async create(_tenantId: string, req: CreateRuleRequest): Promise<CreateRuleResponse> {
      const ruleId = `playground-rule-${alertRuleStore.length + 1}-${Date.now() % 10_000}`;
      alertRuleStore.push({
        rule_id: ruleId,
        name: req.name,
        metric_name: req.metric_name,
        operator: (req.operator as AlertRuleItem["operator"]) ?? "gt",
        threshold: req.threshold,
        severity: "warning",
        silenced: false,
        state: "ok",
        firing: false,
        notification_channels: req.notification_channels ?? [],
        auto_trigger_incident: req.auto_trigger_incident ?? false,
        service_name: req.service_name,
        suppressed: false,
      });
      return { rule_id: ruleId };
    },
    async silence(_tenantId: string, ruleId: string, silenced: boolean): Promise<void> {
      const rule = alertRuleStore.find((r) => r.rule_id === ruleId);
      if (!rule) {
        throw new Error(`Failed to update alert rule: 404`);
      }
      rule.silenced = silenced;
      rule.state = silenced ? "silenced" : rule.firing ? "active" : "ok";
    },
    async setRunbook(_tenantId: string, ruleId: string, runbookUrl: string | null): Promise<void> {
      // The playground's list-shaped fixture carries no runbook field; the
      // write is accepted and reflected only in the detail response cache.
      void ruleId;
      void runbookUrl;
    },
  },
  incidents: {
    async list(_tenantId: string, status?: string): Promise<IncidentListResponse> {
      return {
        items: status ? incidentsData.filter((i) => i.status === status) : incidentsData,
      };
    },
    async get(_tenantId: string, incidentId: string): Promise<IncidentDetailResponse> {
      const detail = incidentDetailFixture(incidentId);
      if (!detail) {
        throw new Error(`Failed to get incident: 404`);
      }
      return detail;
    },
  },
  slos: {
    async list(): Promise<SloListResponse> {
      return { items: sloStore };
    },
    async create(_tenantId: string, req: CreateSloRequest): Promise<SloDefinitionItem> {
      const nowIso = new Date().toISOString();
      const slo: SloDefinitionItem = {
        slo_id: `playground-slo-${sloStore.length + 1}-${Date.now() % 10_000}`,
        service_name: req.service_name,
        environment: req.environment,
        sli_type: "availability",
        target: req.target,
        window_days: req.window_days,
        burn_rate_fast_threshold: req.burn_rate_fast_threshold,
        burn_rate_slow_threshold: req.burn_rate_slow_threshold,
        description: req.description ?? "",
        firing: false,
        created_at: nowIso,
        updated_at: nowIso,
      };
      sloStore.push(slo);
      return slo;
    },
  },
  notificationChannels: {
    async list(): Promise<NotificationChannelItem[]> {
      return notificationChannelStore;
    },
    async create(_tenantId: string, req: CreateChannelRequest): Promise<NotificationChannelItem> {
      const channel: NotificationChannelItem = {
        channel_id: `playground-channel-${notificationChannelStore.length + 1}-${Date.now() % 10_000}`,
        name: req.name,
        channel_type: req.channel_type,
        config: req.config,
      };
      notificationChannelStore.push(channel);
      return channel;
    },
    async delete(_tenantId: string, channelId: string): Promise<void> {
      const idx = notificationChannelStore.findIndex((c) => c.channel_id === channelId);
      if (idx >= 0) notificationChannelStore.splice(idx, 1);
    },
  },
  savedViews: {
    async list(_tenantId: string, signalKind: SignalKind): Promise<SavedViewListResponse> {
      return {
        items: [...savedViewStore.values()].filter((v) => v.signal_kind === signalKind),
      };
    },
    async create(_tenantId: string, req: CreateSavedViewRequest): Promise<SavedView> {
      const nowIso = new Date().toISOString();
      const view: SavedView = {
        saved_view_id: `playground-view-${nextSavedViewId++}`,
        name: req.name,
        signal_kind: req.signal_kind,
        visibility: "private",
        config: req.config,
        created_at: nowIso,
        updated_at: nowIso,
      };
      savedViewStore.set(view.saved_view_id, view);
      savedViewGrantStore.set(view.saved_view_id, [
        { user_id: "playground-user", relation: "owner", granted_at: nowIso },
      ]);
      return view;
    },
    async update(
      _tenantId: string,
      savedViewId: string,
      req: UpdateSavedViewRequest
    ): Promise<SavedView> {
      const existing = savedViewStore.get(savedViewId);
      if (!existing) {
        throw new Error(`Saved view update failed: 404`);
      }
      const updated: SavedView = {
        ...existing,
        name: req.name,
        config: req.config,
        visibility: req.visibility ?? existing.visibility,
        updated_at: new Date().toISOString(),
      };
      savedViewStore.set(savedViewId, updated);
      return updated;
    },
    async delete(_tenantId: string, savedViewId: string): Promise<void> {
      savedViewStore.delete(savedViewId);
      savedViewGrantStore.delete(savedViewId);
    },
    async listGrants(_tenantId: string, savedViewId: string): Promise<GrantListResponse> {
      return { grants: savedViewGrantStore.get(savedViewId) ?? [] };
    },
    async addGrant(
      _tenantId: string,
      savedViewId: string,
      userId: string,
      relation: GrantItem["relation"]
    ): Promise<void> {
      if (!savedViewStore.has(savedViewId)) {
        throw new Error(`Saved view grant add failed: 404`);
      }
      const grants = savedViewGrantStore.get(savedViewId) ?? [];
      if (!grants.some((g) => g.user_id === userId)) {
        grants.push({ user_id: userId, relation, granted_at: new Date().toISOString() });
      }
      savedViewGrantStore.set(savedViewId, grants);
    },
    async revokeGrant(_tenantId: string, savedViewId: string, userId: string): Promise<void> {
      const grants = savedViewGrantStore.get(savedViewId) ?? [];
      savedViewGrantStore.set(
        savedViewId,
        grants.filter((g) => g.user_id !== userId)
      );
    },
  },
  infrastructure: {
    async list(
      _tenantId: string,
      params: { service?: string; environment?: string; entity_type?: string }
    ): Promise<InfrastructureInventoryResponse> {
      let items = infrastructureFixture();
      if (params.service) {
        items = items.filter((e) => e.related_services.includes(params.service!));
      }
      if (params.environment) {
        items = items.filter((e) => e.environment === params.environment);
      }
      if (params.entity_type) {
        items = items.filter((e) => e.entity_type === params.entity_type);
      }
      return { items };
    },
    async get(
      _tenantId: string,
      entityType: InfrastructureEntityType,
      entityId: string
    ): Promise<InfrastructureDetailResponse> {
      const detail = infrastructureDetailFixture(entityType, entityId);
      if (!detail) {
        throw new Error(`Query failed: 404`);
      }
      return detail;
    },
  },
  deployments: {
    async list(_tenantId: string, params: ListDeploymentsParams): Promise<ListDeploymentsResponse> {
      return { items: deploymentsFixture(params) };
    },
  },
  nlq: {
    async execute(_tenantId: string, request: NlqRequest): Promise<NlqResponse> {
      const rawIr = parseRawIrQuestion(request.question);
      let ir = rawIr
        ? request.base_ir
          ? mergeIrs(request.base_ir as NlqIr, rawIr)
          : rawIr
        : (request.base_ir as NlqIr | undefined);
      if (ir) {
        ir = { ...ir, time_range: resolveTimeRange(ir.time_range) };
      }
      // A genuine free-text question (not the raw-IR shorthand) has no
      // deterministic local answer — falls through to the fixture below,
      // same as before.
      const hasFreeTextQuestion = Boolean(request.question) && !rawIr;

      if (
        !hasFreeTextQuestion &&
        ir &&
        ir.operation === "table" &&
        ir.signals?.length === 1 &&
        ir.signals[0] === "traces"
      ) {
        const { executeTraceTable } = await import("../playground/engineClient");
        const { rows, sql } = await executeTraceTable(ir);
        return {
          type: "frame",
          frame: {
            ...STUB_NLQ_FRAME,
            data: rows as unknown as Record<string, unknown>[],
            nlq_ir: ir,
            source_sql: sql,
          },
        };
      }

      if (
        !hasFreeTextQuestion &&
        ir &&
        ir.operation === "table" &&
        ir.signals?.length === 1 &&
        ir.signals[0] === "logs"
      ) {
        const { executeLogTable } = await import("../playground/engineClient");
        const { rows, sql } = await executeLogTable(ir);
        return {
          type: "frame",
          frame: {
            ...STUB_LOG_FRAME,
            data: rows as unknown as Record<string, unknown>[],
            nlq_ir: ir,
            source_sql: sql,
          },
        };
      }

      // Free-text NLQ questions and every other operation/signal still use
      // fixture data — shorthand/IR-merge logic isn't wired yet. Match the
      // fixture shape to the requested signal so at least the response
      // *shape* is correct even when the content is static.
      if (!hasFreeTextQuestion && ir?.signals?.length === 1 && ir.signals[0] === "logs") {
        return { type: "frame", frame: STUB_LOG_FRAME };
      }
      if (!hasFreeTextQuestion && ir?.operation === "inventory") {
        return {
          type: "frame",
          frame: {
            ...STUB_NLQ_FRAME,
            data: infrastructureFixture() as unknown as Record<string, unknown>[],
            nlq_ir: ir,
            source_sql: "-- playground infrastructure fixtures, not executed",
          },
        };
      }
      return { type: "frame", frame: STUB_NLQ_FRAME };
    },
  },
  dashboards: {
    async list(): Promise<DashboardListResponse> {
      return { items: Array.from(dashboardStore.values()) };
    },
    async get(_tenantId: string, dashboardId: string): Promise<Dashboard> {
      return requireDashboard(dashboardId);
    },
    async create(_tenantId: string, request: CreateDashboardRequest): Promise<Dashboard> {
      const dashboard: Dashboard = {
        dashboard_id: makeDashboardId(),
        name: request.name,
        visibility: "private",
        panels: request.panels.map(panelFromCreateRequest),
        created_at: new Date().toISOString(),
      };
      dashboardStore.set(dashboard.dashboard_id, dashboard);
      return dashboard;
    },
    async update(_tenantId: string, dashboardId: string, request: UpdateDashboardRequest): Promise<Dashboard> {
      const existing = requireDashboard(dashboardId);
      const updated: Dashboard = {
        ...existing,
        name: request.name,
        panels: request.panels.map(panelFromUpdateRequest),
      };
      dashboardStore.set(dashboardId, updated);
      return updated;
    },
    async delete(_tenantId: string, dashboardId: string): Promise<void> {
      dashboardStore.delete(dashboardId);
    },
    async export(_tenantId: string, dashboardId: string): Promise<DashboardExport> {
      const dashboard = requireDashboard(dashboardId);
      return {
        schema_version: "1",
        name: dashboard.name,
        panels: dashboard.panels.map((panel) => ({
          title: panel.title,
          panel_kind: panel.panel_kind,
          query_kind: panel.query_kind,
          service: panel.service,
          preset: panel.preset as DashboardExport["panels"][number]["preset"],
          filters: panel.filters as Record<string, unknown>,
          query_text: panel.query_text,
          content: panel.content,
          layout: panel.layout,
          time_range: panel.time_range as DashboardExport["panels"][number]["time_range"],
        })),
      };
    },
    async import(_tenantId: string, export_: DashboardExport): Promise<Dashboard> {
      const dashboard: Dashboard = {
        dashboard_id: makeDashboardId(),
        name: export_.name,
        visibility: "private",
        panels: export_.panels.map((panel) =>
          panelFromCreateRequest({
            title: panel.title,
            panel_kind: panel.panel_kind,
            query_kind: panel.query_kind,
            service: panel.service ?? undefined,
            preset: panel.preset ?? null,
            filters: panel.filters,
            query_text: panel.query_text,
            content: panel.content,
            layout: panel.layout,
            time_range: panel.time_range,
          })
        ),
        created_at: new Date().toISOString(),
      };
      dashboardStore.set(dashboard.dashboard_id, dashboard);
      return dashboard;
    },
  },
};
