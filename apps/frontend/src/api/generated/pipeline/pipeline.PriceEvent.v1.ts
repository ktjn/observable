/**
 * @modelable domain: pipeline
 * @modelable name: PriceEvent
 * @modelable owner: crypto-demo-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface PipelinePriceEventV1 {
  event_id: string;
  asset: string;
  chain: string;
  price_usd: number;
  source: 'DexPaprika' | 'Coinbase';
  ts_unix_ms: number;
}
export type PriceEvent = PipelinePriceEventV1;
