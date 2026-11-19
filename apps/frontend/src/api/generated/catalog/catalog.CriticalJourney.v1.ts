/**
 * @modelable domain: catalog
 * @modelable name: CriticalJourney
 * @modelable owner: service-management-lead
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface CatalogCriticalJourneyV1 {
  id: string;
  name: string;
  description: string;
  criticality: 'critical' | 'high' | 'standard' | 'low';
  in_scope: boolean;
  scope_rationale: string;
  classification: 'public' | 'internal' | 'confidential' | 'restricted';
  owner_kind?: 'user' | 'team';
  owner_id?: string;
  created_at: string;
  updated_at: string;
}
export type CriticalJourney = CatalogCriticalJourneyV1;
