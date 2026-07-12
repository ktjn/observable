import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, expect, test, vi } from "vitest";
import type { LogRecord } from "../api/logs";
import { TimeDisplayProvider } from "../lib/timeDisplay";
import { TenantContextProvider } from "../hooks/useTenantContext";
import LogSearch, {
  buildLogHistogram,
  formatLogMessage,
  otelSeverity,
} from "./LogSearch";

const mockSetCustomRange = vi.fn();

const FIXED_TO_MS   = 1_700_100_000_000;
const FIXED_FROM_MS = FIXED_TO_MS - 60 * 60 * 1000;

vi.mock("../hooks/useGlobalDateRange", () => ({
  useGlobalDateRange: vi.fn(() => ({
    preset: "1h",
    fromMs: FIXED_FROM_MS,
    toMs:   FIXED_TO_MS,
    setPreset: vi.fn(),
    setCustomRange: mockSetCustomRange,
    clearCustomRange: vi.fn(),
  })),
}));

const logs: LogRecord[] = [
  {
    tenant_id: "00000000-0000-0000-0000-000000000001",
    log_id: "log-1",
    timestamp_unix_nano: 1700000000000000000,
    observed_timestamp_unix_nano: 1700000000001000000,
    severity_number: 9,
    severity_text: "INFO",
    body: { message: "checkout completed", ignored: "not primary" },
    trace_id: "trace-1",
    span_id: "span-1",
    service_name: "checkout",
    environment: "prod",
    host_id: "node-1",
    fingerprint: 12345,
    attributes: { "http.route": "/checkout" },
    resource_attributes: { "host.name": "node-1", "k8s.pod.name": "checkout-7f9" },
  },
  {
    tenant_id: "00000000-0000-0000-0000-000000000001",
    log_id: "log-2",
    timestamp_unix_nano: 1700000900000000000,
    observed_timestamp_unix_nano: 1700000900001000000,
    severity_number: 17,
    severity_text: "",
    body: "payment failed",
    service_name: "payments",
    environment: "prod",
    host_id: "node-2",
    attributes: {},
    resource_attributes: { "service.version": "2026.04.29" },
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
      data: logs,
      nlq_ir: {},
      source_sql: "",
      time_range: { from: "now-1h", to: "now" },
      signal_types: ["logs"],
      sample_rate: null,
      approximation_statement: "",
    },
  })),
}));

vi.mock("../api/logs", async () => {
  const actual = await vi.importActual<typeof import("../api/logs")>("../api/logs");
  return {
    ...actual,
    fetchLogHistogram: vi.fn(async (_tenantId: string, params: { from: string; to: string; buckets?: number }) => {
      const fromMs = new Date(params.from).getTime();
      const toMs = new Date(params.to).getTime();
      const count = params.buckets ?? 30;
      const intervalMs = (toMs - fromMs) / count;
      const buckets = Array.from({ length: count }, (_, i) => ({
        start_ms: fromMs + i * intervalMs,
        end_ms: fromMs + (i + 1) * intervalMs,
        counts: {},
      }));
      return { buckets };
    }),
  };
});

vi.mock("../api/dashboards", () => ({
  createDashboard: vi.fn(async () => ({
    dashboard_id: "dash-1",
    name: "Promoted log query",
    panels: [],
    created_at: "2026-04-29T00:00:00Z",
  })),
}));

vi.mock("../api/savedViews", async () => {
  const actual = await vi.importActual<typeof import("../api/savedViews")>("../api/savedViews");
  return {
    ...actual,
    fetchSavedViews: vi.fn(async () => ({
      items: [
        {
          saved_view_id: "view-1",
          name: "Only errors",
          signal_kind: "logs",
          visibility: "private",
          config: {
            query: null,
            severity_filter: "error",
            message_search: "timeout",
            time_range: { mode: "preset", preset: "1h" },
            visible_columns: ["level"],
          },
          created_at: "2026-07-01T00:00:00Z",
          updated_at: "2026-07-01T00:00:00Z",
        },
      ],
    })),
  };
});

