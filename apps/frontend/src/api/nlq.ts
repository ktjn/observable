const DEV_TENANT_ID = "00000000-0000-0000-0000-000000000001";

function tenantHeaders(): HeadersInit {
  return { "X-Tenant-ID": DEV_TENANT_ID };
}

// ── Types ─────────────────────────────────────────────────────────────────────

export interface FieldRole {
  name: string;
  role: "time" | "value" | "bucket" | "series" | "label";
}

export interface VisualizationFrame {
  frame_type:
    | "timeseries"
    | "histogram"
    | "heatmap"
    | "table"
    | "topk"
    | "flamegraph"
    | "distribution";
  x_field: string | null;
  y_field: string | null;
  series_field: string | null;
  unit: string | null;
  suggested_visualization: string;
  field_roles: FieldRole[];
  data: Record<string, unknown>[];
  // Provenance fields (ADR-021 — always present)
  nlq_ir: Record<string, unknown>;
  source_sql: string;
  time_range: { from: string; to: string };
  signal_types: string[];
  sample_rate: number | null;
  approximation_statement: string;
}

export interface NlqFrameResponse {
  type: "frame";
  frame: VisualizationFrame;
}

export interface NlqDeclineResponse {
  type: "decline";
  reason: string;
}

export interface NlqInvalidResponse {
  type: "invalid_response";
  reason: string;
  raw_llm_response: string;
}

export interface NlqCapabilitiesResponse {
  type: "capabilities";
  hint: string;
}

export type NlqResponse =
  | NlqFrameResponse
  | NlqDeclineResponse
  | NlqInvalidResponse
  | NlqCapabilitiesResponse;

// ── API function ──────────────────────────────────────────────────────────────

export interface NlqRequest {
  question: string;
  service_name?: string;
}

export async function submitNlqQuery(req: NlqRequest): Promise<NlqResponse> {
  const res = await fetch("/v1/nlq", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...tenantHeaders(),
    },
    body: JSON.stringify(req),
  });

  if (res.status === 503) {
    throw new Error("NLQ service is not configured on this server");
  }
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error(
      (body as { error?: string }).error ?? `NLQ request failed: ${res.status}`
    );
  }
  return res.json();
}
