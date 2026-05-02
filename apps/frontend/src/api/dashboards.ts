import type { Preset } from "../router";

const DEV_TENANT_ID = "00000000-0000-0000-0000-000000000001";

function tenantHeaders(): HeadersInit {
  return { "X-Tenant-ID": DEV_TENANT_ID };
}

export type DashboardQueryKind = "logs" | "traces" | "metrics";

export interface DashboardPanel {
  panel_id: string;
  title: string;
  query_kind: DashboardQueryKind;
  service?: string | null;
  preset: Preset | null;
  filters: Record<string, unknown>;
}

export interface Dashboard {
  dashboard_id: string;
  name: string;
  panels: DashboardPanel[];
  created_at: string;
}

export interface DashboardListResponse {
  items: Dashboard[];
}

export interface CreateDashboardRequest {
  name: string;
  panels: Array<{
    title: string;
    query_kind: DashboardQueryKind;
    service?: string;
    preset: Preset | null;
    filters: Record<string, unknown>;
  }>;
}

export async function listDashboards(): Promise<DashboardListResponse> {
  const res = await fetch("/v1/dashboards", { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Dashboard query failed: ${res.status}`);
  return res.json();
}

export async function createDashboard(req: CreateDashboardRequest): Promise<Dashboard> {
  const res = await fetch("/v1/dashboards", {
    method: "POST",
    headers: {
      ...tenantHeaders(),
      "Content-Type": "application/json",
    },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new Error(`Dashboard create failed: ${res.status}`);
  return res.json();
}
