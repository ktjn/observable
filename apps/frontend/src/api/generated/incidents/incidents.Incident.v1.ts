/**
 * @modelable domain: incidents
 * @modelable name: Incident
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface IncidentsIncidentV1 {
  incident_id: string;
  title: string;
  severity: string;
  status: string;
  triggered_at: string;
  resolved_at?: string;
  triggered_by_rule_id?: string;
}
export type Incident = IncidentsIncidentV1;
