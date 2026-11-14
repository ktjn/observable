import { describe, expect, test, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { TimeSeriesGraph } from "./time-series-graph";
import { DataFreshness } from "./data-freshness";
import { ConfirmDialog } from "./confirm-dialog";
import {
  SHARED_TERMINOLOGY,
  usePreservedDraft,
  SloPlainExplanation,
  FeedbackControl,
  ExportHandoverButton,
} from "./interaction-governance";
import { TimeDisplayProvider, useTimeDisplay } from "../../lib/timeDisplay";
import { formatTimestamp } from "../../utils/formatTimestamp";

function DraftHarness() {
  const { value, setValue, outcome, markSaved, markPendingReauth } = usePreservedDraft(
    "incident.note.inc-42",
  );
  return (
    <div>
      <textarea
        aria-label="Incident note"
        value={value}
        onChange={(e) => setValue(e.target.value)}
      />
      <button type="button" onClick={markPendingReauth}>
        Simulate expiry
      </button>
      <button type="button" onClick={markSaved}>
        Confirm save
      </button>
      <span data-testid="draft-outcome">{outcome}</span>
    </div>
  );
}

function TimePrefHarness() {
  const { format, setFormat, density, setDensity } = useTimeDisplay();
  const sampleNs = "1700000000000000000";
  return (
    <div>
      <div data-testid="pref-state">
        {format}|{density}|{formatTimestamp(sampleNs, format)}
      </div>
      <button type="button" onClick={() => setFormat("iso-utc-ms")}>
        Use UTC
      </button>
      <button type="button" onClick={() => setDensity("compact")}>
        Use compact
      </button>
    </div>
  );
}

describe("Interaction States, Accessibility, and Access-Aware UI P1", () => {
  beforeEach(() => {
    sessionStorage.clear();
    localStorage.clear();
  });

  test("obs_24_06_chart_provides_accessible_tabular_alternative", () => {
    render(
      <TimeSeriesGraph
        title="Checkout Latency"
        rangeStartMs={1000}
        rangeEndMs={3000}
        series={[
          {
            key: "p95",
            label: "P95 Latency",
            color: "#38bdf8",
            unit: "ms",
            aggregation: "p95",
            points: [
              { timestampMs: 1000, value: 120 },
              { timestampMs: 2000, value: 480 },
            ],
          },
        ]}
      />,
    );
    const table = screen.getByTestId("time-series-accessible-table");
    expect(table).toHaveAttribute("aria-label", "Checkout Latency accessible summary");
    expect(table).toHaveTextContent("P95 Latency (p95 · ms)");
    expect(table).toHaveTextContent("480.0");
  });

  test("obs_24_07_consistent_terminology_across_views", () => {
    expect(SHARED_TERMINOLOGY.acknowledge).toBe("Acknowledge");
    expect(SHARED_TERMINOLOGY.escalate).toBe("Escalate");
    expect(SHARED_TERMINOLOGY.resolve).toBe("Resolve");
    expect(SHARED_TERMINOLOGY.undoFilter).toBe("Undo last refinement");
  });

  test("obs_24_08_and_24_11_freshness_auto_refresh_and_deferred_update_notice", () => {
    const onApply = vi.fn();
    render(
      <DataFreshness
        dataUpdatedAt={Date.now()}
        viewMode="live"
        autoRefresh={false}
        hasPendingUpdate
        onApplyPendingUpdate={onApply}
      />,
    );
    expect(screen.getByTestId("auto-refresh-indicator")).toHaveTextContent("Auto-refresh paused");
    const notice = screen.getByTestId("pending-update-notice");
    expect(notice).toHaveTextContent("New data available");
    fireEvent.click(screen.getByRole("button", { name: "Apply update" }));
    expect(onApply).toHaveBeenCalledTimes(1);
  });

  test("obs_24_10_confirm_dialog_explains_non_reversible_consequences", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(
      <ConfirmDialog
        open
        title="Delete alert rule"
        description="Remove rule checkout-p95?"
        consequences="This action is permanent and cannot be undone."
        onConfirm={onConfirm}
        onCancel={onCancel}
      />,
    );
    expect(screen.getByRole("alertdialog")).toHaveTextContent(
      "This action is permanent and cannot be undone.",
    );
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  test("obs_24_12_preserves_unsent_input_across_session_expiry_and_reports_outcome", () => {
    const { unmount } = render(<DraftHarness />);
    fireEvent.change(screen.getByLabelText("Incident note"), {
      target: { value: "Mitigated by scaling checkout replicas" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Simulate expiry" }));
    expect(screen.getByTestId("draft-outcome")).toHaveTextContent("pending_reauth");

    // Simulate re-authentication / remount
    unmount();
    render(<DraftHarness />);
    expect(screen.getByLabelText("Incident note")).toHaveValue(
      "Mitigated by scaling checkout replicas",
    );
    fireEvent.click(screen.getByRole("button", { name: "Confirm save" }));
    expect(screen.getByTestId("draft-outcome")).toHaveTextContent("saved");
  });

  test("obs_25_04_user_adjusts_time_display_and_density_locally", () => {
    render(
      <TimeDisplayProvider>
        <TimePrefHarness />
      </TimeDisplayProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Use UTC" }));
    fireEvent.click(screen.getByRole("button", { name: "Use compact" }));
    expect(screen.getByTestId("pref-state")).toHaveTextContent("iso-utc-ms|compact|2023-11-14 22:13:20.000Z");
  });

  test("obs_25_05_plain_language_slo_explanation_for_business_users", () => {
    render(<SloPlainExplanation attainmentPercent={99.2} targetPercent={99.9} />);
    const el = screen.getByTestId("slo-plain-explanation");
    expect(el).toHaveTextContent("Commitment at risk");
    expect(el).toHaveTextContent("99.20% achieved vs 99.90% target");
    expect(el).toHaveTextContent("agreed reliability goal allows");
  });

  test("obs_25_06_feedback_control_captures_view_and_context", () => {
    const onSubmit = vi.fn();
    render(
      <FeedbackControl
        route="/services/checkout"
        service="checkout"
        environment="prod"
        periodLabel="Last 15m"
        onSubmit={onSubmit}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Report display issue" }));
    fireEvent.change(screen.getByLabelText("Issue description"), {
      target: { value: "Latency chart unit unclear" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Submit feedback" }));
    expect(onSubmit).toHaveBeenCalledWith({
      route: "/services/checkout",
      service: "checkout",
      environment: "prod",
      periodLabel: "Last 15m",
      description: "Latency chart unit unclear",
    });
  });

  test("obs_25_07_exports_accessible_handover_report_with_units_and_context", () => {
    render(
      <ExportHandoverButton
        reportInput={{
          title: "Checkout Shift Handover",
          service: "checkout",
          environment: "prod",
          periodLabel: "Last 1h",
          filters: ["status=error"],
          metrics: [{ label: "P95 Latency", value: "420", unit: "ms" }],
        }}
      />,
    );
    fireEvent.click(screen.getByTestId("export-handover-button"));
    const preview = screen.getByTestId("handover-report-preview");
    expect(preview).toHaveTextContent("# Checkout Shift Handover");
    expect(preview).toHaveTextContent("Environment: prod");
    expect(preview).toHaveTextContent("Time context: Last 1h");
    expect(preview).toHaveTextContent("Active filters: status=error");
    expect(preview).toHaveTextContent("P95 Latency: 420 (ms)");
  });

  test("obs_25_10_representative_incident_workflow_completes_within_targets", () => {
    const onConfirm = vi.fn();
    render(
      <div>
        <SloPlainExplanation attainmentPercent={98.5} targetPercent={99.9} />
        <ConfirmDialog
          open
          title="Escalate incident INC-42"
          description="Page secondary on-call?"
          confirmLabel={SHARED_TERMINOLOGY.escalate}
          onConfirm={onConfirm}
          onCancel={vi.fn()}
        />
        <ExportHandoverButton
          reportInput={{
            title: "INC-42 Handover",
            service: "checkout",
            environment: "prod",
            periodLabel: "Last 15m",
            filters: [],
            metrics: [{ label: "Error Rate", value: "5.2", unit: "%" }],
          }}
        />
      </div>,
    );
    expect(screen.getByTestId("slo-plain-explanation")).toHaveTextContent("Commitment at risk");
    fireEvent.click(screen.getByRole("button", { name: "Escalate" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByTestId("export-handover-button"));
    expect(screen.getByTestId("handover-report-preview")).toHaveTextContent("Error Rate: 5.2 (%)");
  });
});
