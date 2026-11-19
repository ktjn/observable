/**
 * @modelable domain: catalog
 * @modelable name: Provider
 * @modelable owner: service-management-lead
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface CatalogProviderV1 {
  id: string;
  name: string;
  kind: 'cloud' | 'saas' | 'network' | 'payment' | 'data' | 'other';
  material: boolean;
  contract_ref: string;
  contract_owner_kind?: 'user' | 'team';
  contract_owner_id?: string;
  classification: 'public' | 'internal' | 'confidential' | 'restricted';
  created_at: string;
  updated_at: string;
}
export type Provider = CatalogProviderV1;
