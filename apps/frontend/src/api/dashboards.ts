import type { Preset } from "../router";
import type { DashboardPanelLayout } from "./generated/dashboards/dashboards.DashboardPanelLayout.v0";
import type { DashboardPanel } from "./generated/dashboards/dashboards.DashboardPanel.v0";
import type { Dashboard } from "./generated/dashboards/dashboards.Dashboard.v1";
import { dispatch } from "../runtime/requestTransport";

export type DashboardQueryKind = "logs" | "traces" | "metrics";
export type DashboardPanelKind = "query" | "text";
export type { DashboardPanelLayout };
export type DashboardPanelTimeRange =
  | { mode: "global" }
  | { mode: "preset"; preset: Preset }
  | { mode: "absolute"; from_ms: number; to_ms: number };

export type { DashboardPanel };

export type { Dashboard };

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

export async function listDashboards(_tenantId: string): Promise<DashboardListResponse> {
  return dispatch<DashboardListResponse>({ method: "GET", path: "/v1/dashboards" });
}

export async function getDashboard(_tenantId: string, dashboardId: string): Promise<Dashboard> {
  return dispatch<Dashboard>({ method: "GET", path: `/v1/dashboards/${dashboardId}` });
}

export async function createDashboard(_tenantId: string, req: CreateDashboardRequest): Promise<Dashboard> {
  return dispatch<Dashboard>({ method: "POST", path: "/v1/dashboards", body: req });
}

export async function updateDashboard(
  _tenantId: string,
  dashboardId: string,
  req: UpdateDashboardRequest,
): Promise<Dashboard> {
  return dispatch<Dashboard>({ method: "PUT", path: `/v1/dashboards/${dashboardId}`, body: req });
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

export async function deleteDashboard(_tenantId: string, dashboardId: string): Promise<void> {
  await dispatch({ method: "DELETE", path: `/v1/dashboards/${dashboardId}` });
}

export async function exportDashboard(_tenantId: string, dashboardId: string): Promise<DashboardExport> {
  return dispatch<DashboardExport>({ method: "GET", path: `/v1/dashboards/${dashboardId}/export` });
}

export async function importDashboard(_tenantId: string, export_: DashboardExport): Promise<Dashboard> {
  return dispatch<Dashboard>({ method: "POST", path: "/v1/dashboards/import", body: export_ });
}
