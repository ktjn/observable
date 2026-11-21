import { dispatch } from "../runtime/requestTransport";

function msToIso(ms?: number): string | undefined {
  return ms != null ? new Date(ms).toISOString() : undefined;
}

export interface MetricCardinalitySummary {
  metric_name: string;
  series_count: number;
}

export interface UsageTelemetrySummary {
  spans: number;
  logs: number;
  metric_points: number;
  metric_series_created: number;
  active_metric_series?: number;
  top_cardinality_metrics?: MetricCardinalitySummary[];
}

export interface UsageControlPlaneSummary {
  query_reads: number;
  query_rows: number;
  credential_checks: number;
  credential_allows: number;
  credential_denies: number;
  token_operations?: number;
}

export interface TenantUsageReportResponse {
  tenant_id: string;
  from: string;
  to: string;
  retention_days?: number;
  ingest_quota_per_sec?: number;
  telemetry_summary: UsageTelemetrySummary;
  control_plane_summary: UsageControlPlaneSummary;
  estimated_cost_index: number;
}

export async function getTenantUsageReport(
  _tenantId: string,
  params: {
    from?: number;
    to?: number;
  },
): Promise<TenantUsageReportResponse> {
  return dispatch<TenantUsageReportResponse>({
    method: "GET",
    path: "/v1/tenants/usage-report",
    query: { from: msToIso(params.from), to: msToIso(params.to) },
  });
}
