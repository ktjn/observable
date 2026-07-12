import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, expect, test, vi } from "vitest";
import { TimeDisplayProvider } from "../lib/timeDisplay";
import { TenantContextProvider } from "../hooks/useTenantContext";
import type { TraceResponse } from "../api/traces";
import TraceSearch, { TraceContextSidebar } from "./TraceSearch";

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

test("toggles trace fields as table columns from the context panel", async () => {
  renderTraceSearch();
  const table = await screen.findByRole("table", { name: "Trace results" });
  fireEvent.click(screen.getByText("GET /checkout"));
  const sidebar = screen.getByRole("complementary", { name: "Selected trace context" });

  fireEvent.click(within(sidebar).getByRole("button", { name: "Remove operation column" }));
  expect(within(table).queryByRole("columnheader", { name: "Operation" })).not.toBeInTheDocument();
  expect(within(table).queryByText("GET /checkout")).not.toBeInTheDocument();
  fireEvent.click(within(sidebar).getByRole("button", { name: "Add operation as a column" }));
  expect(within(table).getByRole("columnheader", { name: "Operation" })).toBeInTheDocument();
  expect(within(table).getByText("GET /checkout")).toBeInTheDocument();

  fireEvent.click(within(sidebar).getByRole("button", { name: "Remove service.name column" }));
  expect(within(table).queryByRole("columnheader", { name: "service.name" })).not.toBeInTheDocument();
  fireEvent.click(within(sidebar).getByRole("button", { name: "Add service.name as a column" }));
  expect(within(table).getByRole("columnheader", { name: "service.name" })).toBeInTheDocument();

  fireEvent.click(within(sidebar).getByRole("button", { name: "Remove status column" }));
  expect(within(table).queryByRole("columnheader", { name: "Status" })).not.toBeInTheDocument();
  expect(within(sidebar).getByRole("status")).toHaveTextContent("OK");
});

test("removing trace_id still allows row selection and full-trace navigation", async () => {
  renderTraceSearch();
  await screen.findByRole("table", { name: "Trace results" });
  fireEvent.click(screen.getByText("GET /checkout"));
  let sidebar = screen.getByRole("complementary", { name: "Selected trace context" });
  fireEvent.click(within(sidebar).getByRole("button", { name: "Remove trace_id column" }));
  expect(screen.queryByRole("columnheader", { name: "Trace ID" })).not.toBeInTheDocument();
  fireEvent.click(within(sidebar).getByRole("button", { name: "Close" }));
  fireEvent.click(screen.getByText("GET /checkout"));
  sidebar = screen.getByRole("complementary", { name: "Selected trace context" });
  expect(within(sidebar).getByRole("link", { name: "View Full Trace Explorer" })).toHaveAttribute(
    "href", "/traces/trace-abc-1234567890",
  );
});

test("toggles an arbitrary resolvable trace attribute from the context sidebar", () => {
  const trace: TraceResponse = {
    trace_id: "trace-attribute", events: [], spans: [{
      tenant_id: "tenant", trace_id: "trace-attribute", span_id: "span", service_name: "checkout",
      service_namespace: "", service_version: "", operation_name: "GET /checkout", span_kind: "SERVER",
      start_time_unix_nano: 1, end_time_unix_nano: 2, duration_ns: 1, status_code: "OK",
      status_message: "", attributes: { "http.route": "/checkout" },
      resource_attributes: { "k8s.pod.name": "checkout-7f9" }, environment: "", host_id: "",
      workload: "", deployment_id: "",
    }],
  };
  const onToggleColumn = vi.fn();
  render(<TimeDisplayProvider><TraceContextSidebar trace={trace} onClose={vi.fn()}
    visibleColumns={[]} onToggleColumn={onToggleColumn} /></TimeDisplayProvider>);
  const sidebar = screen.getByRole("complementary", { name: "Selected trace context" });
  fireEvent.click(within(sidebar).getByRole("button", { name: "Add span.http.route as a column" }));
  expect(onToggleColumn).toHaveBeenCalledWith("span.http.route");
  expect(within(sidebar).getByText("/checkout")).toBeInTheDocument();
  expect(within(sidebar).getByRole("button", { name: "Add resource.k8s.pod.name as a column" })).toBeEnabled();
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
