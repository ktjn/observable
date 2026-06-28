/**
 * @modelable domain: dashboards
 * @modelable name: Dashboard
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
import type { DashboardPanel } from "./dashboards.DashboardPanel.v0";

export interface DashboardsDashboardV1 {
  dashboard_id: string;
  name: string;
  visibility: 'public' | 'private';
  panels: DashboardPanel[];
  created_at: string;
}
export type Dashboard = DashboardsDashboardV1;