vi.mock("@tanstack/react-router", () => ({
  Link: ({
    to,
    params,
    search,
    children,
  }: {
    to: string;
    params?: Record<string, string>;
    search?: Record<string, string>;
    children: ReactNode;
  }) => {
    let href = to;
    for (const [key, value] of Object.entries(params ?? {})) {
      href = href.replace(`$${key}`, value);
    }
    const query = search
      ? `?${new URLSearchParams(
          Object.entries(search).reduce<Record<string, string>>((acc, [key, value]) => {
            acc[key] = value;
            return acc;
          }, {}),
        ).toString()}`
      : "";
    return <a href={`${href}${query}`}>{children}</a>;
  },
  useNavigate: () => vi.fn(),
  useSearch: () => ({}),
}));

const { fetchLogHistogram } = await import("../api/logs");

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
  window.history.pushState({}, "", "/logs");
  mockSetCustomRange.mockClear();
});

function renderLogSearch() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  return render(
    <TenantContextProvider>
      <QueryClientProvider client={client}>
        <TimeDisplayProvider>
          <LogSearch />
        </TimeDisplayProvider>
      </QueryClientProvider>
    </TenantContextProvider>,
  );
}

test("queries logs via NLQ execute on load", async () => {
  const { submitNlqQuery } = await import("../api/nlq");
  renderLogSearch();

  await waitFor(() =>
    expect(submitNlqQuery).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ mode: "execute", base_ir: expect.objectContaining({ signals: ["logs"] }) }),
    ),
  );
});

test("renders histogram and primary Time Level Message columns", async () => {
  renderLogSearch();

  expect(await screen.findByRole("group", { name: "Log volume histogram" })).toBeInTheDocument();

  const table = screen.getByRole("table", { name: "Log results" });
  expect(within(table).getByRole("columnheader", { name: "Time" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "Level" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "Service" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "Message" })).toBeInTheDocument();
  expect(within(table).getByText("checkout completed")).toBeInTheDocument();
});

test("selecting a log opens context properties in the right sidebar", async () => {
  renderLogSearch();

  fireEvent.click(await screen.findByRole("button", { name: "Open log context for checkout completed" }));

  const sidebar = screen.getByRole("complementary", { name: "Selected log context" });
  expect(within(sidebar).getByText("Context Properties")).toBeInTheDocument();
  expect(within(sidebar).getByText("host.name")).toBeInTheDocument();
  expect(within(sidebar).getAllByText("node-1").length).toBeGreaterThanOrEqual(1);
  expect(within(sidebar).getByText("log.http.route")).toBeInTheDocument();
  expect(within(sidebar).getByText("/checkout")).toBeInTheDocument();
  expect(within(sidebar).getByText("environment")).toBeInTheDocument();
  expect(within(sidebar).getByText("prod")).toBeInTheDocument();
  expect(within(sidebar).getByText("trace_id")).toBeInTheDocument();
  expect(within(sidebar).getByText("trace-1")).toBeInTheDocument();
  expect(sidebar).toHaveClass("overflow-y-auto");
});

test("toggles log fields as table columns from the context panel", async () => {
  renderLogSearch();

  fireEvent.click(await screen.findByRole("button", { name: "Open log context for checkout completed" }));
  const sidebar = screen.getByRole("complementary", { name: "Selected log context" });
  const table = screen.getByRole("table", { name: "Log results" });

  fireEvent.click(within(sidebar).getByRole("button", { name: "Add service.name as a column" }));
  expect(within(table).getByRole("columnheader", { name: "service.name" })).toBeInTheDocument();
  expect(within(sidebar).getByRole("button", { name: "Remove service.name column" })).toBeEnabled();
  fireEvent.click(within(sidebar).getByRole("button", { name: "Remove service.name column" }));
  expect(within(table).queryByRole("columnheader", { name: "service.name" })).not.toBeInTheDocument();

  fireEvent.click(within(sidebar).getByRole("button", { name: "Add log.http.route as a column" }));
  expect(within(table).getByRole("columnheader", { name: "log.http.route" })).toBeInTheDocument();
  expect(within(table).getByText("/checkout")).toBeInTheDocument();
  fireEvent.click(within(sidebar).getByRole("button", { name: "Remove log.http.route column" }));
  expect(within(table).queryByRole("columnheader", { name: "log.http.route" })).not.toBeInTheDocument();
});

