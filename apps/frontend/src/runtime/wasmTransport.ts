import type { HttpMethod, RequestTransport, RouteRequest } from "./requestTransport";
import type { LogRecord } from "../api/logs";
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

/** `pattern` segments prefixed with `:` bind that segment into `params`. */
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
