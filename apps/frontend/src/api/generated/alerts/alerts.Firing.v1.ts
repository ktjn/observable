/**
 * @modelable domain: alerts
 * @modelable name: Firing
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface AlertsFiringV1 {
  firing_id: string;
  state: 'pending' | 'active' | 'resolved' | 'suppressed';
  value?: number;
  occurred_at: string;
  resolved_at?: string;
  suppressed_by_rule_name?: string;
}
export type Firing = AlertsFiringV1;
