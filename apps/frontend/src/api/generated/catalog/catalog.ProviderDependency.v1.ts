/**
 * @modelable domain: catalog
 * @modelable name: ProviderDependency
 * @modelable owner: service-management-lead
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface CatalogProviderDependencyV1 {
  id: string;
  provider_id?: string;
  provider_name: string;
  service_id?: string;
  journey_id?: string;
  function: string;
}
export type ProviderDependency = CatalogProviderDependencyV1;
