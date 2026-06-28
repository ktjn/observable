/**
 * @modelable domain: slos
 * @modelable name: SloDefinition
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface SlosSloDefinitionV1 {
  slo_id: string;
  service_name: string;
  environment: string;
  sli_type: 'availability';
  target: number;
  window_days: number;
  burn_rate_fast_threshold: number;
  burn_rate_slow_threshold: number;
  description: string;
  firing: boolean;
  last_fired_at?: string;
  created_at: string;
  updated_at: string;
}
export type SloDefinition = SlosSloDefinitionV1;
