/**
 * @modelable domain: alerts
 * @modelable name: AlertRule
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface AlertsAlertRuleV1 {
  rule_id: string;
  name: string;
  metric_name: string;
  operator: 'gt' | 'gte' | 'lt' | 'lte' | 'eq';
  threshold: number;
  severity: string;
  silenced: boolean;
  state: 'ok' | 'pending' | 'active' | 'resolved' | 'silenced' | 'suppressed';
  firing: boolean;
  last_fired_at?: string;
  notification_channels: string[];
  auto_trigger_incident: boolean;
  service_name?: string;
  suppressed: boolean;
}
export type AlertRule = AlertsAlertRuleV1;
