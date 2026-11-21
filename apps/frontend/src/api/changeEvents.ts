import { dispatch } from "../runtime/requestTransport";

export type ChangeEventType = "config_change" | "feature_flag" | "migration" | "incident" | "other";

export interface ChangeEvent {
  change_event_id: string;
  tenant_id: string;
  project_id: string | null;
  event_type: ChangeEventType;
  service_name: string | null;
  environment: string;
  title: string;
  description: string | null;
  occurred_at: string;
  source: string | null;
  created_by: string | null;
  metadata: Record<string, unknown> | null;
}

export interface ListChangeEventsResponse {
  items: ChangeEvent[];
}

export interface ListChangeEventsParams {
  service_name?: string;
  environment?: string;
  event_type?: ChangeEventType;
  start_time?: string;
  end_time?: string;
  limit?: number;
}

export async function listChangeEvents(
  _tenantId: string,
  params: ListChangeEventsParams = {},
): Promise<ListChangeEventsResponse> {
  return dispatch<ListChangeEventsResponse>({
    method: "GET",
    path: "/v1/events/changes",
    query: {
      service_name: params.service_name,
      environment: params.environment,
      event_type: params.event_type,
      start_time: params.start_time,
      end_time: params.end_time,
      limit: params.limit,
    },
  });
}
