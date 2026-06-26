/**
 * @modelable domain: pipeline
 * @modelable name: TxEvent
 * @modelable owner: crypto-demo-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface PipelineTxEventV1 {
  tx_hash: string;
  value_usd: number;
  block_height?: number;
  ts_unix_ms: number;
}
export type TxEvent = PipelineTxEventV1;
