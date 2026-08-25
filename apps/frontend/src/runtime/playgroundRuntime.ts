import type { Span, TraceHistogramResponse, TraceListResponse, TraceResponse } from "../api/traces";
import type { LogRecord, LogHistogramResponse, LogListResponse } from "../api/logs";
import type { TenantListResponse, EnvironmentListResponse } from "../api/tenants";
import type { NlqRequest, NlqResponse, NlqIr, VisualizationFrame } from "../api/nlq";
import type {
  Dashboard,
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
  FirstSignalStatus,
  LlmModelsResult,
  PlatformConfig,
  SaveLlmConfigParams,
} from "../api/setup";
import type {
  CreateTokenRequest,
  CreateTokenResponse,
  TokenListResponse,
} from "../api/tokens";
import type { MemberListResponse, MemberRecord, TenantRole } from "../api/admin-members";
import type { TenantUsageReportResponse } from "../api/usage";
import type { MeResponse } from "../api/auth";
import type {
  RuntimeApi,
  TraceHistogramParams,
  LogHistogramParams,
  LogSearchParams,
  LogTailParams,
  ServiceSummaryParams,
  TopologyParams,
} from "./types";
import { SqliteSavedViewRepository } from "./sqliteSavedViewRepository";
import { SqliteDashboardRepository } from "./sqliteDashboardRepository";
import { SqliteAlertRuleRepository } from "./sqliteAlertRuleRepository";
import { SqliteSloRepository } from "./sqliteSloRepository";
import { SqliteNotificationChannelRepository } from "./sqliteNotificationChannelRepository";
import { SqliteTokenRepository } from "./sqliteTokenRepository";
import { SqliteMemberRepository } from "./sqliteMemberRepository";
import { SqlitePlatformConfigRepository } from "./sqlitePlatformConfigRepository";
import { SqliteIncidentRepository } from "./sqliteIncidentRepository";
import { SqliteAuthRepository } from "./sqliteAuthRepository";
import { SqliteInfrastructureRepository } from "./sqliteInfrastructureRepository";
import { SqliteDeploymentRepository } from "./sqliteDeploymentRepository";
import { SqliteUsageRepository } from "./sqliteUsageRepository";
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
void STUB_TRACE;

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
 * Dashboards are user-created control-plane content. The playground keeps
 * their production-shaped JSON payloads in a tenant-scoped SQLite adapter;
 * this browser-local database is intentionally separate from DuckDB telemetry.
 */
let dashboardRepositoryPromise: Promise<SqliteDashboardRepository> | undefined;

function dashboardRepository(): Promise<SqliteDashboardRepository> {
  dashboardRepositoryPromise ??= SqliteDashboardRepository.open();
  return dashboardRepositoryPromise;
}

/**
 * Alert-rule / incident / deployment fixtures. Like the dashboards store,
 * these are control-plane content with no DuckDB table to plan against, so
 * they are static demo rows shaped exactly like the production API
 * responses (Phase 5 contract: response shapes match production frontend
 * types). Timestamps are derived at call time so the rows always look
 * recent relative to a page load.
 */
/**
 * Mutable in-memory stores for the control-plane fixtures. Unlike the
 * analytical data (regenerated by Reset), these behave like dashboards:
 * mutations (silence/create) persist for the page's lifetime.
 */
let alertRuleRepositoryPromise: Promise<SqliteAlertRuleRepository> | undefined;

function alertRuleRepository(): Promise<SqliteAlertRuleRepository> {
  alertRuleRepositoryPromise ??= SqliteAlertRuleRepository.open();
  return alertRuleRepositoryPromise;
}

let sloRepositoryPromise: Promise<SqliteSloRepository> | undefined;
function sloRepository(): Promise<SqliteSloRepository> { sloRepositoryPromise ??= SqliteSloRepository.open(); return sloRepositoryPromise; }

let notificationChannelRepositoryPromise: Promise<SqliteNotificationChannelRepository> | undefined;
function notificationChannelRepository(): Promise<SqliteNotificationChannelRepository> { notificationChannelRepositoryPromise ??= SqliteNotificationChannelRepository.open(); return notificationChannelRepositoryPromise; }

/** The browser-local control-plane repository is initialized once per runtime. */
let savedViewRepositoryPromise: Promise<SqliteSavedViewRepository> | undefined;

