function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

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
  tenantId: string,
  params: ListChangeEventsParams = {},
): Promise<ListChangeEventsResponse> {
  const url = new URL("/v1/events/changes", window.location.origin);
  if (params.service_name) url.searchParams.set("service_name", params.service_name);
  if (params.environment) url.searchParams.set("environment", params.environment);
  if (params.event_type) url.searchParams.set("event_type", params.event_type);
  if (params.start_time) url.searchParams.set("start_time", params.start_time);
  if (params.end_time) url.searchParams.set("end_time", params.end_time);
  if (params.limit !== undefined) url.searchParams.set("limit", String(params.limit));

  const res = await fetch(url.toString(), { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Change events fetch failed: ${res.status}`);
  return res.json();
}