test("maps OpenTelemetry severity numbers into stable level labels and tones", () => {
  expect(otelSeverity(1)).toMatchObject({ label: "TRACE", tone: "neutral" });
  expect(otelSeverity(5)).toMatchObject({ label: "DEBUG", tone: "info" });
  expect(otelSeverity(9)).toMatchObject({ label: "INFO", tone: "good" });
  expect(otelSeverity(13)).toMatchObject({ label: "WARN", tone: "warn" });
  expect(otelSeverity(17)).toMatchObject({ label: "ERROR", tone: "bad" });
  expect(otelSeverity(21)).toMatchObject({ label: "FATAL", tone: "bad" });
});

test("formats structured log bodies as a message instead of raw JSON", () => {
  expect(formatLogMessage({ message: "structured message", error: "ignored" })).toBe("structured message");
  expect(formatLogMessage({ event: "cache_miss", cache: "catalog" })).toBe("event=cache_miss cache=catalog");
});

test("builds histogram buckets across the selected time range", () => {
  const toMs = Date.now();
  const fromMs = toMs - 60 * 60 * 1000;
  const buckets = buildLogHistogram(logs, fromMs, toMs);

  expect(buckets).toHaveLength(30);
  expect(buckets.reduce((sum, bucket) => sum + bucket.total, 0)).toBe(2);
  expect(buckets.some((bucket) => bucket.categories.INFO === 1)).toBe(true);
  expect(buckets.some((bucket) => bucket.categories.ERROR === 1)).toBe(true);
});

test("histogram drag selection calls setCustomRange with selected bucket range", async () => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    left: 0, top: 0, right: 600, bottom: 112, width: 600, height: 112, x: 0, y: 0,
    toJSON: () => ({}),
  });

  renderLogSearch();
  await screen.findByRole("group", { name: "Log volume histogram" });

  const histogram = screen.getByRole("group", { name: "Log volume histogram" });
  const grid = histogram.querySelector("[aria-hidden='true']") as HTMLElement;

  // Drag across the first 6 buckets (left half)
  fireEvent.pointerDown(grid, { clientX: 0, pointerId: 1 });
  fireEvent.pointerMove(grid, { clientX: 300, pointerId: 1 });
  fireEvent.pointerUp(grid, { pointerId: 1 });

  await waitFor(() => {
    expect(mockSetCustomRange).toHaveBeenCalled();
  });

  vi.restoreAllMocks();
});

test("date range controls are managed globally, not per-page", async () => {
  renderLogSearch();

  await screen.findByRole("group", { name: "Log volume histogram" });

  // The per-page time range dropdown and reset button no longer exist;
  // date range is managed by the global AppShell picker.
  expect(screen.queryByLabelText("Logs time range")).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Reset range" })).not.toBeInTheDocument();
});

test("trace_id in log context sidebar is a link to the trace detail page", async () => {
  renderLogSearch();

  fireEvent.click(await screen.findByRole("button", { name: "Open log context for checkout completed" }));

  const sidebar = screen.getByRole("complementary", { name: "Selected log context" });
  const traceLink = within(sidebar).getByRole("link", { name: "trace-1" });
  expect(traceLink).toHaveAttribute("href", "/traces/trace-1");
});

test("span_id in log context sidebar links to the parent trace", async () => {
  renderLogSearch();

  fireEvent.click(await screen.findByRole("button", { name: "Open log context for checkout completed" }));

  const sidebar = screen.getByRole("complementary", { name: "Selected log context" });
  const spanLink = within(sidebar).getByRole("link", { name: "span-1" });
  expect(spanLink).toHaveAttribute("href", "/traces/trace-1");
  expect(spanLink).toHaveAttribute("title", "View parent trace");
});

