import type { NlqIrLike } from "../features/nlq/queryFilters";

export type { NlqIrLike };

function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
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

export interface NlqIrResponse {
  type: "ir";
  ir: Record<string, unknown>;
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
  | NlqIrResponse
  | NlqDeclineResponse
  | NlqInvalidResponse
  | NlqCapabilitiesResponse;

// ── API function ──────────────────────────────────────────────────────────────

export interface NlqRequest {
  /** Natural-language question or raw IR JSON string. Optional when `base_ir` is set. */
  question?: string;
  service_name?: string;
  /**
   * Page base IR — defines the page surface and is used as the merge base for user NLQ.
   * When `question` is omitted, the backend executes `base_ir` directly (page-load pattern).
   * When `question` is present and mode is "execute", the backend merges the interpreted
   * user IR into `base_ir` (preserving base `operation`/`signals`) before execution.
   * When mode is "interpret", `base_ir` guides the LLM system prompt only.
   *
   * Replaces the former `surface_hint` string; LLM context is derived from
   * `base_ir.signals`/`base_ir.operation` directly.
   */
  base_ir?: NlqIrLike;
  mode?: "execute" | "interpret";
}

export async function submitNlqQuery(tenantId: string, req: NlqRequest): Promise<NlqResponse> {
  const res = await fetch("/v1/nlq", {
    credentials: "include",
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...tenantHeaders(tenantId),
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
