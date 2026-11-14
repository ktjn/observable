import { useState } from "react";
import { Badge } from "../../../components/ui/badge";
import { Button } from "../../../components/ui/button";
import { Panel } from "../../../components/ui/panel";

export interface ExplorerRefinement {
  field: string;
  value: string;
}

export interface ComparisonSubject {
  id: string;
  label: string;
  errorRatePercent: number;
  p95LatencyMs: number;
  requestRateRps: number;
}

export interface AnomalyCandidate {
  metric: string;
  deltaSummary: string;
  comparisonPeriod: string;
  uncertaintyLabel: string;
}

export interface ExplorerControlsBarProps {
  visualMode: "all" | "failures" | "latency" | "usage";
  onVisualModeChange: (mode: "all" | "failures" | "latency" | "usage") => void;
  refinements: ExplorerRefinement[];
  onUndoRefinement: () => void;
  groupBy: string;
  onGroupByChange: (field: string) => void;
  comparisonSubjects?: ComparisonSubject[];
  selectedCompareIds?: string[];
  onCompareIdsChange?: (ids: string[]) => void;
  comparisonWindowLabel?: string;
  anomalyCandidate?: AnomalyCandidate;
  shareUrl?: string;
  onAdvancedQueryApply?: (expr: string) => void;
}

const VISUAL_MODES: Array<{ id: ExplorerControlsBarProps["visualMode"]; label: string }> = [
  { id: "all", label: "All evidence" },
  { id: "failures", label: "Failures" },
  { id: "latency", label: "High latency" },
  { id: "usage", label: "Usage" },
];

export function buildSharedViewUrl(
  basePath: string,
  refinements: ExplorerRefinement[],
  periodPreset: string,
  service?: string,
): string {
  const params = new URLSearchParams();
  if (service) params.set("service", service);
  if (periodPreset) params.set("preset", periodPreset);
  for (const r of refinements) {
    params.append("filter", `${r.field}:${r.value}`);
  }
  const qs = params.toString();
  return qs ? `${basePath}?${qs}` : basePath;
}

