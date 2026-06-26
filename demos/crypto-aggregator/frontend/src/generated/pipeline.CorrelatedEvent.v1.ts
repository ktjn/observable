/**
 * @modelable domain: pipeline
 * @modelable name: CorrelatedEvent
 * @modelable owner: crypto-demo-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface PipelineCorrelatedEventV1 {
  correlation_id: string;
  asset: string;
  tx_hash: string;
  price_usd: number;
  lag_ms: number;
  price_source: 'DexPaprika' | 'Coinbase';
  ts_unix_ms: number;
}
export type CorrelatedEvent = PipelineCorrelatedEventV1;
