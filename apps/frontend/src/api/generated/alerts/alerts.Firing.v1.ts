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
  state: 'pending' | 'active' | 'resolved';
  value?: number;
  occurred_at: string;
  resolved_at?: string;
}
export type Firing = AlertsFiringV1;
