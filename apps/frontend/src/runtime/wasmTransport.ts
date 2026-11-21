import type { HttpMethod, RequestTransport, RouteRequest } from "./requestTransport";
import type { LogRecord } from "../api/logs";
import type { Span } from "../api/traces";
import type { CreateRuleRequest } from "../api/alerts";
import type { CreateSloRequest } from "../api/slos";
import type { CreateChannelRequest } from "../api/notifications";
import type {
  CreateSavedViewRequest,
  GrantItem,
  SignalKind,
  UpdateSavedViewRequest,
} from "../api/savedViews";
import type { NlqLogRow } from "../playground/engineClient";

/**
 * The in-process local-engine transport (ADR-047 §1). Each capability's
 * routes are registered here as they migrate off their own hand-written
 * playgroundRuntime.ts implementation; `wasmTransport` is additive and does
 * not yet cover every route `httpTransport` does.
 */

type WasmHandler = (params: Record<string, string>, route: RouteRequest) => Promise<unknown>;

interface RegisteredRoute {
  method: HttpMethod;
  segments: string[];
  handler: WasmHandler;
}

const routes: RegisteredRoute[] = [];

/**
 * `pattern` segments prefixed with `:` bind that segment into `params`.
 * Matching tries routes in registration order and returns the first hit, so
 * a literal route sharing a segment count with a `:param` route (e.g.
 * `/v1/traces/histogram` vs `/v1/traces/:traceId`) must be registered
 * before the param route, or the param route swallows it.
 */
export function registerWasmRoute(method: HttpMethod, pattern: string, handler: WasmHandler): void {
  routes.push({ method, segments: pattern.split("/").filter(Boolean), handler });
}

function matchPath(routeSegments: string[], pathSegments: string[]): Record<string, string> | null {
  if (routeSegments.length !== pathSegments.length) return null;
  const params: Record<string, string> = {};
  for (let i = 0; i < routeSegments.length; i++) {
    const routeSegment = routeSegments[i];
    const pathSegment = pathSegments[i];
    if (routeSegment.startsWith(":")) {
      params[routeSegment.slice(1)] = decodeURIComponent(pathSegment);
    } else if (routeSegment !== pathSegment) {
      return null;
    }
  }
  return params;
}

export const wasmTransport: RequestTransport = {
  async dispatch<T>(route: RouteRequest): Promise<T> {
    const pathSegments = route.path.split("/").filter(Boolean);
    for (const candidate of routes) {
      if (candidate.method !== route.method) continue;
      const params = matchPath(candidate.segments, pathSegments);
      if (params === null) continue;
      return candidate.handler(params, route) as Promise<T>;
    }
    throw new Error(`no wasm handler registered for ${route.method} ${route.path}`);
  },
};

const DEMO_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const DEMO_TENANT_NAME = "observable";

registerWasmRoute("GET", "/v1/tenants", async () => ({
  tenants: [{ id: DEMO_TENANT_ID, name: DEMO_TENANT_NAME }],
}));

registerWasmRoute("GET", "/v1/tenants/:id/environments", async () => ({
  environments: [{ environment: "production" }, { environment: "staging" }],
}));

function toNanos(ms: number): string {
  return String(BigInt(Math.floor(ms)) * 1_000_000n);
}

function isoToMs(iso: unknown, fallback: number): number {
  return typeof iso === "string" ? Date.parse(iso) : fallback;
}

registerWasmRoute("GET", "/v1/topology", async (_params, route) => {
  const { executeTopology } = await import("../playground/engineClient");
  const query = route.query ?? {};
  const nowMs = Date.now();
  const fromMs = isoToMs(query.from, nowMs - 3_600_000);
  const toMs = isoToMs(query.to, nowMs);
  const { edges } = await executeTopology({
    fromNs: toNanos(fromMs),
    toNs: toNanos(toMs),
    environment: query.environment as string | undefined,
    service: query.service as string | undefined,
  });
  return { edges };
});

