import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, expect, test, vi } from "vitest";
import { TimeDisplayProvider } from "../lib/timeDisplay";
import { TenantContextProvider } from "../hooks/useTenantContext";
import TraceSearch from "./TraceSearch";

const FIXED_TO_MS   = 1_700_100_000_000;
const FIXED_FROM_MS = FIXED_TO_MS - 60 * 60 * 1000;

vi.mock("../hooks/useGlobalDateRange", () => ({
  useGlobalDateRange: vi.fn(() => ({
    preset: "1h",
    fromMs: FIXED_FROM_MS,
    toMs:   FIXED_TO_MS,
    setPreset: vi.fn(),
    setCustomRange: vi.fn(),
    clearCustomRange: vi.fn(),
  })),
}));

/** NLQ trace row returned by execute_trace_query */
const nlqTraceRows = [
  {
    trace_id: "trace-abc-1234567890",
    root_service: "checkout",
    root_operation: "GET /checkout",
    duration_ms: 5,
    status_code: "OK",
    environment: "prod",
    start_time_unix_nano: 1,
  },
];

vi.mock("../api/nlq", () => ({
  submitNlqQuery: vi.fn(async () => ({
    type: "frame",
    frame: {
      frame_type: "table",
      x_field: null,
      y_field: null,
      series_field: null,
      unit: null,
      suggested_visualization: "table",
      field_roles: [],
      data: nlqTraceRows,
      nlq_ir: {},
      source_sql: "",
      time_range: { from: "now-1h", to: "now" },
      signal_types: ["traces"],
      sample_rate: null,
      approximation_statement: "",
    },
  })),
}));

vi.mock("../api/traces", () => ({
  fetchTraceHistogram: vi.fn(async () => ({ buckets: [] })),
}));

vi.mock("../api/dashboards", () => ({
  createDashboard: vi.fn(async () => ({
    dashboard_id: "dash-1",
    name: "Promoted trace query",
    panels: [],
    created_at: "2026-04-30T00:00:00Z",
  })),
}));

vi.mock("@tanstack/react-router", () => ({
  Link: ({
    to,
    params,
    children,
  }: {
    to: string;
    params?: Record<string, string>;
    children: ReactNode;
  }) => {
    let href = to;
    if (params) {
        for (const [k, v] of Object.entries(params)) {
            href = href.replace(`$${k}`, v);
        }
    }
    return <a href={href}>{children}</a>;
  },
  useNavigate: () => vi.fn(),
  useSearch: () => ({}),
}));

const { fetchTraceHistogram } = await import("../api/traces");

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getVirtualItems: () =>
      Array.from({ length: count }, (_, i) => ({
        key: i,
        index: i,
        start: i * 40,
        end: (i + 1) * 40,
      })),
    getTotalSize: () => count * 40,
    measureElement: (_el: Element | null) => {},
  }),
}));

beforeEach(() => {
  vi.clearAllMocks();
  window.history.pushState({}, "", "/traces");
});

function renderTraceSearch() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  return render(
    <TenantContextProvider>
      <QueryClientProvider client={client}>
        <TimeDisplayProvider>
          <TraceSearch />
        </TimeDisplayProvider>
      </QueryClientProvider>
    </TenantContextProvider>,
  );
}

test("queries traces via NLQ execute on load", async () => {
  const { submitNlqQuery } = await import("../api/nlq");
  renderTraceSearch();

  await waitFor(() =>
    expect(submitNlqQuery).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ mode: "execute", base_ir: expect.objectContaining({ signals: ["traces"] }) }),
    ),
  );
});

test("renders the trace explorer shell and named results table", async () => {
  renderTraceSearch();

  await waitFor(() => expect(screen.getByText("Explorer")).toBeInTheDocument());
  expect(screen.getByRole("heading", { name: "Traces" })).toBeInTheDocument();

  const table = await screen.findByRole("table", { name: "Trace results" });
  expect(within(table).getByRole("columnheader", { name: "Trace ID" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "Duration" })).toBeInTheDocument();
  expect(within(table).getByText("GET /checkout")).toBeInTheDocument();
});

test("renders a visible trace histogram from search results when histogram buckets are empty", async () => {
  renderTraceSearch();

  const histogram = await screen.findByRole("group", { name: "Trace volume histogram" });
  const traceBar = histogram.querySelector("[title*='Traces: 1']");

  expect(traceBar).toBeInTheDocument();
});

test("selecting a trace opens context sidebar that scrolls internally", async () => {
  renderTraceSearch();

  await screen.findByRole("table", { name: "Trace results" });
  // The trace-id link stops propagation (it navigates to the full trace
  // page), so click another cell in the row to exercise row selection.
  fireEvent.click(screen.getByText("GET /checkout"));

  const sidebar = screen.getByRole("complementary", { name: "Selected trace context" });
  expect(within(sidebar).getByText("Root Span Details")).toBeInTheDocument();
  expect(sidebar).toHaveClass("overflow-y-auto");
});

test("keeps a visible trace histogram when the histogram query fails", async () => {
  vi.mocked(fetchTraceHistogram).mockRejectedValueOnce(new Error("histogram backend error"));

  renderTraceSearch();

  const histogram = await screen.findByRole("group", { name: "Trace volume histogram" });
  const traceBar = histogram.querySelector("[title*='Traces: 1']");

  expect(traceBar).toBeInTheDocument();
});

test("renders the trace summary stat-card row", async () => {
  renderTraceSearch();

  const summary = await screen.findByLabelText("Trace summary");
  expect(summary).toBeInTheDocument();
});
