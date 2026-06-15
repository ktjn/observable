import { render, screen } from "@testing-library/react";
import { describe, test, expect } from "vitest";
import { VisualizationPanel } from "./VisualizationPanel";
import type { VisualizationFrame } from "../../api/nlq";
import { TimeDisplayProvider } from "../../lib/timeDisplay";

function Wrapper({ children }: { children: React.ReactNode }) {
  return <TimeDisplayProvider>{children}</TimeDisplayProvider>;
}

function renderViz(frame: VisualizationFrame) {
  return render(<VisualizationPanel frame={frame} />, { wrapper: Wrapper });
}

function baseFrame(overrides: Partial<VisualizationFrame> = {}): VisualizationFrame {
  return {
    frame_type: "table",
    x_field: null,
    y_field: "value",
    series_field: null,
    unit: null,
    suggested_visualization: "table",
    field_roles: [],
    data: [],
    nlq_ir: {
      operation: "table",
      signals: ["metrics"],
      filters: [],
      group_by: [],
      time_range: { from: "now-1h", to: "now" },
      metric: null,
      window: null,
      resolution: null,
      visualization_hint: null,
    },
    source_sql: "SELECT ...",
    time_range: { from: "now-1h", to: "now" },
    signal_types: ["metrics"],
    sample_rate: null,
    approximation_statement: "Advisory",
    ...overrides,
  };
}

