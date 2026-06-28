import type { NlqNlqFilterV0 } from "./nlq.NlqFilter.v0";
import type { NlqNlqTimeRangeV0 } from "./nlq.NlqTimeRange.v0";

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
  filters: NlqNlqFilterV0[];
  group_by: string[];
  time_range: NlqNlqTimeRangeV0;
  percentiles?: string[];
  catalog_field?: string;
  limit?: number;
  query?: string;
}
export type NlqIr = NlqNlqIrV0;
