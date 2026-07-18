import type { NlqIrLike } from "../features/nlq/queryFilters";
import type { NlqIr as GeneratedNlqIr } from "./generated/nlq/nlq.NlqIr.v0";
import type { NlqFilter } from "./generated/nlq/nlq.NlqFilter.v0";
import type { NlqTimeRange } from "./generated/nlq/nlq.NlqTimeRange.v0";
import type { FieldRole } from "./generated/nlq/nlq.FieldRole.v0";

export type { NlqIrLike, NlqFilter, NlqTimeRange, FieldRole };

function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

// ── Types ─────────────────────────────────────────────────────────────────────

// Derived from the generated NlqIr/FieldRole literal unions — no duplication.
export type NlqOperation = GeneratedNlqIr["operation"];
export type FieldRoleKind = FieldRole["role"];

// Hand-written: array<enum(...)> emits invalid TS (Phase 1 backlog item 9),
// so NlqIr.signals is generated as string[] and NlqSignal can't be derived.
// Mirrors libs/domain/src/nlq.rs::NlqSignal (rename_all = "lowercase").
export type NlqSignal = "metrics" | "traces" | "logs";

// Hand-written: shared by NlqIr.visualization_hint (Phase 1 backlog item 8 -
// Option<T> without skip_serializing_if, can't be generated) and
// VisualizationFrame.frame_type (libs/domain/src/visualization.rs's
// `impl From<NlqVisualizationHint> for VisualizationFrameType` - identical
// 7-variant value sets). Mirrors libs/domain/src/nlq.rs::NlqVisualizationHint
// / libs/domain/src/visualization.rs::VisualizationFrameType
// (both rename_all = "snake_case").
export type NlqVisualizationHint =
  | "timeseries"
  | "histogram"
  | "heatmap"
  | "table"
  | "topk"
  | "flamegraph"
  | "distribution";

export type VisualizationFrameType = NlqVisualizationHint;

// Adds back the 4 fields nlq.mdl's NlqIr can't represent (Phase 1 backlog
// item 8) and narrows `signals` from string[] to NlqSignal[] (Phase 1
// backlog item 9).
export interface NlqIr extends GeneratedNlqIr {
  signals: NlqSignal[];
  metric: string | null;
  window: string | null;
  resolution: string | null;
  visualization_hint: NlqVisualizationHint | null;
}

export interface VisualizationFrame {
  frame_type: VisualizationFrameType;
  x_field: string | null;
  y_field: string | null;
  series_field: string | null;
  unit: string | null;
  suggested_visualization: string;
  field_roles: FieldRole[];
  data: Record<string, unknown>[];
  // Provenance fields (ADR-021 — always present)
  nlq_ir: NlqIr;
  source_sql: string;
  time_range: NlqTimeRange;
  signal_types: NlqSignal[];
  sample_rate: number | null;
  approximation_statement: string;
}

export interface NlqFrameResponse {
  type: "frame";
  frame: VisualizationFrame;
}

export interface NlqIrResponse {
  type: "ir";
  ir: NlqIr;
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

// ── Two-phase prepare/complete (WebLLM client-side inference) ──────────────────
//
// `NlqRequest` already covers the `/v1/nlq/prepare` body shape (question/service_name/
// base_ir/mode), so it's reused directly rather than duplicating an identical type.
export type NlqPrepareRequest = NlqRequest;

export type NlqPrepareResult =
  | { type: "final"; response: NlqResponse }
  | { type: "prepared"; session_token: string; system_prompt: string; question: string };

export async function prepareNlqQuery(
  tenantId: string,
  req: NlqPrepareRequest
): Promise<NlqPrepareResult> {
  const res = await fetch("/v1/nlq/prepare", {
    credentials: "include",
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...tenantHeaders(tenantId),
    },
    body: JSON.stringify(req),
  });

  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error(
      (body as { error?: string }).error ?? `NLQ prepare request failed: ${res.status}`
    );
  }
  return res.json();
}

export type NlqCompleteResult =
  | { type: "final"; response: NlqResponse }
  | { type: "needs_repair"; repair_prompt: string };

export async function completeNlqQuery(
  tenantId: string,
  sessionToken: string,
  rawLlmResponse: string
): Promise<NlqCompleteResult> {
  const res = await fetch("/v1/nlq/complete", {
    credentials: "include",
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...tenantHeaders(tenantId),
    },
    body: JSON.stringify({ session_token: sessionToken, raw_llm_response: rawLlmResponse }),
  });

  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error(
      (body as { error?: string }).error ?? `NLQ complete request failed: ${res.status}`
    );
  }
  return res.json();
}
