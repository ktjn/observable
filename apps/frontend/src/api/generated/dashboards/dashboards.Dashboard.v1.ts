import type { DashboardsDashboardPanelV0 } from "./dashboards.DashboardPanel.v0";

/**
 * @modelable domain: dashboards
 * @modelable name: Dashboard
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface DashboardsDashboardV1 {
  dashboard_id: string;
  name: string;
  visibility: 'public' | 'private';
  panels: DashboardsDashboardPanelV0[];
  created_at: string;
}
export type Dashboard = DashboardsDashboardV1;