function savedViewRepository(): Promise<SqliteSavedViewRepository> {
  savedViewRepositoryPromise ??= SqliteSavedViewRepository.open();
  return savedViewRepositoryPromise;
}

let infrastructureRepository: SqliteInfrastructureRepository | null = null;
async function getInfrastructureRepository(): Promise<SqliteInfrastructureRepository> {
  return (infrastructureRepository ??= await SqliteInfrastructureRepository.open());
}

let deploymentRepository: SqliteDeploymentRepository | null = null;
async function getDeploymentRepository(): Promise<SqliteDeploymentRepository> {
  return (deploymentRepository ??= await SqliteDeploymentRepository.open());
}

let usageRepository: SqliteUsageRepository | null = null;
async function getUsageRepository(): Promise<SqliteUsageRepository> {
  return (usageRepository ??= await SqliteUsageRepository.open());
}

/**
 * Legacy fixture retained temporarily for non-runtime documentation examples.
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

// Kept only as a migration reference while the embedded repository seed is
// reconciled with the generated browser inventory.
void infrastructureFixture;

/**
 * Control-plane fixtures. The playground has no backend to persist these,
 * so they are in-memory stores with demo seeds — mutations work for the
 * page's lifetime, like dashboards.
 */
let platformConfigRepository: SqlitePlatformConfigRepository | null = null;
async function getPlatformConfigRepository(): Promise<SqlitePlatformConfigRepository> {
  return (platformConfigRepository ??= await SqlitePlatformConfigRepository.open());
}

let tokenRepository: SqliteTokenRepository | null = null;
async function getTokenRepository(): Promise<SqliteTokenRepository> {
  return (tokenRepository ??= await SqliteTokenRepository.open());
}

let memberRepository: SqliteMemberRepository | null = null;
async function getMemberRepository(): Promise<SqliteMemberRepository> {
  return (memberRepository ??= await SqliteMemberRepository.open());
}

function usageReportFixture(params: { from?: number; to?: number }): TenantUsageReportResponse {
  const toMs = params.to ?? Date.now();
  const fromMs = params.from ?? toMs - 3_600_000;
  return {
    tenant_id: DEMO_TENANT_ID,
    from: new Date(fromMs).toISOString(),
    to: new Date(toMs).toISOString(),
    telemetry_summary: { spans: 240, logs: 40, metric_points: 720, metric_series_created: 12 },
    control_plane_summary: {
      query_reads: 36,
      query_rows: 1_842,
      credential_checks: 4,
      credential_allows: 4,
      credential_denies: 0,
    },
    estimated_cost_index: 512,
  };
}
void usageReportFixture;

let authRepository: SqliteAuthRepository | null = null;
async function getAuthRepository(): Promise<SqliteAuthRepository> {
  return (authRepository ??= await SqliteAuthRepository.open());
}

let incidentRepository: SqliteIncidentRepository | null = null;
async function getIncidentRepository(): Promise<SqliteIncidentRepository> {
  return (incidentRepository ??= await SqliteIncidentRepository.open());
}

function incidentDetailFixture(incident: IncidentItem): IncidentDetailResponse {
  return {
    ...incident,
    dedup_key: `dedup-${incident.incident_id}`,
    resolved_at: incident.resolved_at ?? null,
    triggered_by_rule_id: incident.triggered_by_rule_id ?? null,
    runbook_url: null,
    rule_name:
      incident.triggered_by_rule_id === "playground-rule-1" ? "payment error rate > 5%" : null,
    timeline: [
      {
        event_time: incident.triggered_at,
        event_type: "triggered",
        actor: "alert-evaluator",
        message: `Incident triggered: ${incident.title}`,
      },
    ],
    impacted_service:
      incident.triggered_by_rule_id === "playground-rule-1" ? "payment" : null,
  };
}
void incidentDetailFixture;

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
void deploymentsFixture;

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

function nlqServiceFilter(ir: NlqIr): string | undefined {
  const filter = ir.filters.find(({ field }) => field === "service" || field === "service_name");
  return filter?.value || undefined;
}

function nlqBucketCount(ir: NlqIr): number {
  return Math.max(1, Math.min(240, Math.floor(ir.limit ?? 24)));
}

interface PlaygroundMetricPoint {
  metric_series_id: string;
  time_unix_nano: number;
  value_double?: number;
  [key: string]: unknown;
}