registerWasmRoute("GET", "/v1/events/changes", async (_params, route) => {
  const { executeChangeEvents } = await import("../playground/engineClient");
  const query = route.query ?? {};
  const nowMs = Date.now();
  const fromMs = isoToMs(query.start_time, nowMs - 3_600_000);
  const toMs = isoToMs(query.end_time, nowMs);
  const { changeEvents } = await executeChangeEvents({
    fromNs: toNanos(fromMs),
    toNs: toNanos(toMs),
    service: query.service_name as string | undefined,
    environment: query.environment as string | undefined,
    eventType: query.event_type as string | undefined,
    limit: typeof query.limit === "number" ? query.limit : 50,
  });
  return { items: changeEvents };
});

registerWasmRoute("GET", "/v1/metrics", async (_params, route) => {
  const { executeMetricCatalog } = await import("../playground/engineClient");
  const { metrics } = await executeMetricCatalog(route.query?.service as string | undefined);
  return { metrics };
});

registerWasmRoute("GET", "/v1/metrics/points", async (_params, route) => {
  const { executeMetricGroupPoints } = await import("../playground/engineClient");
  const query = route.query ?? {};
  // Only metric_name/service_name/environment/metric_type/unit are read by
  // the engine's group-points query — the rest of MetricCatalogEntry's
  // shape is reconstructed with placeholders to satisfy the type.
  const { points } = await executeMetricGroupPoints({
    tenant_id: "",
    metric_name: String(query.metric_name ?? ""),
    description: "",
    unit: String(query.unit ?? ""),
    metric_type: String(query.metric_type ?? ""),
    service_name: String(query.service ?? ""),
    environment: String(query.environment ?? ""),
    series_count: 0,
  });
  return { points };
});

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

registerWasmRoute("GET", "/v1/logs/histogram", async (_params, route) => {
  const { executeLogHistogram } = await import("../playground/engineClient");
  const query = route.query ?? {};
  const { buckets } = await executeLogHistogram({
    fromNs: String(query.from ?? ""),
    toNs: String(query.to ?? ""),
    bucketCount: typeof query.buckets === "number" ? query.buckets : 30,
    service: query.service as string | undefined,
  });
  return { buckets };
});

registerWasmRoute("GET", "/v1/logs", async (_params, route) => {
  const { executeLogsSearch } = await import("../playground/engineClient");
  const query = route.query ?? {};
  const { logs } = await executeLogsSearch({
    traceId: query.trace_id as string | undefined,
    service: query.service as string | undefined,
    limit: typeof query.limit === "number" ? query.limit : 100,
  });
  return { logs: logs.map(nlqRowToLogRecord), total: logs.length, facets: {} };
});

registerWasmRoute("GET", "/v1/logs/:logId/context", async (params, route) => {
  const { executeLogsContext } = await import("../playground/engineClient");
  const query = route.query ?? {};
  const { logs } = await executeLogsContext({
    logId: params.logId,
    window: typeof query.window === "number" ? query.window : undefined,
  });
  return { logs: logs.map(nlqRowToLogRecord), total: logs.length, facets: {} };
});

registerWasmRoute("GET", "/v1/logs/tail", async (_params, route) => {
  const { executeLogsTail } = await import("../playground/engineClient");
  const query = route.query ?? {};
  const { logs } = await executeLogsTail({
    service: query.service as string | undefined,
    severity: typeof query.severity === "number" ? query.severity : undefined,
    sinceUnixNano: query.since_unix_nano as string | undefined,
    limit: typeof query.limit === "number" ? query.limit : 100,
  });
  return { logs: logs.map(nlqRowToLogRecord), total: logs.length, facets: {} };
});

// Registered before /v1/traces/:traceId — see registerWasmRoute's doc comment.
registerWasmRoute("GET", "/v1/traces/histogram", async (_params, route) => {
  const query = route.query ?? {};
  if (!query.from || !query.to) {
    return { buckets: [{ start_ms: 0, end_ms: 60_000, count: 1 }] };
  }
  const { executeTraceHistogram } = await import("../playground/engineClient");
  const { buckets } = await executeTraceHistogram({
    fromNs: String(query.from),
    toNs: String(query.to),
    bucketCount: typeof query.buckets === "number" ? query.buckets : 30,
    service: query.service as string | undefined,
  });
  return { buckets };
});

