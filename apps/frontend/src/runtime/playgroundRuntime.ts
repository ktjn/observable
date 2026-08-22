import type { Span, TraceHistogramResponse, TraceListResponse, TraceResponse } from "../api/traces";
import type { LogRecord, LogHistogramResponse } from "../api/logs";
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
  RuntimeApi,
  TraceHistogramParams,
  LogHistogramParams,
  ServiceSummaryParams,
  TopologyParams,
} from "./types";
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
