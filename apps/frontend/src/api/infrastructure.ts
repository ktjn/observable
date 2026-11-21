import { dispatch } from "../runtime/requestTransport";

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
  health_state: "healthy" | "watch" | "breach" | "unknown";
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
  _tenantId: string,
  params: Record<string, string> = {},
): Promise<InfrastructureInventoryResponse> {
  return dispatch<InfrastructureInventoryResponse>({
    method: "GET",
    path: "/v1/infrastructure",
    query: params,
  });
}

export async function getInfrastructureDetail(
  _tenantId: string,
  entityType: InfrastructureEntityType,
  entityId: string,
): Promise<InfrastructureDetailResponse> {
  return dispatch<InfrastructureDetailResponse>({
    method: "GET",
    path: `/v1/infrastructure/${encodeURIComponent(entityType)}/${encodeURIComponent(entityId)}`,
  });
}
