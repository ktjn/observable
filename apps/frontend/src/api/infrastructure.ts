function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

export type InfrastructureEntityType =
  | "host"
  | "cluster"
  | "namespace"
  | "pod"
  | "container";

export interface InfrastructureEntitySummary {
  entity_type: InfrastructureEntityType;
  entity_id: string;
  display_name: string;
  parent_id: string | null;
  parent_display_name: string | null;
  environment: string | null;
  health_state: "healthy" | "watch" | "breach";
  last_seen_unix_nano: number;
  related_services: string[];
  log_rate_per_minute: number | null;
  error_rate: number | null;
  restart_count: number | null;
  cpu_usage: number | null;
  memory_usage: number | null;
  disk_usage: number | null;
  network_io: number | null;
}

export interface InfrastructureInventoryResponse {
  items: InfrastructureEntitySummary[];
}

export interface InfrastructureDetailResponse {
  entity: InfrastructureEntitySummary;
  links: {
    logs: string;
    traces: string;
    metrics: string;
  };
}

export async function listInfrastructure(
  tenantId: string,
  params: Record<string, string> = {},
): Promise<InfrastructureInventoryResponse> {
  const url = new URL("/v1/infrastructure", window.location.origin);
  for (const [key, value] of Object.entries(params)) {
    if (value) url.searchParams.set(key, value);
  }

  const res = await fetch(url.toString(), { headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}

export async function getInfrastructureDetail(
  tenantId: string,
  entityType: InfrastructureEntityType,
  entityId: string,
): Promise<InfrastructureDetailResponse> {
  const url = new URL(
    `/v1/infrastructure/${encodeURIComponent(entityType)}/${encodeURIComponent(entityId)}`,
    window.location.origin,
  );

  const res = await fetch(url.toString(), { headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}
