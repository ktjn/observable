/**
 * @modelable domain: incidents
 * @modelable name: IncidentEvent
 * @modelable owner: platform-team
 * @modelable kind: event
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface IncidentsIncidentEventV1 {
  event_time: string;
  event_type: string;
  actor: string;
  message?: string;
}
export type IncidentEvent = IncidentsIncidentEventV1;
