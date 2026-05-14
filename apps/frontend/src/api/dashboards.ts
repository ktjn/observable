import type { Preset } from "../router";

function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

export type DashboardQueryKind = "logs" | "traces" | "metrics";
export type DashboardPanelKind = "query" | "text";
export type DashboardPanelLayout = { x: number; y: number; w: number; h: number };
export type DashboardPanelTimeRange =
  | { mode: "global" }
  | { mode: "preset"; preset: Preset }
  | { mode: "absolute"; from_ms: number; to_ms: number };

export interface DashboardPanel {
  panel_id: string;
  title: string;
  panel_kind: DashboardPanelKind;
  query_kind: DashboardQueryKind | null;
  service?: string | null;
  preset: Preset | null;
  filters: Record<string, unknown>;
  query_text?: string | null;
  content?: string | null;
  layout: DashboardPanelLayout;
  time_range: DashboardPanelTimeRange;
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
    panel_kind?: DashboardPanelKind;
    query_kind?: DashboardQueryKind | null;
    service?: string;
    preset: Preset | null;
    filters: Record<string, unknown>;
    query_text?: string | null;
    content?: string | null;
    layout?: DashboardPanelLayout;
    time_range?: DashboardPanelTimeRange;
  }>;
}

export interface UpdateDashboardRequest {
  name: string;
  panels: Array<{
    panel_id?: string;
    title: string;
    panel_kind: DashboardPanelKind;
    query_kind: DashboardQueryKind | null;
    service?: string | null;
    preset: Preset | null;
    filters: Record<string, unknown>;
    query_text?: string | null;
    content?: string | null;
    layout: DashboardPanelLayout;
    time_range: DashboardPanelTimeRange;
  }>;
}

export async function listDashboards(tenantId: string): Promise<DashboardListResponse> {
  const res = await fetch("/v1/dashboards", { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Dashboard query failed: ${res.status}`);
  return res.json();
}

export async function getDashboard(tenantId: string, dashboardId: string): Promise<Dashboard> {
  const res = await fetch(`/v1/dashboards/${dashboardId}`, { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Dashboard get failed: ${res.status}`);
  return res.json();
}

export async function createDashboard(tenantId: string, req: CreateDashboardRequest): Promise<Dashboard> {
  const res = await fetch("/v1/dashboards", {
    credentials: "include",
    method: "POST",
    headers: {
      ...tenantHeaders(tenantId),
      "Content-Type": "application/json",
    },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new Error(`Dashboard create failed: ${res.status}`);
  return res.json();
}

export async function updateDashboard(
  tenantId: string,
  dashboardId: string,
  req: UpdateDashboardRequest,
): Promise<Dashboard> {
  const res = await fetch(`/v1/dashboards/${dashboardId}`, {
    credentials: "include",
    method: "PUT",
    headers: {
      ...tenantHeaders(tenantId),
      "Content-Type": "application/json",
    },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new Error(`Dashboard update failed: ${res.status}`);
  return res.json();
}

export interface DashboardExportPanel {
  title: string;
  panel_kind?: DashboardPanelKind;
  query_kind?: DashboardQueryKind | null;
  service?: string | null;
  preset?: Preset | null;
  filters: Record<string, unknown>;
  query_text?: string | null;
  content?: string | null;
  layout?: DashboardPanelLayout;
  time_range?: DashboardPanelTimeRange;
}

export interface DashboardExport {
  schema_version: string;
  name: string;
  panels: DashboardExportPanel[];
}

export async function deleteDashboard(tenantId: string, dashboardId: string): Promise<void> {
  const res = await fetch(`/v1/dashboards/${dashboardId}`, {
    credentials: "include",
    method: "DELETE",
    headers: tenantHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`Dashboard delete failed: ${res.status}`);
}

export async function exportDashboard(tenantId: string, dashboardId: string): Promise<DashboardExport> {
  const res = await fetch(`/v1/dashboards/${dashboardId}/export`, {
    credentials: "include",
    headers: tenantHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`Dashboard export failed: ${res.status}`);
  return res.json();
}

export async function importDashboard(tenantId: string, export_: DashboardExport): Promise<Dashboard> {
  const res = await fetch("/v1/dashboards/import", {
    credentials: "include",
    method: "POST",
    headers: {
      ...tenantHeaders(tenantId),
      "Content-Type": "application/json",
    },
    body: JSON.stringify(export_),
  });
  if (!res.ok) throw new Error(`Dashboard import failed: ${res.status}`);
  return res.json();
}
