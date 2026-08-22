import type { TraceHistogramResponse, TraceListResponse } from "../api/traces";
import type { LogHistogramResponse } from "../api/logs";
import type { TenantListResponse, EnvironmentListResponse } from "../api/tenants";
import type { NlqRequest, NlqResponse } from "../api/nlq";
import type { CreateDashboardRequest, Dashboard } from "../api/dashboards";
import type { ServiceSummaryResponse, DiscoveryResponse, TopologyResponse } from "../api/services";
import type { MetricCatalogResponse, MetricPointsResponse, MetricCatalogEntry } from "../api/metrics";
import type { ListChangeEventsResponse, ListChangeEventsParams } from "../api/changeEvents";

export interface SearchTracesParams {
  service?: string;
  limit?: number;
  facets?: string[];
  from?: string;
  to?: string;
}

export interface TraceHistogramParams {
  service?: string;
  from?: string;
  to?: string;
  buckets?: number;
}

export interface LogHistogramParams {
  service?: string;
  from: string;
  to: string;
  buckets?: number;
}

export interface ServiceSummaryParams {
  environment?: string;
  from?: number;
  to?: number;
}

export interface TopologyParams {
  environment?: string;
  from?: number;
  to?: number;
  service?: string;
}

/**
 * Typed seam between UI components and either the production HTTP backend
 * (`httpRuntime`) or the browser-local playground engine (`playgroundRuntime`).
 * Components must depend only on this interface, never on the transport.
 */
export interface RuntimeApi {
  readonly mode: "http" | "playground";
  tenants: {
    list(): Promise<TenantListResponse>;
    listEnvironments(tenantId: string): Promise<EnvironmentListResponse>;
  };
  traces: {
    search(tenantId: string, params: SearchTracesParams): Promise<TraceListResponse>;
    histogram(tenantId: string, params: TraceHistogramParams): Promise<TraceHistogramResponse>;
  };
  logs: {
    histogram(tenantId: string, params: LogHistogramParams): Promise<LogHistogramResponse>;
  };
  services: {
    list(tenantId: string, params: ServiceSummaryParams): Promise<ServiceSummaryResponse>;
    listNames(tenantId: string): Promise<DiscoveryResponse>;
  };
  topology: {
    get(tenantId: string, params: TopologyParams): Promise<TopologyResponse>;
  };
  metrics: {
    list(tenantId: string, params: { service?: string }): Promise<MetricCatalogResponse>;
    points(tenantId: string, metric: MetricCatalogEntry): Promise<MetricPointsResponse>;
  };
  changeEvents: {
    list(tenantId: string, params: ListChangeEventsParams): Promise<ListChangeEventsResponse>;
  };
  nlq: {
    execute(tenantId: string, request: NlqRequest): Promise<NlqResponse>;
  };
  dashboards: {
    create(tenantId: string, request: CreateDashboardRequest): Promise<Dashboard>;
  };
}
