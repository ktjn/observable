/**
 * @modelable domain: dashboards
 * @modelable name: DashboardPanel
 * @modelable owner: platform-team
 * @modelable kind: value
 * @modelable version: 0
 * @modelable changeKind: additive
 */
export interface DashboardsDashboardPanelV0 {
  panel_id: string;
  title: string;
  panel_kind: 'query' | 'text';
  query_kind?: 'logs' | 'traces' | 'metrics';
  service?: string;
  preset?: string;
  filters: unknown;
  query_text?: string;
  content?: string;
  layout: DashboardPanelLayout;
  time_range: unknown;
}
export type DashboardPanel = DashboardsDashboardPanelV0;