/** Derives Prometheus-style counter values from the real DuckDB point stream. */
function deriveMetricPoints(
  points: PlaygroundMetricPoint[],
  operation: "rate" | "irate" | "increase",
): PlaygroundMetricPoint[] {
  const bySeries = new Map<string, PlaygroundMetricPoint[]>();
  for (const point of points) {
    const series = bySeries.get(point.metric_series_id) ?? [];
    series.push(point);
    bySeries.set(point.metric_series_id, series);
  }

  const derived: PlaygroundMetricPoint[] = [];
  for (const series of bySeries.values()) {
    const ordered = [...series].sort((a, b) => a.time_unix_nano - b.time_unix_nano);
    if (ordered.length < 2) continue;
    const pairs = ordered.slice(1).map((point, index) => {
      const previous = ordered[index];
      const elapsedSeconds = (point.time_unix_nano - previous.time_unix_nano) / 1_000_000_000;
      const currentValue = point.value_double ?? 0;
      const previousValue = previous.value_double ?? 0;
      const delta = currentValue >= previousValue ? currentValue - previousValue : currentValue;
      return { point, rate: elapsedSeconds > 0 ? delta / elapsedSeconds : 0, delta };
    });
    const selected = operation === "irate" ? pairs.slice(-1) : pairs;
    if (operation === "increase") {
      const last = ordered[ordered.length - 1];
      derived.push({ ...last, value_double: pairs.reduce((sum, pair) => sum + pair.delta, 0) });
    } else {
      derived.push(...selected.map(({ point, rate }) => ({ ...point, value_double: rate })));
    }
  }
  return derived;
}

function metricDistribution(points: PlaygroundMetricPoint[]): PlaygroundMetricPoint[] {
  const values = points.map((point) => point.value_double ?? 0).sort((a, b) => a - b);
  if (values.length === 0) return [];
  const percentile = (fraction: number) => values[Math.min(values.length - 1, Math.floor((values.length - 1) * fraction))];
  const last = points[points.length - 1];
  return [
    { ...last, stat: "min", value_double: values[0] },
    { ...last, stat: "average", value_double: values.reduce((sum, value) => sum + value, 0) / values.length },
    { ...last, stat: "p50", value_double: percentile(0.5) },
    { ...last, stat: "p95", value_double: percentile(0.95) },
    { ...last, stat: "max", value_double: values[values.length - 1] },
  ];
}

function metricHistogram(points: PlaygroundMetricPoint[], bucketCount: number): Record<string, unknown>[] {
  const values = points
    .map((point) => point.value_double)
    .filter((value): value is number => typeof value === "number" && Number.isFinite(value));
  if (values.length === 0) return [];
  const min = Math.min(...values);
  const max = Math.max(...values);
  const count = Math.max(1, Math.min(240, Math.floor(bucketCount)));
  const width = (max - min) / count || 1;
  const buckets = Array.from({ length: count }, (_, index) => ({
    start: min + index * width,
    end: min + (index + 1) * width,
    count: 0,
  }));
  for (const value of values) {
    const index = Math.min(count - 1, Math.max(0, Math.floor((value - min) / width)));
    buckets[index].count += 1;
  }
  return buckets;
}

function traceTopK(rows: NlqTraceRow[], limit: number): Record<string, unknown>[] {
  const groups = new Map<string, { count: number; totalDuration: number; latest: NlqTraceRow }>();
  for (const row of rows) {
    const group = groups.get(row.root_service) ?? { count: 0, totalDuration: 0, latest: row };
    group.count += 1;
    group.totalDuration += row.duration_ms;
    if (String(row.start_time_unix_nano) > String(group.latest.start_time_unix_nano)) group.latest = row;
    groups.set(row.root_service, group);
  }
  return [...groups.entries()]
    .map(([service_name, group]) => ({
      service_name,
      request_count: group.count,
      average_duration_ms: group.totalDuration / group.count,
      latest_start_time_unix_nano: group.latest.start_time_unix_nano,
    }))
    .sort((a, b) => Number(b.request_count) - Number(a.request_count))
    .slice(0, Math.max(1, Math.min(100, limit)));
}