export function ExplorerControlsBar({
  visualMode,
  onVisualModeChange,
  refinements,
  onUndoRefinement,
  groupBy,
  onGroupByChange,
  comparisonSubjects = [],
  selectedCompareIds = [],
  onCompareIdsChange,
  comparisonWindowLabel = "Same incident window",
  anomalyCandidate,
  shareUrl,
  onAdvancedQueryApply,
}: ExplorerControlsBarProps) {
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [advancedExpr, setAdvancedExpr] = useState("");
  const [sharedCopied, setSharedCopied] = useState(false);
  const [compareTarget, setCompareTarget] = useState("");

  const activeComparisons = comparisonSubjects.filter((s) =>
    selectedCompareIds.includes(s.id),
  );

  function handleShare() {
    if (shareUrl && typeof navigator !== "undefined" && navigator.clipboard) {
      navigator.clipboard.writeText(shareUrl).catch(() => {});
    }
    setSharedCopied(true);
  }

  return (
    <div className="space-y-2" data-testid="explorer-controls-bar">
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <div className="flex items-center gap-1" role="group" aria-label="Visual exploration modes">
          {VISUAL_MODES.map((m) => (
            <Button
              key={m.id}
              variant={visualMode === m.id ? "primary" : "secondary"}
              data-testid={`visual-mode-${m.id}`}
              onClick={() => onVisualModeChange(m.id)}
            >
              {m.label}
            </Button>
          ))}
        </div>

        <select
          aria-label="Group results by"
          data-testid="explorer-group-by"
          value={groupBy}
          onChange={(e) => onGroupByChange(e.target.value)}
          className="rounded border border-[var(--border)] bg-[var(--surface-1)] px-2 py-1 text-xs"
        >
          <option value="">No grouping</option>
          <option value="service">Group by service</option>
          <option value="version">Group by version</option>
          <option value="location">Group by location</option>
        </select>

        {comparisonSubjects.length > 0 && onCompareIdsChange && (
          <div className="flex items-center gap-1" data-testid="explorer-compare-control">
            <select
              aria-label="Compare target"
              value={compareTarget}
              onChange={(e) => {
                const id = e.target.value;
                setCompareTarget(id);
                if (id && !selectedCompareIds.includes(id)) {
                  onCompareIdsChange([...selectedCompareIds, id]);
                }
              }}
              className="rounded border border-[var(--border)] bg-[var(--surface-1)] px-2 py-1 text-xs"
            >
              <option value="">Compare with…</option>
              {comparisonSubjects.map((subj) => (
                <option key={subj.id} value={subj.id}>
                  {subj.label}
                </option>
              ))}
            </select>
          </div>
        )}

        <Button
          variant="ghost"
          data-testid="toggle-advanced-syntax"
          onClick={() => setShowAdvanced((v) => !v)}
        >
          {showAdvanced ? "Hide advanced syntax" : "Advanced syntax"}
        </Button>

        {shareUrl && (
          <Button
            variant="secondary"
            data-testid="share-scoped-view-button"
            onClick={handleShare}
          >
            {sharedCopied ? "Shared link copied" : "Share scoped view"}
          </Button>
        )}
      </div>

      {sharedCopied && shareUrl && (
        <div
          data-testid="shared-view-link-banner"
          className="rounded border border-[var(--border)] bg-[var(--surface-1)] px-2.5 py-1 text-xs"
        >
          Scoped view URL: <code className="font-mono">{shareUrl}</code>
        </div>
      )}

      {showAdvanced && onAdvancedQueryApply && (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (advancedExpr.trim()) onAdvancedQueryApply(advancedExpr.trim());
          }}
          data-testid="advanced-syntax-form"
          className="flex items-center gap-2"
        >
          <input
            type="text"
            aria-label="Advanced query expression"
            placeholder="e.g. duration_ms > 500 AND http.status_code >= 500"
            value={advancedExpr}
            onChange={(e) => setAdvancedExpr(e.target.value)}
            className="flex-1 rounded border border-[var(--border)] bg-[var(--surface-1)] px-2 py-1 font-mono text-xs"
          />
          <Button type="submit" variant="primary">
            Apply syntax
          </Button>
        </form>
      )}

      {refinements.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5 text-xs" data-testid="incremental-filter-chips">
          <span className="text-[var(--muted)]">Refinements:</span>
          {refinements.map((r, idx) => (
            <Badge key={`${r.field}-${r.value}-${idx}`} tone="info">
              {r.field}={r.value}
            </Badge>
          ))}
          <Button
            variant="ghost"
            data-testid="undo-refinement-button"
            onClick={onUndoRefinement}
          >
            Undo last refinement
          </Button>
        </div>
      )}

      {anomalyCandidate && (
        <Panel>
          <div
            data-testid="anomaly-callout"
            className="flex flex-wrap items-center justify-between gap-2 text-xs"
          >
            <div className="flex flex-wrap items-center gap-2">
              <Badge tone="warn">Candidate anomaly (unconfirmed)</Badge>
              <strong className="text-[var(--text)]">{anomalyCandidate.metric}</strong>
              <span>{anomalyCandidate.deltaSummary}</span>
              <span className="text-[var(--muted)]">({anomalyCandidate.comparisonPeriod})</span>
            </div>
            <span className="text-[var(--muted)]">
              Uncertainty: {anomalyCandidate.uncertaintyLabel} · Statistical suggestion, not a confirmed incident
            </span>
          </div>
        </Panel>
      )}

      {activeComparisons.length > 0 && (
        <Panel>
          <div data-testid="aligned-comparison-panel" className="space-y-2 text-xs">
            <div className="flex items-center justify-between">
              <strong className="uppercase text-[var(--muted)]">
                Aligned Comparison ({comparisonWindowLabel})
              </strong>
              {onCompareIdsChange && (
                <button
                  type="button"
                  onClick={() => onCompareIdsChange([])}
                  className="text-[var(--brand)] hover:underline"
                >
                  Clear comparison
                </button>
              )}
            </div>
            <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
              {activeComparisons.map((subj) => (
                <div
                  key={subj.id}
                  className="rounded border border-[var(--border)] bg-[var(--surface-1)] p-2"
                >
                  <div className="font-semibold text-[var(--text)]">{subj.label}</div>
                  <div className="mt-1 flex gap-3 text-[var(--muted)]">
                    <span>Errors: {subj.errorRatePercent.toFixed(1)}%</span>
                    <span>P95: {Math.round(subj.p95LatencyMs)}ms</span>
                    <span>Rate: {subj.requestRateRps.toFixed(1)} rps</span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </Panel>
      )}
    </div>
  );
}
