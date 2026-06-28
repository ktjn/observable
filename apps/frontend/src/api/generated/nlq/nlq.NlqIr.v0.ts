/**
 * @modelable domain: nlq
 * @modelable name: NlqIr
 * @modelable owner: platform-team
 * @modelable kind: value
 * @modelable version: 0
 * @modelable changeKind: additive
 */
export interface NlqNlqIrV0 {
  operation: 'timeseries' | 'rate' | 'irate' | 'increase' | 'histogram' | 'topk' | 'table' | 'distribution' | 'catalog' | 'inventory';
  signals: string[];
  filters: NlqFilter[];
  group_by: string[];
  time_range: NlqTimeRange;
  percentiles?: string[];
  catalog_field?: string;
  limit?: number;
  query?: string;
}
export type NlqIr = NlqNlqIrV0;