function traceDistribution(rows: NlqTraceRow[]): Record<string, unknown>[] {
  const values = rows.map((row) => row.duration_ms).sort((a, b) => a - b);
  if (values.length === 0) return [];
  const percentile = (fraction: number) => values[Math.min(values.length - 1, Math.floor((values.length - 1) * fraction))];
  return [
    { stat: "min", value_double: values[0], unit: "ms" },
    { stat: "average", value_double: values.reduce((sum, value) => sum + value, 0) / values.length, unit: "ms" },
    { stat: "p50", value_double: percentile(0.5), unit: "ms" },
    { stat: "p95", value_double: percentile(0.95), unit: "ms" },
    { stat: "max", value_double: values[values.length - 1], unit: "ms" },
  ];
}

function logTopK(rows: NlqLogRow[], limit: number): Record<string, unknown>[] {
  const groups = new Map<string, { count: number; errorCount: number; latest: NlqLogRow }>();
  for (const row of rows) {
    const group = groups.get(row.service_name) ?? { count: 0, errorCount: 0, latest: row };
    group.count += 1;
    if (row.severity_number >= 17) group.errorCount += 1;
    if (String(row.timestamp_unix_nano) > String(group.latest.timestamp_unix_nano)) group.latest = row;
    groups.set(row.service_name, group);
  }
  return [...groups.entries()]
    .map(([service_name, group]) => ({
      service_name,
      log_count: group.count,
      error_count: group.errorCount,
      latest_timestamp_unix_nano: group.latest.timestamp_unix_nano,
    }))
    .sort((a, b) => Number(b.log_count) - Number(a.log_count))
    .slice(0, Math.max(1, Math.min(100, limit)));
}

