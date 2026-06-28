/**
 * @modelable domain: nlq
 * @modelable name: FieldRole
 * @modelable owner: platform-team
 * @modelable kind: value
 * @modelable version: 0
 * @modelable changeKind: additive
 */
export interface NlqFieldRoleV0 {
  name: string;
  role: 'time' | 'value' | 'bucket' | 'series' | 'label';
}
export type FieldRole = NlqFieldRoleV0;
