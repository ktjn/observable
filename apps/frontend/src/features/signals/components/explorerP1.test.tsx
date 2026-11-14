import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { useState } from "react";
import { FacetSidebar } from "../../../components/FacetSidebar";
import {
  ExplorerControlsBar,
  buildSharedViewUrl,
  type ExplorerRefinement,
} from "./ExplorerControlsBar";

function ExplorerHarness() {
  const [visualMode, setVisualMode] = useState<"all" | "failures" | "latency" | "usage">("all");
  const [refinements, setRefinements] = useState<ExplorerRefinement[]>([
    { field: "service", value: "checkout" },
    { field: "version", value: "v2.4.1" },
  ]);
  const [groupBy, setGroupBy] = useState("");
  const [compareIds, setCompareIds] = useState<string[]>([]);
  const [appliedSyntax, setAppliedSyntax] = useState<string | null>(null);

  const shareUrl = buildSharedViewUrl("/traces", refinements, "15m", "checkout");

  return (
    <div>
      <ExplorerControlsBar
        visualMode={visualMode}
        onVisualModeChange={(m) => {
          setVisualMode(m);
          if (m === "failures") {
            setRefinements((prev) => [...prev, { field: "status", value: "error" }]);
          }
        }}
        refinements={refinements}
        onUndoRefinement={() => setRefinements((prev) => prev.slice(0, -1))}
        groupBy={groupBy}
        onGroupByChange={setGroupBy}
        comparisonSubjects={[
          { id: "checkout-prev", label: "checkout (previous release)", errorRatePercent: 0.4, p95LatencyMs: 180, requestRateRps: 115 },
          { id: "payment", label: "payment (incident window)", errorRatePercent: 6.2, p95LatencyMs: 740, requestRateRps: 98 },
          { id: "checkout-curr", label: "checkout (current release)", errorRatePercent: 4.8, p95LatencyMs: 610, requestRateRps: 120 },
        ]}
        selectedCompareIds={compareIds}
        onCompareIdsChange={setCompareIds}
        comparisonWindowLabel="Incident window 12:00–12:15 UTC"
        anomalyCandidate={{
          metric: "checkout.p95_latency",
          deltaSummary: "+238% above baseline",
          comparisonPeriod: "vs previous 1h baseline",
          uncertaintyLabel: "±12% confidence interval",
        }}
        shareUrl={shareUrl}
        onAdvancedQueryApply={setAppliedSyntax}
      />
      <FacetSidebar
        facets={{ service: [{ value: "checkout", count: 42 }] }}
        onFacetClick={(field, value) => setRefinements((prev) => [...prev, { field, value }])}
        onCustomAttributeFilter={(field, value) =>
          setRefinements((prev) => [...prev, { field, value }])
        }
      />
      <div data-testid="harness-state">
        mode={visualMode};count={refinements.length};group={groupBy};syntax={appliedSyntax ?? "none"}
      </div>
    </div>
  );
}

describe("Telemetry Exploration and Filtering P1 (OBS-22)", () => {
  test("obs_22_04_visual_exploration_isolates_failures_without_query_syntax", () => {
    render(<ExplorerHarness />);
    fireEvent.click(screen.getByTestId("visual-mode-failures"));
    expect(screen.getByTestId("harness-state")).toHaveTextContent("mode=failures");
    expect(screen.getByTestId("incremental-filter-chips")).toHaveTextContent("status=error");
  });

  test("obs_22_06_and_22_09_side_by_side_comparison_of_releases_and_services", () => {
    render(<ExplorerHarness />);
    const select = screen.getByLabelText("Compare target");
    fireEvent.change(select, { target: { value: "checkout-prev" } });
    fireEvent.change(select, { target: { value: "payment" } });

    const panel = screen.getByTestId("aligned-comparison-panel");
    expect(panel).toHaveTextContent("Incident window 12:00–12:15 UTC");
    expect(panel).toHaveTextContent("checkout (previous release)");
    expect(panel).toHaveTextContent("payment (incident window)");
  });

  test("obs_22_07_advanced_syntax_refines_beyond_predefined_filters", () => {
    render(<ExplorerHarness />);
    fireEvent.click(screen.getByTestId("toggle-advanced-syntax"));
    const input = screen.getByLabelText("Advanced query expression");
    fireEvent.change(input, { target: { value: "duration_ms > 500" } });
    fireEvent.submit(screen.getByTestId("advanced-syntax-form"));
    expect(screen.getByTestId("harness-state")).toHaveTextContent("syntax=duration_ms > 500");
  });

  test("obs_22_08_share_scoped_view_preserves_filters_and_period", () => {
    Object.assign(navigator, {
      clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
    });
    render(<ExplorerHarness />);
    fireEvent.click(screen.getByTestId("share-scoped-view-button"));
    const banner = screen.getByTestId("shared-view-link-banner");
    expect(banner).toHaveTextContent("/traces?service=checkout&preset=15m");
    expect(banner).toHaveTextContent("filter=service%3Acheckout");
  });

  test("obs_22_10_filters_any_authorized_recorded_attribute_and_blocks_restricted", () => {
    render(<ExplorerHarness />);
    const fieldInput = screen.getByLabelText("Attribute name");
    const valueInput = screen.getByLabelText("Attribute value");

    // Restricted attribute blocked
    fireEvent.change(fieldInput, { target: { value: "token" } });
    fireEvent.change(valueInput, { target: { value: "abc" } });
    fireEvent.submit(screen.getByTestId("custom-attribute-filter"));
    expect(screen.getByRole("alert")).toHaveTextContent("restricted by authorization policy");

    // Rare non-predefined attribute succeeds
    fireEvent.change(fieldInput, { target: { value: "build_sha" } });
    fireEvent.change(valueInput, { target: { value: "canary-99" } });
    fireEvent.submit(screen.getByTestId("custom-attribute-filter"));
    expect(screen.getByTestId("incremental-filter-chips")).toHaveTextContent("build_sha=canary-99");
  });

  test("obs_22_12_incremental_refinements_and_undo_last_refinement", () => {
    render(<ExplorerHarness />);
    fireEvent.change(screen.getByTestId("explorer-group-by"), { target: { value: "location" } });
    expect(screen.getByTestId("harness-state")).toHaveTextContent("group=location;syntax=none");
    expect(screen.getByTestId("incremental-filter-chips")).toHaveTextContent("version=v2.4.1");

    fireEvent.click(screen.getByTestId("undo-refinement-button"));
    expect(screen.getByTestId("incremental-filter-chips")).not.toHaveTextContent("version=v2.4.1");
    expect(screen.getByTestId("incremental-filter-chips")).toHaveTextContent("service=checkout");
  });

  test("obs_22_13_anomaly_callout_shows_candidate_period_and_uncertainty", () => {
    render(<ExplorerHarness />);
    const callout = screen.getByTestId("anomaly-callout");
    expect(callout).toHaveTextContent("Candidate anomaly (unconfirmed)");
    expect(callout).toHaveTextContent("vs previous 1h baseline");
    expect(callout).toHaveTextContent("±12% confidence interval");
    expect(callout).toHaveTextContent("not a confirmed incident");
  });
});
