/**
 * @modelable domain: nlq
 * @modelable name: NlqFilter
 * @modelable owner: platform-team
 * @modelable kind: value
 * @modelable version: 0
 * @modelable changeKind: additive
 */
export interface NlqNlqFilterV0 {
  field: string;
  op: string;
  value: string;
}
export type NlqFilter = NlqNlqFilterV0;
