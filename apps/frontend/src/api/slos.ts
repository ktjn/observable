function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

export interface SloDefinitionItem {
  slo_id: string;
  service_name: string;
  environment: string;
  sli_type: "availability";
  target: number;
  window_days: number;
  burn_rate_fast_threshold: number;
  burn_rate_slow_threshold: number;
  description: string;
  firing: boolean;
  last_fired_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface SloListResponse {
  items: SloDefinitionItem[];
}

export interface CreateSloRequest {
  service_name: string;
  environment: string;
  target: number;
  window_days: number;
  burn_rate_fast_threshold: number;
  burn_rate_slow_threshold: number;
  description?: string;
}

export async function listSlos(tenantId: string): Promise<SloListResponse> {
  const res = await fetch("/v1/slos", { headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Failed to list SLOs: ${res.status}`);
  return res.json();
}

export async function createSlo(
  tenantId: string,
  req: CreateSloRequest,
): Promise<SloDefinitionItem> {
  const res = await fetch("/v1/slos", {
    method: "POST",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new Error(`Failed to create SLO: ${res.status}`);
  return res.json();
}