test("renders the log summary stat-card row", async () => {
  renderLogSearch();

  const summary = await screen.findByLabelText("Log summary");
  expect(summary).toBeInTheDocument();
});

test("shows 'Histogram unavailable' when histogram query fails", async () => {
  vi.mocked(fetchLogHistogram).mockRejectedValueOnce(new Error("histogram backend error"));

  renderLogSearch();

  await waitFor(() => {
    expect(screen.getByText("Histogram unavailable")).toBeInTheDocument();
  });
});

test("loading a saved view applies its severity filter and message search", async () => {
  renderLogSearch();

  fireEvent.click(screen.getByRole("button", { name: /saved views/i }));
  await waitFor(() => screen.getByText("Only errors"));
  fireEvent.click(screen.getByText("Only errors"));

  await waitFor(() => {
    expect(screen.getByLabelText("Search log messages")).toHaveValue("timeout");
  });
});

test("plain-mode quick filter matches substrings as before", async () => {
  renderLogSearch();

  const input = await screen.findByLabelText("Search log messages");
  fireEvent.change(input, { target: { value: "failed" } });

  await waitFor(() => {
    expect(screen.getByText("payment failed")).toBeInTheDocument();
    expect(screen.queryByText("checkout completed")).not.toBeInTheDocument();
  });
});

test("regex-mode quick filter matches a pattern against log messages", async () => {
  renderLogSearch();

  fireEvent.click(await screen.findByRole("button", { name: /enable regex quick filter/i }));
  const input = screen.getByLabelText("Search log messages");
  fireEvent.change(input, { target: { value: "^payment" } });

  await waitFor(() => {
    expect(screen.getByText("payment failed")).toBeInTheDocument();
    expect(screen.queryByText("checkout completed")).not.toBeInTheDocument();
  });
});

test("invalid regex in regex mode shows all rows with an inline notice", async () => {
  renderLogSearch();

  fireEvent.click(await screen.findByRole("button", { name: /enable regex quick filter/i }));
  const input = screen.getByLabelText("Search log messages");
  fireEvent.change(input, { target: { value: "(unterminated" } });

  await waitFor(() => {
    expect(screen.getByText("payment failed")).toBeInTheDocument();
    expect(screen.getByText("checkout completed")).toBeInTheDocument();
    expect(screen.getByText("Invalid regex — showing all results.")).toBeInTheDocument();
  });
});

test("histogram resets to a zero-filled range when the API returns no buckets", async () => {
  vi.mocked(fetchLogHistogram).mockResolvedValueOnce({ buckets: [] });

  renderLogSearch();
  await screen.findByRole("group", { name: "Log volume histogram" });

  const histogram = screen.getByRole("group", { name: "Log volume histogram" });
  // Each of the 30 zero-filled fallback buckets renders a background bar rect,
  // even with zero counts — an empty `buckets: []` response must not leave the
  // histogram with no bars at all (i.e. it must reset, not go blank/broken).
  const bars = histogram.querySelectorAll("rect");
  expect(bars.length).toBe(30);
});

test("histogram renders visible bars when API returns non-zero severity counts", async () => {
  vi.mocked(fetchLogHistogram).mockResolvedValueOnce({
    buckets: [
      { start_ms: 1700000000000, end_ms: 1700001000000, counts: { 9: 5, 17: 2 } },
      { start_ms: 1700001000000, end_ms: 1700002000000, counts: {} },
    ],
  });

  renderLogSearch();
  await screen.findByRole("group", { name: "Log volume histogram" });

  const histogram = screen.getByRole("group", { name: "Log volume histogram" });
  // bars have title="<timestamp> <LEVEL>: <count>" — only rendered for count > 0
  const infoBar = histogram.querySelector("[title*='INFO: 5']");
  const errorBar = histogram.querySelector("[title*='ERROR: 2']");
  expect(infoBar).toBeInTheDocument();
  expect(errorBar).toBeInTheDocument();
});