registerWasmRoute("GET", "/v1/traces", async (_params, route) => {
  const { executeTraceTable } = await import("../playground/engineClient");
  const query = route.query ?? {};
  const service = query.service as string | undefined;
  const filters = service ? [{ field: "service_name", op: "=", value: service }] : [];
  const { rows } = await executeTraceTable({
    operation: "table",
    signals: ["traces"],
    filters,
    time_range: {
      from: (query.from as string | undefined) ?? "now-1h",
      to: (query.to as string | undefined) ?? "now",
    },
  });
  const limit = typeof query.limit === "number" ? query.limit : 50;
  const traces = rows.slice(0, limit).map((row) => ({
    trace_id: row.trace_id,
    spans: [
      {
        span_id: `${row.trace_id}-root`,
        trace_id: row.trace_id,
        tenant_id: DEMO_TENANT_ID,
        service_name: row.root_service,
        service_namespace: "",
        service_version: "",
        operation_name: row.root_operation,
        span_kind: "SERVER",
        start_time_unix_nano: Number(row.start_time_unix_nano),
        end_time_unix_nano: Number(row.start_time_unix_nano) + row.duration_ms * 1_000_000,
        duration_ns: row.duration_ms * 1_000_000,
        status_code: row.status_code === "ERROR" ? "ERROR" : "OK",
        status_message: "",
        attributes: {},
        resource_attributes: {},
        environment: row.environment ?? "",
        host_id: "",
        workload: "",
        deployment_id: "",
      } as Span,
    ],
    events: [],
  }));
  return { traces, total: rows.length, facets: {} };
});

// Must come after /v1/traces/histogram — this `:traceId` param would
// otherwise swallow it.
registerWasmRoute("GET", "/v1/traces/:traceId", async (params) => {
  const { executeTraceDetail } = await import("../playground/engineClient");
  const { spans } = await executeTraceDetail(params.traceId);
  if (spans.length === 0) {
    // Mirror production's 404 so the page renders "Trace not found."
    throw new Error(`Not found: ${params.traceId}`);
  }
  return { trace_id: params.traceId, spans, events: [] };
});

function serviceSummaryWindow(route: RouteRequest): { fromMs: number; toMs: number } {
  const query = route.query ?? {};
  const nowMs = Date.now();
  return {
    fromMs: isoToMs(query.from, nowMs - 3_600_000),
    toMs: isoToMs(query.to, nowMs),
  };
}

registerWasmRoute("GET", "/v1/services/summary", async (_params, route) => {
  const { executeServiceSummaries } = await import("../playground/engineClient");
  const { fromMs, toMs } = serviceSummaryWindow(route);
  const { items } = await executeServiceSummaries({
    fromNs: toNanos(fromMs),
    toNs: toNanos(toMs),
    environment: route.query?.environment as string | undefined,
  });
  return { items };
});

registerWasmRoute("GET", "/v1/services/:serviceName/summary", async (params, route) => {
  const { executeServiceSummaries } = await import("../playground/engineClient");
  const { fromMs, toMs } = serviceSummaryWindow(route);
  const { items } = await executeServiceSummaries({
    fromNs: toNanos(fromMs),
    toNs: toNanos(toMs),
    environment: route.query?.environment as string | undefined,
  });
  const service = items.find((item) => item.service_name === params.serviceName);
  if (!service) {
    throw new Error(`Service not found: ${params.serviceName}`);
  }
  return { service };
});

registerWasmRoute("GET", "/v1/services", async () => {
  const { executeServiceNames } = await import("../playground/engineClient");
  const { names } = await executeServiceNames();
  return { items: names };
});

registerWasmRoute("GET", "/v1/services/:serviceName/response-time-history", async (params, route) => {
  const { executeResponseTimeHistogram } = await import("../playground/engineClient");
  const query = route.query ?? {};
  const nowMs = Date.now();
  const fromMs = isoToMs(query.from, nowMs - 3_600_000);
  const toMs = isoToMs(query.to, nowMs);
  const { buckets } = await executeResponseTimeHistogram({
    fromNs: toNanos(fromMs),
    toNs: toNanos(toMs),
    bucketCount: typeof query.buckets === "number" ? query.buckets : 60,
    serviceName: params.serviceName,
  });
  return { buckets };
});

