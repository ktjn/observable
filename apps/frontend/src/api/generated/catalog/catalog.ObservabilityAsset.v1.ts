/**
 * @modelable domain: catalog
 * @modelable name: ObservabilityAsset
 * @modelable owner: service-management-lead
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface CatalogObservabilityAssetV1 {
  external_id: string;
  kind: 'alert_rule' | 'objective' | 'dashboard' | 'proactive_check' | 'health_rule' | 'runbook';
  owning_component: string;
  service_id: string;
  name: string;
  owner_kind?: 'user' | 'team';
  owner_id?: string;
  registered_at: string;
  updated_at: string;
}
export type ObservabilityAsset = CatalogObservabilityAssetV1;
