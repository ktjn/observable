function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

function msToIso(ms?: number): string | undefined {
  return ms != null ? new Date(ms).toISOString() : undefined;
}

export interface UsageTelemetrySummary {
  spans: number;
  logs: number;
  metric_points: number;
  metric_series_created: number;
}

export interface UsageControlPlaneSummary {
  query_reads: number;
  query_rows: number;
  credential_checks: number;
  credential_allows: number;
  credential_denies: number;
}

export interface TenantUsageReportResponse {
  tenant_id: string;
  from: string;
  to: string;
  telemetry_summary: UsageTelemetrySummary;
  control_plane_summary: UsageControlPlaneSummary;
  estimated_cost_index: number;
}

export async function getTenantUsageReport(
  tenantId: string,
  params: {
    from?: number;
    to?: number;
  },
): Promise<TenantUsageReportResponse> {
  const url = new URL("/v1/tenants/usage-report", window.location.origin);
  const fromIso = msToIso(params.from);
  const toIso = msToIso(params.to);
  if (fromIso) url.searchParams.set("from", fromIso);
  if (toIso) url.searchParams.set("to", toIso);

  const res = await fetch(url.toString(), {
    credentials: "include",
    headers: tenantHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}