// SQLite-backed control-plane repositories (alerts, incidents, etc.) have no
// query engine to plan against — they're a different backing than the
// DuckDB-engine routes above, but the same dispatch()/route-table shape.
let alertRuleRepositoryPromise:
  | Promise<import("./sqliteAlertRuleRepository").SqliteAlertRuleRepository>
  | undefined;

async function alertRuleRepository() {
  alertRuleRepositoryPromise ??= (async () => {
    const { SqliteAlertRuleRepository } = await import("./sqliteAlertRuleRepository");
    return SqliteAlertRuleRepository.open();
  })();
  return alertRuleRepositoryPromise;
}

registerWasmRoute("GET", "/v1/alerts/rules", async () => {
  const repository = await alertRuleRepository();
  return repository.list(DEMO_TENANT_ID);
});

registerWasmRoute("GET", "/v1/alerts/rules/:ruleId", async (params) => {
  const repository = await alertRuleRepository();
  const rule = repository.get(DEMO_TENANT_ID, params.ruleId);
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
    runbook_url: repository.getRunbook(DEMO_TENANT_ID, params.ruleId),
  };
});

registerWasmRoute("POST", "/v1/admin/alerts/rules", async (_params, route) => {
  const repository = await alertRuleRepository();
  const ruleId = repository.create(DEMO_TENANT_ID, route.body as CreateRuleRequest);
  return { rule_id: ruleId };
});

registerWasmRoute("PATCH", "/v1/admin/alerts/rules/:ruleId/silence", async (params, route) => {
  const repository = await alertRuleRepository();
  const { silenced } = route.body as { silenced: boolean };
  repository.setSilenced(DEMO_TENANT_ID, params.ruleId, silenced);
});

registerWasmRoute("PATCH", "/v1/admin/alerts/rules/:ruleId/runbook", async (params, route) => {
  const repository = await alertRuleRepository();
  const { runbook_url } = route.body as { runbook_url: string | null };
  repository.setRunbook(DEMO_TENANT_ID, params.ruleId, runbook_url);
});

let incidentRepositoryPromise:
  | Promise<import("./sqliteIncidentRepository").SqliteIncidentRepository>
  | undefined;

async function incidentRepository() {
  incidentRepositoryPromise ??= (async () => {
    const { SqliteIncidentRepository } = await import("./sqliteIncidentRepository");
    return SqliteIncidentRepository.open();
  })();
  return incidentRepositoryPromise;
}

type IncidentActionPayload = { actor?: string; message?: string };

registerWasmRoute("GET", "/v1/incidents", async (_params, route) => {
  const repository = await incidentRepository();
  return repository.list(DEMO_TENANT_ID, route.query?.status as string | undefined);
});

registerWasmRoute("GET", "/v1/incidents/:incidentId", async (params) => {
  const repository = await incidentRepository();
  const incident = repository.get(DEMO_TENANT_ID, params.incidentId);
  if (!incident) {
    throw new Error("Failed to get incident: 404");
  }
  const detail = repository.getDetail(DEMO_TENANT_ID, params.incidentId);
  if (!detail) {
    throw new Error("Failed to get incident: 404");
  }
  return detail;
});

registerWasmRoute("POST", "/v1/incidents/:incidentId/acknowledge", async (params, route) => {
  const repository = await incidentRepository();
  return repository.acknowledge(DEMO_TENANT_ID, params.incidentId, route.body as IncidentActionPayload);
});

registerWasmRoute("POST", "/v1/incidents/:incidentId/status", async (params, route) => {
  const repository = await incidentRepository();
  const { status, ...payload } = route.body as IncidentActionPayload & { status: string };
  return repository.updateStatus(DEMO_TENANT_ID, params.incidentId, status, payload);
});

registerWasmRoute("POST", "/v1/incidents/:incidentId/resolve", async (params, route) => {
  const repository = await incidentRepository();
  return repository.resolve(DEMO_TENANT_ID, params.incidentId, route.body as IncidentActionPayload);
});

registerWasmRoute("POST", "/v1/incidents/:incidentId/comments", async (params, route) => {
  const repository = await incidentRepository();
  const { message, actor } = route.body as { message: string; actor?: string };
  return repository.addComment(DEMO_TENANT_ID, params.incidentId, message, actor);
});

