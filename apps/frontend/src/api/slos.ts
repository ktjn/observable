import type { SloDefinition as SloDefinitionItem } from "./generated/slos/slos.SloDefinition.v1";
import { dispatch } from "../runtime/requestTransport";
export type { SloDefinitionItem };

export interface SloListResponse {
  items: SloDefinitionItem[];
}

export type CommitmentTier = "objective" | "internal_slo" | "contractual_sla";
export type SloApprovalStatus = "draft" | "approved";

export interface CreateSloRequest {
  service_name: string;
  environment: string;
  target: number;
  window_days: number;
  burn_rate_fast_threshold: number;
  burn_rate_slow_threshold: number;
  description?: string;
  sli_outcome?: string;
  sli_scope?: string;
  commitment_tier?: CommitmentTier;
  approval_status?: SloApprovalStatus;
}

export function formatCommitmentTier(tier?: string): string {
  switch (tier) {
    case "contractual_sla":
      return "Contractual SLA";
    case "objective":
      return "Objective";
    case "internal_slo":
    default:
      return "Internal SLO";
  }
}

export function isSloApproved(slo: {
  approval_status?: string;
  approved_by?: string | null;
  window_days?: number;
  sli_outcome?: string;
  sli_scope?: string;
}): boolean {
  return (
    slo.approval_status === "approved" &&
    Boolean(slo.approved_by) &&
    (slo.window_days ?? 0) > 0 &&
    Boolean(slo.sli_outcome?.trim() && slo.sli_scope?.trim())
  );
}

export interface SloBudgetSummary {
  attainmentPct: number | null;
  remainingBudgetPct: number | null;
  breached: boolean;
  missingIntervals: number;
  evidenceDisclosure: string | null;
}

/**
 * Computes attainment, breach state, and remaining error budget.
 * Missing evidence intervals are disclosed and never counted as good events (OBS-06-04).
 */
export function computeSloBudgetSummary(
  target: number,
  evaluation?: {
    total_events?: number;
    good_events?: number;
    expected_intervals?: number;
    observed_intervals?: number;
  },
): SloBudgetSummary {
  const totalEvents = evaluation?.total_events ?? 0;
  const goodEvents = evaluation?.good_events ?? 0;
  const expectedIntervals = evaluation?.expected_intervals ?? 0;
  const observedIntervals = evaluation?.observed_intervals ?? 0;
  const missingIntervals = Math.max(0, expectedIntervals - observedIntervals);

  const evidenceDisclosure =
    missingIntervals > 0 || totalEvents === 0
      ? `Missing evidence for ${missingIntervals} of ${expectedIntervals} evaluation intervals; missing intervals are not counted as good events.`
      : null;

  if (totalEvents <= 0) {
    return {
      attainmentPct: null,
      remainingBudgetPct: null,
      breached: false,
      missingIntervals,
      evidenceDisclosure,
    };
  }

  const boundedGood = Math.min(goodEvents, totalEvents);
  const attainment = boundedGood / totalEvents;
  const budgetTotal = Math.max(0, 1 - target);
  const budgetConsumed = Math.max(0, 1 - attainment);
  const remainingRatio =
    budgetTotal > 0 ? Math.max(-1, Math.min(1, (budgetTotal - budgetConsumed) / budgetTotal)) : 0;

  return {
    attainmentPct: attainment * 100,
    remainingBudgetPct: remainingRatio * 100,
    breached: attainment < target,
    missingIntervals,
    evidenceDisclosure,
  };
}

export async function listSlos(_tenantId: string): Promise<SloListResponse> {
  return dispatch<SloListResponse>({ method: "GET", path: "/v1/slos" });
}

export async function createSlo(
  _tenantId: string,
  req: CreateSloRequest,
): Promise<SloDefinitionItem> {
  return dispatch<SloDefinitionItem>({ method: "POST", path: "/v1/slos", body: req });
}