function logDistribution(rows: NlqLogRow[]): Record<string, unknown>[] {
  const counts = new Map<string, number>();
  for (const row of rows) counts.set(row.severity_text, (counts.get(row.severity_text) ?? 0) + 1);
  return [...counts.entries()]
    .map(([severity, count]) => ({ severity, value_double: count, unit: "logs" }))
    .sort((a, b) => Number(b.value_double) - Number(a.value_double));
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
    async search(_tenantId: string, params: { service?: string; limit?: number; from?: string; to?: string }): Promise<TraceListResponse> {
      const { executeTraceTable } = await import("../playground/engineClient");
      const filters = params.service ? [{ field: "service_name", op: "=", value: params.service }] : [];
      const { rows } = await executeTraceTable({
        operation: "table",
        signals: ["traces"],
        filters,
        time_range: { from: params.from ?? "now-1h", to: params.to ?? "now" },
      });
      const traces = rows.slice(0, params.limit ?? 50).map((row) => ({
        trace_id: row.trace_id,
        spans: [{
          span_id: `${row.trace_id}-root`, trace_id: row.trace_id, tenant_id: _tenantId,
          service_name: row.root_service, service_namespace: "", service_version: "",
          operation_name: row.root_operation, span_kind: "SERVER", start_time_unix_nano: Number(row.start_time_unix_nano),
          end_time_unix_nano: Number(row.start_time_unix_nano) + row.duration_ms * 1_000_000,
          duration_ns: row.duration_ms * 1_000_000, status_code: row.status_code === "ERROR" ? "ERROR" : "OK",
          status_message: "", attributes: {}, resource_attributes: {}, environment: row.environment ?? "",
          host_id: "", workload: "", deployment_id: "",
        } as Span],
        events: [],
      }));
      return { traces, total: rows.length, facets: {} };
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
    async list(tenantId: string): Promise<AlertRuleListResponse> {
      return (await alertRuleRepository()).list(tenantId);
    },
    async get(tenantId: string, ruleId: string): Promise<AlertRuleDetailResponse> {
      const repository = await alertRuleRepository();
      const rule = repository.get(tenantId, ruleId);
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
        runbook_url: repository.getRunbook(tenantId, ruleId),
      };
    },
    async create(tenantId: string, req: CreateRuleRequest): Promise<CreateRuleResponse> {
      return { rule_id: (await alertRuleRepository()).create(tenantId, req) };
    },
    async silence(tenantId: string, ruleId: string, silenced: boolean): Promise<void> {
      (await alertRuleRepository()).setSilenced(tenantId, ruleId, silenced);
    },
    async setRunbook(tenantId: string, ruleId: string, runbookUrl: string | null): Promise<void> {
      (await alertRuleRepository()).setRunbook(tenantId, ruleId, runbookUrl);
    },
  },
  incidents: {
    async list(tenantId: string, status?: string): Promise<IncidentListResponse> {
      return (await getIncidentRepository()).list(tenantId, status);
    },
    async get(tenantId: string, incidentId: string): Promise<IncidentDetailResponse> {
      const incident = (await getIncidentRepository()).get(tenantId, incidentId);
      if (!incident) {
        throw new Error(`Failed to get incident: 404`);
      }
      return (await getIncidentRepository()).getDetail(tenantId, incidentId) ??
        (() => { throw new Error(`Failed to get incident: 404`); })();
    },
  },
  slos: {
    async list(tenantId: string): Promise<SloListResponse> {
      return (await sloRepository()).list(tenantId);
    },
    async create(tenantId: string, req: CreateSloRequest): Promise<SloDefinitionItem> {
      return (await sloRepository()).create(tenantId, req);
    },
  },
  notificationChannels: {
    async list(tenantId: string): Promise<NotificationChannelItem[]> {
      return (await notificationChannelRepository()).list(tenantId);
    },
    async create(tenantId: string, req: CreateChannelRequest): Promise<NotificationChannelItem> {
      return (await notificationChannelRepository()).create(tenantId, req);
    },
    async delete(tenantId: string, channelId: string): Promise<void> {
      (await notificationChannelRepository()).delete(tenantId, channelId);
    },
  },
  savedViews: {
    async list(tenantId: string, signalKind: SignalKind): Promise<SavedViewListResponse> {
      return (await savedViewRepository()).list(tenantId, signalKind);
    },
    async create(tenantId: string, req: CreateSavedViewRequest): Promise<SavedView> {
      return (await savedViewRepository()).create(tenantId, req);
    },
    async update(
      tenantId: string,
      savedViewId: string,
      req: UpdateSavedViewRequest
    ): Promise<SavedView> {
      return (await savedViewRepository()).update(tenantId, savedViewId, req);
    },
    async delete(tenantId: string, savedViewId: string): Promise<void> {
      (await savedViewRepository()).delete(tenantId, savedViewId);
    },
    async listGrants(tenantId: string, savedViewId: string): Promise<GrantListResponse> {
      return (await savedViewRepository()).listGrants(tenantId, savedViewId);
    },
    async addGrant(
      tenantId: string,
      savedViewId: string,
      userId: string,
      relation: GrantItem["relation"]
    ): Promise<void> {
      (await savedViewRepository()).addGrant(tenantId, savedViewId, userId, relation);
    },
    async revokeGrant(tenantId: string, savedViewId: string, userId: string): Promise<void> {
      (await savedViewRepository()).revokeGrant(tenantId, savedViewId, userId);
    },
  },
  infrastructure: {
    async list(
      tenantId: string,
      params: { service?: string; environment?: string; entity_type?: string }
    ): Promise<InfrastructureInventoryResponse> {
      let items = (await getInfrastructureRepository()).list(
        tenantId,
        params.entity_type as InfrastructureEntityType | undefined
      );
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
      tenantId: string,
      entityType: InfrastructureEntityType,
      entityId: string
    ): Promise<InfrastructureDetailResponse> {
      const entity = (await getInfrastructureRepository()).get(tenantId, entityType, entityId);
      const detail = entity
        ? {
            entity,
            links: {
              logs: `/logs?service=${encodeURIComponent(entity.related_services[0] ?? "")}`,
              traces: `/traces?service=${encodeURIComponent(entity.related_services[0] ?? "")}`,
              metrics: `/metrics?service=${encodeURIComponent(entity.related_services[0] ?? "")}`,
            },
          }
        : null;
      if (!detail) {
        throw new Error(`Query failed: 404`);
      }
      return detail;
    },
  },
  setup: {
    async getFirstSignalStatus(_tenantId: string): Promise<FirstSignalStatus> {
      // The demo dataset is generated at engine init, so the first signal
      // has always "arrived".
      return { state: "detected", traces: 40, logs: 40, metrics: 720 };
    },
    async getConfig(tenantId: string): Promise<PlatformConfig> {
      return (await getPlatformConfigRepository()).get(tenantId);
    },
    async saveLlmConfig(tenantId: string, params: SaveLlmConfigParams): Promise<void> {
      (await getPlatformConfigRepository()).update(tenantId, params);
    },
    async fetchAvailableModels(
      tenantId: string,
      url?: string
    ): Promise<LlmModelsResult> {
      const effectiveUrl = url ?? (await getPlatformConfigRepository()).get(tenantId).llm_url;
      if (!effectiveUrl) {
        return { ok: false, models: [], error: "No LLM endpoint configured in the playground." };
      }
      return { ok: true, models: ["playground/demo-model"] };
    },
  },
  tokens: {
    async list(tenantId: string): Promise<TokenListResponse> {
      return (await getTokenRepository()).list(tenantId);
    },
    async create(tenantId: string, req: CreateTokenRequest): Promise<CreateTokenResponse> {
      return (await getTokenRepository()).create(tenantId, DEMO_TENANT_NAME, req);
    },
    async revoke(tenantId: string, id: string): Promise<void> {
      (await getTokenRepository()).setRevoked(tenantId, id, true);
    },
    async renew(tenantId: string, id: string): Promise<CreateTokenResponse> {
      const repository = await getTokenRepository();
      const existing = repository.find(tenantId, id);
      if (!existing) throw new Error(`renewToken failed: 404`);
      const created = await this.create(tenantId, {
        name: `${existing.name} (renewed)`,
        environment: existing.environment,
      });
      await this.revoke(tenantId, id);
      return created;
    },
    async restore(tenantId: string, id: string): Promise<void> {
      (await getTokenRepository()).setRevoked(tenantId, id, false);
    },
    async delete(tenantId: string, id: string): Promise<void> {
      (await getTokenRepository()).delete(tenantId, id);
    },
  },
  members: {
    async list(tenantId: string): Promise<MemberListResponse> {
      return { members: (await getMemberRepository()).list(tenantId) };
    },
    async add(
      tenantId: string,
      body: { email: string; role: TenantRole }
    ): Promise<MemberRecord> {
      return (await getMemberRepository()).add(tenantId, body.email, body.role);
    },
    async updateRole(tenantId: string, userId: string, role: TenantRole): Promise<void> {
      (await getMemberRepository()).updateRole(tenantId, userId, role);
    },
    async remove(tenantId: string, userId: string): Promise<void> {
      (await getMemberRepository()).remove(tenantId, userId);
    },
    async revokeSessions(_tenantId: string, _userId: string): Promise<void> {
      // No sessions to revoke in the playground.
    },
  },
  usage: {
    async report(
      _tenantId: string,
      params: { from?: number; to?: number }
    ): Promise<TenantUsageReportResponse> {
      return (await getUsageRepository()).report(_tenantId, params);
    },
  },
  auth: {
    async me(): Promise<MeResponse> {
      return (await getAuthRepository()).me();
    },
    login(): void {
      // The playground has no login backend; useAuth already provides the
      // synthetic identity.
    },
    async logout(): Promise<void> {},
  },
  deployments: {
    async list(tenantId: string, params: ListDeploymentsParams): Promise<ListDeploymentsResponse> {
      return { items: (await getDeploymentRepository()).list(tenantId, params) };
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

      if (!hasFreeTextQuestion && ir && ir.operation === "catalog" && ir.signals?.length === 1 && ir.signals[0] === "metrics") {
        const { executeMetricCatalog } = await import("../playground/engineClient");
        const { metrics } = await executeMetricCatalog(nlqServiceFilter(ir));
        return {
          type: "frame",
          frame: {
            ...STUB_NLQ_FRAME,
            data: metrics as unknown as Record<string, unknown>[],
            nlq_ir: ir,
            signal_types: ir.signals,
            time_range: ir.time_range,
            source_sql: "-- playground DuckDB metric catalog query",
          },
        };
      }

      if (
        !hasFreeTextQuestion &&
        ir &&
        (ir.operation === "timeseries" || ir.operation === "rate" || ir.operation === "irate" || ir.operation === "increase") &&
        ir.signals?.length === 1 &&
        ir.signals[0] === "metrics"
      ) {
        const { executeMetricCatalog, executeMetricGroupPoints } = await import("../playground/engineClient");
        const service = nlqServiceFilter(ir);
        const { metrics } = await executeMetricCatalog(service);
        const metric = metrics.find((candidate) => candidate.metric_name === ir.metric) ?? metrics[0];
        const { points } = metric ? await executeMetricGroupPoints(metric) : { points: [] };
        const data = ir.operation === "timeseries"
          ? points
          : deriveMetricPoints(points as unknown as PlaygroundMetricPoint[], ir.operation);
        return {
          type: "frame",
          frame: {
            ...STUB_NLQ_FRAME,
            frame_type: "timeseries",
            suggested_visualization: "timeseries",
            x_field: "time_unix_nano",
            y_field: "value_double",
            data: data as unknown as Record<string, unknown>[],
            nlq_ir: ir,
            signal_types: ir.signals,
            time_range: ir.time_range,
            unit: metric?.unit ?? null,
            source_sql: "-- playground DuckDB metric points query",
          },
        };
      }

      if (
        !hasFreeTextQuestion &&
        ir &&
        (ir.operation === "topk" || ir.operation === "distribution") &&
        ir.signals?.length === 1 &&
        ir.signals[0] === "metrics"
      ) {
        const { executeMetricCatalog, executeMetricGroupPoints } = await import("../playground/engineClient");
        const { metrics } = await executeMetricCatalog(nlqServiceFilter(ir));
        const selectedMetrics = ir.operation === "distribution"
          ? metrics.filter((candidate) => !ir.metric || candidate.metric_name === ir.metric).slice(0, 1)
          : metrics;
        const groups = await Promise.all(selectedMetrics.map(async (candidate) => ({
          metric: candidate,
          points: (await executeMetricGroupPoints(candidate)).points as unknown as PlaygroundMetricPoint[],
        })));
        const data = ir.operation === "distribution"
          ? metricDistribution(groups[0]?.points ?? [])
          : groups
            .flatMap(({ metric, points }) => {
              const point = points.at(-1);
              return point ? [{ ...point, metric_name: metric.metric_name, value_double: point.value_double ?? 0 }] : [];
            })
            .sort((a, b) => (b.value_double ?? 0) - (a.value_double ?? 0))
            .slice(0, Math.max(1, Math.min(100, ir.limit ?? 10)));
        return {
          type: "frame",
          frame: {
            ...STUB_NLQ_FRAME,
            frame_type: ir.operation,
            suggested_visualization: ir.operation,
            x_field: ir.operation === "topk" ? "metric_name" : "stat",
            y_field: "value_double",
            data: data as unknown as Record<string, unknown>[],
            nlq_ir: ir,
            signal_types: ir.signals,
            time_range: ir.time_range,
            unit: selectedMetrics[0]?.unit ?? null,
            source_sql: "-- playground DuckDB metric points query",
          },
        };
      }

      if (
        !hasFreeTextQuestion &&
        ir &&
        ir.operation === "table" &&
        ir.signals?.length === 1 &&
        ir.signals[0] === "metrics"
      ) {
        const { executeMetricCatalog } = await import("../playground/engineClient");
        const { metrics } = await executeMetricCatalog(nlqServiceFilter(ir));
        return {
          type: "frame",
          frame: {
            ...STUB_NLQ_FRAME,
            frame_type: "table",
            suggested_visualization: "table",
            data: metrics as unknown as Record<string, unknown>[] ,
            nlq_ir: ir,
            signal_types: ir.signals,
            time_range: ir.time_range,
            source_sql: "-- playground DuckDB metric catalog query",
          },
        };
      }

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
        (ir.operation === "topk" || ir.operation === "distribution") &&
        ir.signals?.length === 1 &&
        ir.signals[0] === "traces"
      ) {
        const { executeTraceTable } = await import("../playground/engineClient");
        const { rows, sql } = await executeTraceTable({ ...ir, operation: "table" });
        const data = ir.operation === "topk"
          ? traceTopK(rows, ir.limit ?? 10)
          : traceDistribution(rows);
        return {
          type: "frame",
          frame: {
            ...STUB_NLQ_FRAME,
            frame_type: ir.operation,
            suggested_visualization: ir.operation,
            x_field: ir.operation === "topk" ? "service_name" : "stat",
            y_field: ir.operation === "topk" ? "request_count" : "value_double",
            data,
            nlq_ir: ir,
            signal_types: ir.signals,
            time_range: ir.time_range,
            unit: ir.operation === "distribution" ? "ms" : null,
            source_sql: sql,
          },
        };
      }

      if (
        !hasFreeTextQuestion &&
        ir &&
        ir.operation === "histogram" &&
        ir.signals?.length === 1 &&
        ir.signals[0] === "metrics"
      ) {
        const { executeMetricCatalog, executeMetricGroupPoints } = await import("../playground/engineClient");
        const { metrics } = await executeMetricCatalog(nlqServiceFilter(ir));
        const metric = metrics.find((candidate) => candidate.metric_name === ir.metric) ?? metrics[0];
        const { points } = metric ? await executeMetricGroupPoints(metric) : { points: [] };
        return {
          type: "frame",
          frame: {
            ...STUB_NLQ_FRAME,
            frame_type: "histogram",
            suggested_visualization: "histogram",
            x_field: "start",
            y_field: "count",
            data: metricHistogram(points as unknown as PlaygroundMetricPoint[], nlqBucketCount(ir)),
            nlq_ir: ir,
            signal_types: ir.signals,
            time_range: ir.time_range,
            unit: metric?.unit ?? null,
            source_sql: "-- playground DuckDB metric points query",
          },
        };
      }

      if (
        !hasFreeTextQuestion &&
        ir &&
        ir.operation === "histogram" &&
        ir.signals?.length === 1 &&
        (ir.signals[0] === "traces" || ir.signals[0] === "logs")
      ) {
        const fromNs = ir.time_range.from;
        const toNs = ir.time_range.to;
        const bucketCount = nlqBucketCount(ir);
        const service = nlqServiceFilter(ir);
        const engine = await import("../playground/engineClient");
        const { buckets } = ir.signals[0] === "traces"
          ? await engine.executeTraceHistogram({ fromNs, toNs, bucketCount, service })
          : await engine.executeLogHistogram({ fromNs, toNs, bucketCount, service });
        return {
          type: "frame",
          frame: {
            ...STUB_NLQ_FRAME,
            frame_type: "histogram",
            suggested_visualization: "histogram",
            data: buckets as unknown as Record<string, unknown>[],
            nlq_ir: ir,
            signal_types: ir.signals,
            time_range: ir.time_range,
            source_sql: "-- playground DuckDB histogram query",
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

      if (
        !hasFreeTextQuestion &&
        ir &&
        (ir.operation === "topk" || ir.operation === "distribution") &&
        ir.signals?.length === 1 &&
        ir.signals[0] === "logs"
      ) {
        const { executeLogTable } = await import("../playground/engineClient");
        const { rows, sql } = await executeLogTable({ ...ir, operation: "table" });
        const data = ir.operation === "topk"
          ? logTopK(rows, ir.limit ?? 10)
          : logDistribution(rows);
        return {
          type: "frame",
          frame: {
            ...STUB_LOG_FRAME,
            frame_type: ir.operation,
            suggested_visualization: ir.operation,
            x_field: ir.operation === "topk" ? "service_name" : "severity",
            y_field: ir.operation === "topk" ? "log_count" : "value_double",
            data,
            nlq_ir: ir,
            signal_types: ir.signals,
            time_range: ir.time_range,
            unit: ir.operation === "distribution" ? "logs" : null,
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
            data: (await getInfrastructureRepository()).list(_tenantId) as unknown as Record<string, unknown>[],
            nlq_ir: ir,
            source_sql: "-- playground SQLite infrastructure inventory",
          },
        };
      }
      return { type: "frame", frame: STUB_NLQ_FRAME };
    },
  },
  dashboards: {
    async list(tenantId: string): Promise<DashboardListResponse> {
      return (await dashboardRepository()).list(tenantId);
    },
    async get(tenantId: string, dashboardId: string): Promise<Dashboard> {
      return (await dashboardRepository()).get(tenantId, dashboardId);
    },
    async create(tenantId: string, request: CreateDashboardRequest): Promise<Dashboard> {
      return (await dashboardRepository()).create(tenantId, request);
    },
    async update(tenantId: string, dashboardId: string, request: UpdateDashboardRequest): Promise<Dashboard> {
      return (await dashboardRepository()).update(tenantId, dashboardId, request);
    },
    async delete(tenantId: string, dashboardId: string): Promise<void> {
      (await dashboardRepository()).delete(tenantId, dashboardId);
    },
    async export(tenantId: string, dashboardId: string): Promise<DashboardExport> {
      const dashboard = (await dashboardRepository()).get(tenantId, dashboardId);
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
    async import(tenantId: string, export_: DashboardExport): Promise<Dashboard> {
      return (await dashboardRepository()).create(tenantId, {
        name: export_.name,
        panels: export_.panels.map((panel) => ({
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
          })),
      });
    },
  },
};