registerWasmRoute("POST", "/v1/incidents/:incidentId/link-deployment", async (params, route) => {
  const repository = await incidentRepository();
  const { deployment_id, ...payload } = route.body as IncidentActionPayload & { deployment_id: string };
  return repository.linkDeployment(DEMO_TENANT_ID, params.incidentId, deployment_id, payload);
});

let sloRepositoryPromise: Promise<import("./sqliteSloRepository").SqliteSloRepository> | undefined;

async function sloRepository() {
  sloRepositoryPromise ??= (async () => {
    const { SqliteSloRepository } = await import("./sqliteSloRepository");
    return SqliteSloRepository.open();
  })();
  return sloRepositoryPromise;
}

registerWasmRoute("GET", "/v1/slos", async () => {
  const repository = await sloRepository();
  return repository.list(DEMO_TENANT_ID);
});

registerWasmRoute("POST", "/v1/slos", async (_params, route) => {
  const repository = await sloRepository();
  return repository.create(DEMO_TENANT_ID, route.body as CreateSloRequest);
});

let notificationChannelRepositoryPromise:
  | Promise<import("./sqliteNotificationChannelRepository").SqliteNotificationChannelRepository>
  | undefined;

async function notificationChannelRepository() {
  notificationChannelRepositoryPromise ??= (async () => {
    const { SqliteNotificationChannelRepository } = await import(
      "./sqliteNotificationChannelRepository"
    );
    return SqliteNotificationChannelRepository.open();
  })();
  return notificationChannelRepositoryPromise;
}

registerWasmRoute("GET", "/v1/notifications/channels", async () => {
  const repository = await notificationChannelRepository();
  return repository.list(DEMO_TENANT_ID);
});

registerWasmRoute("POST", "/v1/notifications/channels", async (_params, route) => {
  const repository = await notificationChannelRepository();
  return repository.create(DEMO_TENANT_ID, route.body as CreateChannelRequest);
});

registerWasmRoute("DELETE", "/v1/notifications/channels/:channelId", async (params) => {
  const repository = await notificationChannelRepository();
  repository.delete(DEMO_TENANT_ID, params.channelId);
});

let savedViewRepositoryPromise:
  | Promise<import("./sqliteSavedViewRepository").SqliteSavedViewRepository>
  | undefined;

async function savedViewRepository() {
  savedViewRepositoryPromise ??= (async () => {
    const { SqliteSavedViewRepository } = await import("./sqliteSavedViewRepository");
    return SqliteSavedViewRepository.open();
  })();
  return savedViewRepositoryPromise;
}

registerWasmRoute("GET", "/v1/saved-views", async (_params, route) => {
  const repository = await savedViewRepository();
  return repository.list(DEMO_TENANT_ID, route.query?.signal_kind as SignalKind);
});

registerWasmRoute("POST", "/v1/saved-views", async (_params, route) => {
  const repository = await savedViewRepository();
  return repository.create(DEMO_TENANT_ID, route.body as CreateSavedViewRequest);
});

registerWasmRoute("PUT", "/v1/saved-views/:savedViewId", async (params, route) => {
  const repository = await savedViewRepository();
  return repository.update(DEMO_TENANT_ID, params.savedViewId, route.body as UpdateSavedViewRequest);
});

registerWasmRoute("DELETE", "/v1/saved-views/:savedViewId", async (params) => {
  const repository = await savedViewRepository();
  repository.delete(DEMO_TENANT_ID, params.savedViewId);
});

registerWasmRoute("GET", "/v1/saved-views/:savedViewId/grants", async (params) => {
  const repository = await savedViewRepository();
  return repository.listGrants(DEMO_TENANT_ID, params.savedViewId);
});

registerWasmRoute("POST", "/v1/saved-views/:savedViewId/grants", async (params, route) => {
  const repository = await savedViewRepository();
  const { user_id, relation } = route.body as { user_id: string; relation: GrantItem["relation"] };
  repository.addGrant(DEMO_TENANT_ID, params.savedViewId, user_id, relation);
});

registerWasmRoute("DELETE", "/v1/saved-views/:savedViewId/grants/:userId", async (params) => {
  const repository = await savedViewRepository();
  repository.revokeGrant(DEMO_TENANT_ID, params.savedViewId, params.userId);
});
