/**
 * @modelable domain: catalog
 * @modelable name: CoverageGap
 * @modelable owner: service-management-lead
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface CatalogCoverageGapV1 {
  id: string;
  service_id?: string;
  journey_id?: string;
  item: string;
  state: 'open' | 'remediating' | 'accepted' | 'closed';
  remediation_owner_kind?: 'user' | 'team';
  remediation_owner_id?: string;
  due_date?: string;
  review_date?: string;
  acceptance_approver?: string;
  acceptance_rationale?: string;
  closed_reason?: 'remediated' | 'risk_accepted';
  created_at: string;
  updated_at: string;
}
export type CoverageGap = CatalogCoverageGapV1;
