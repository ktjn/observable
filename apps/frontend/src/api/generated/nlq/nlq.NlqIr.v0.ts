/**
 * @modelable domain: nlq
 * @modelable name: NlqIr
 * @modelable owner: platform-team
 * @modelable kind: value
 * @modelable version: 0
 * @modelable changeKind: additive
 */
import type { NlqFilter } from "./nlq.NlqFilter.v0";
import type { NlqTimeRange } from "./nlq.NlqTimeRange.v0";
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
