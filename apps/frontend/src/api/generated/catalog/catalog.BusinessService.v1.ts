/**
 * @modelable domain: catalog
 * @modelable name: BusinessService
 * @modelable owner: service-management-lead
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface CatalogBusinessServiceV1 {
  id: string;
  business_name: string;
  operational_name: string;
  domain: string;
  description: string;
  criticality: 'critical' | 'high' | 'standard' | 'low';
  lifecycle: 'planned' | 'live' | 'deprecated' | 'retired';
  classification: 'public' | 'internal' | 'confidential' | 'restricted';
  support_route: string;
  operated_by: 'internal' | 'third_party';
  operator_provider_id?: string;
  created_at: string;
  updated_at: string;
}
export type BusinessService = CatalogBusinessServiceV1;