describe("VisualizationPanel", () => {
  test("renders empty state when data is empty", () => {
    renderViz(baseFrame({ data: [] }));
    expect(screen.getByTestId("viz-empty")).toBeInTheDocument();
  });

  test("renders timeseries table with correct frame_type attribute", () => {
    const frame = baseFrame({
      frame_type: "timeseries",
      x_field: "bucket",
      y_field: "value",
      data: [{ bucket: "2026-01-01 10:00", value: 120.5 }],
    });
    renderViz(frame);
    expect(screen.getByTestId("timeseries-table")).toBeInTheDocument();
    expect(screen.getByTestId("viz-panel")).toHaveAttribute(
      "data-frame-type",
      "timeseries"
    );
  });

  test("timeseries table shows x_field column header", () => {
    const frame = baseFrame({
      frame_type: "timeseries",
      x_field: "bucket",
      y_field: "avg_latency",
      data: [{ bucket: "10:00", avg_latency: 42.1 }],
    });
    renderViz(frame);
    expect(screen.getByText("Time bucket")).toBeInTheDocument();
    expect(screen.getByText("42.100")).toBeInTheDocument();
  });

  test("timeseries table includes series_field column when present", () => {
    const frame = baseFrame({
      frame_type: "timeseries",
      x_field: "bucket",
      y_field: "value",
      series_field: "service_name",
      data: [{ bucket: "10:00", value: 55.0, service_name: "checkout" }],
    });
    renderViz(frame);
    expect(screen.getByText("service_name")).toBeInTheDocument();
    expect(screen.getByText("checkout")).toBeInTheDocument();
  });

  test("renders histogram table with bar distribution column", () => {
    const frame = baseFrame({
      frame_type: "histogram",
      x_field: "bound",
      y_field: "count",
      data: [
        { bound: 10, count: 5 },
        { bound: 50, count: 20 },
      ],
    });
    renderViz(frame);
    expect(screen.getByTestId("histogram-table")).toBeInTheDocument();
    expect(screen.getByText("Distribution")).toBeInTheDocument();
  });

  test("renders topk table with rank column", () => {
    const frame = baseFrame({
      frame_type: "topk",
      x_field: "service_name",
      y_field: "avg_value",
      data: [
        { service_name: "checkout", avg_value: 200 },
        { service_name: "payment", avg_value: 150 },
      ],
    });
    renderViz(frame);
    expect(screen.getByTestId("topk-table")).toBeInTheDocument();
    expect(screen.getByText("#1")).toBeInTheDocument();
    expect(screen.getByText("#2")).toBeInTheDocument();
  });

  test("renders distribution table with all returned stats", () => {
    const frame = baseFrame({
      frame_type: "distribution",
      data: [{ p50: 10, p90: 50, p95: 80, p99: 200, min_val: 1, max_val: 500 }],
    });
    renderViz(frame);
    expect(screen.getByTestId("distribution-table")).toBeInTheDocument();
    expect(screen.getByText("p50 (median)")).toBeInTheDocument();
    expect(screen.getByText("p99")).toBeInTheDocument();
    // Legacy aliases mapped to "min" / "max".
    expect(screen.getAllByText("min").length).toBeGreaterThan(0);
    expect(screen.getAllByText("max").length).toBeGreaterThan(0);
  });

  test("renders distribution table with only requested stats (data-driven)", () => {
    const frame = baseFrame({
      frame_type: "distribution",
      data: [{ p99: 200, average: 45, median: 30 }],
    });
    renderViz(frame);
    expect(screen.getByTestId("distribution-table")).toBeInTheDocument();
    expect(screen.getByText("p99")).toBeInTheDocument();
    expect(screen.getByText("average")).toBeInTheDocument();
    expect(screen.getByText("median")).toBeInTheDocument();
    // Stats not in data must not appear.
    expect(screen.queryByText("p50 (median)")).not.toBeInTheDocument();
    expect(screen.queryByText("p90")).not.toBeInTheDocument();
  });

  test("renders generic table for table frame type", () => {
    const frame = baseFrame({
      frame_type: "table",
      data: [{ ts: "2026-01-01", metric_name: "cpu_usage", value: 0.75 }],
    });
    renderViz(frame);
    expect(screen.getByTestId("generic-table")).toBeInTheDocument();
    // metric_name maps to display label "Metric"
    expect(screen.getByText("Metric")).toBeInTheDocument();
  });

  test("falls back to generic table for unknown frame types", () => {
    const frame = baseFrame({
      frame_type: "heatmap" as VisualizationFrame["frame_type"],
      data: [{ x: 1, y: 2 }],
    });
    renderViz(frame);
    expect(screen.getByTestId("generic-table")).toBeInTheDocument();
  });

  test("formats integer values without decimal places", () => {
    const frame = baseFrame({
      frame_type: "timeseries",
      x_field: "bucket",
      y_field: "value",
      data: [{ bucket: "10:00", value: 42 }],
    });
    renderViz(frame);
    expect(screen.getByText("42")).toBeInTheDocument();
  });

  test("formats float values to 3 decimal places", () => {
    const frame = baseFrame({
      frame_type: "timeseries",
      x_field: "bucket",
      y_field: "value",
      data: [{ bucket: "10:00", value: 3.14159 }],
    });
    renderViz(frame);
    expect(screen.getByText("3.142")).toBeInTheDocument();
  });

  test("renders em-dash for null/undefined values", () => {
    const frame = baseFrame({
      frame_type: "timeseries",
      x_field: "bucket",
      y_field: "value",
      data: [{ bucket: null, value: undefined }],
    });
    renderViz(frame);
    // Both bucket and value should show em-dash
    const dashes = screen.getAllByText("—");
    expect(dashes.length).toBeGreaterThanOrEqual(2);
  });

  test("distribution table shows p95 + median + average (regression: all three stats)", () => {
    const frame = baseFrame({
      frame_type: "distribution",
      data: [{ p95: 4.237, median: 3.1, average: 3.5 }],
    });
    renderViz(frame);
    expect(screen.getByTestId("distribution-table")).toBeInTheDocument();
    expect(screen.getByText("p95")).toBeInTheDocument();
    expect(screen.getByText("median")).toBeInTheDocument();
    expect(screen.getByText("average")).toBeInTheDocument();
    expect(screen.getByText("4.237")).toBeInTheDocument();
    expect(screen.getByText("3.100")).toBeInTheDocument();
    expect(screen.getByText("3.500")).toBeInTheDocument();
  });

  test("formatValue renders em-dash for NaN number", () => {
    const frame = baseFrame({
      frame_type: "distribution",
      data: [{ p95: NaN }],
    });
    renderViz(frame);
    expect(screen.getByText("—")).toBeInTheDocument();
    expect(screen.queryByText("NaN")).not.toBeInTheDocument();
  });

  test("generic table pins timestamp column first and shows display label 'Occurred Time'", () => {
    const frame = baseFrame({
      frame_type: "table",
      // Backend returns body before timestamp — column order must be fixed
      data: [{ body: "hello", timestamp_unix_nano: "1700000000000000000", service_name: "svc" }],
    });
    renderViz(frame);
    const headers = screen.getAllByRole("columnheader");
    expect(headers[0].textContent).toBe("Occurred Time");
    expect(headers[1].textContent).toBe("Message");
    expect(headers[2].textContent).toBe("Service");
  });

  test("generic table formats timestamp_unix_nano using the time format context", () => {
    const frame = baseFrame({
      frame_type: "table",
      data: [{ timestamp_unix_nano: "1700000000000000000" }],
    });
    renderViz(frame);
    // Default format is iso-local-ms; should NOT render raw nanosecond string
    expect(screen.queryByText("1700000000000000000")).not.toBeInTheDocument();
    // Should render a formatted timestamp (contains at least year and colons)
    const cell = screen.getByRole("cell");
    expect(cell.textContent).toMatch(/\d{4}-\d{2}-\d{2}/);
  });
});
