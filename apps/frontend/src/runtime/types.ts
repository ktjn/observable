import type { TraceHistogramResponse, TraceListResponse, TraceResponse } from "../api/traces";
import type {
  LogHistogramResponse,
  LogListResponse,
} from "../api/logs";
import type { TenantListResponse, EnvironmentListResponse } from "../api/tenants";
import type { NlqRequest, NlqResponse } from "../api/nlq";
import type {
  CreateDashboardRequest,
  UpdateDashboardRequest,
  Dashboard,
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
import type { AlertRuleListResponse, AlertRuleDetailResponse, CreateRuleRequest, CreateRuleResponse } from "../api/alerts";
import type { IncidentDetailResponse, IncidentListResponse } from "../api/incidents";
import type { ListDeploymentsParams, ListDeploymentsResponse } from "../api/deployments";
import type { CreateSloRequest, SloDefinitionItem, SloListResponse } from "../api/slos";
import type { CreateChannelRequest, NotificationChannelItem } from "../api/notifications";
import type {
  CreateSavedViewRequest,
  GrantListResponse,
  SavedView,
  SavedViewListResponse,
  SignalKind,
  UpdateSavedViewRequest,
} from "../api/savedViews";

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

export interface LogSearchParams {
  service?: string;
  trace_id?: string;
  span_id?: string;
  from?: string;
  to?: string;
  limit?: number;
}

export interface LogTailParams {
  service?: string;
  severity?: number;
  since_unix_nano?: string;
  limit?: number;
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
    get(tenantId: string, traceId: string): Promise<TraceResponse>;
    histogram(tenantId: string, params: TraceHistogramParams): Promise<TraceHistogramResponse>;
  };
  logs: {
    histogram(tenantId: string, params: LogHistogramParams): Promise<LogHistogramResponse>;
    search(tenantId: string, params: LogSearchParams): Promise<LogListResponse>;
    context(tenantId: string, logId: string, params?: { window?: number }): Promise<LogListResponse>;
    tail(tenantId: string, params: LogTailParams): Promise<LogListResponse>;
  };
  services: {
    list(tenantId: string, params: ServiceSummaryParams): Promise<ServiceSummaryResponse>;
    listNames(tenantId: string): Promise<DiscoveryResponse>;
    summary(
      tenantId: string,
      serviceName: string,
      params: ServiceSummaryParams
    ): Promise<ServiceDetailResponse>;
    responseTimeHistory(
      tenantId: string,
      serviceName: string,
      params: { from?: number; to?: number; buckets?: number }
    ): Promise<ResponseTimeHistoryResponse>;
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
  alerts: {
    list(tenantId: string): Promise<AlertRuleListResponse>;
    get(tenantId: string, ruleId: string): Promise<AlertRuleDetailResponse>;
    create(tenantId: string, req: CreateRuleRequest): Promise<CreateRuleResponse>;
    silence(tenantId: string, ruleId: string, silenced: boolean): Promise<void>;
    setRunbook(tenantId: string, ruleId: string, runbookUrl: string | null): Promise<void>;
  };
  incidents: {
    list(tenantId: string, status?: string): Promise<IncidentListResponse>;
    get(tenantId: string, incidentId: string): Promise<IncidentDetailResponse>;
  };
  slos: {
    list(tenantId: string): Promise<SloListResponse>;
    create(tenantId: string, req: CreateSloRequest): Promise<SloDefinitionItem>;
  };
  notificationChannels: {
    list(tenantId: string): Promise<NotificationChannelItem[]>;
    create(tenantId: string, req: CreateChannelRequest): Promise<NotificationChannelItem>;
    delete(tenantId: string, channelId: string): Promise<void>;
  };
  savedViews: {
    list(tenantId: string, signalKind: SignalKind): Promise<SavedViewListResponse>;
    create(tenantId: string, req: CreateSavedViewRequest): Promise<SavedView>;
    update(tenantId: string, savedViewId: string, req: UpdateSavedViewRequest): Promise<SavedView>;
    delete(tenantId: string, savedViewId: string): Promise<void>;
    listGrants(tenantId: string, savedViewId: string): Promise<GrantListResponse>;
    addGrant(
      tenantId: string,
      savedViewId: string,
      userId: string,
      relation: "owner" | "editor" | "viewer"
    ): Promise<void>;
    revokeGrant(tenantId: string, savedViewId: string, userId: string): Promise<void>;
  };
  deployments: {
    list(tenantId: string, params: ListDeploymentsParams): Promise<ListDeploymentsResponse>;
  };
  nlq: {
    execute(tenantId: string, request: NlqRequest): Promise<NlqResponse>;
  };
  dashboards: {
    list(tenantId: string): Promise<DashboardListResponse>;
    get(tenantId: string, dashboardId: string): Promise<Dashboard>;
    create(tenantId: string, request: CreateDashboardRequest): Promise<Dashboard>;
    update(tenantId: string, dashboardId: string, request: UpdateDashboardRequest): Promise<Dashboard>;
    delete(tenantId: string, dashboardId: string): Promise<void>;
    export(tenantId: string, dashboardId: string): Promise<DashboardExport>;
    import(tenantId: string, export_: DashboardExport): Promise<Dashboard>;
  };
}
