/**
 * @modelable domain: catalog
 * @modelable name: CoveragePolicy
 * @modelable owner: service-management-lead
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface CatalogCoveragePolicyV1 {
  criticality: 'critical' | 'high' | 'standard' | 'low';
  items: string[];
  window_minutes: number;
  approved_by?: string;
  approved_at?: string;
}
export type CoveragePolicy = CatalogCoveragePolicyV1;
