import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import type { LogRecord } from "../api/logs";
import { TimeDisplayProvider } from "../lib/timeDisplay";
import LogSearch, {
  buildLogHistogram,
  formatLogMessage,
  otelSeverity,
} from "./LogSearch";

const logs: LogRecord[] = [
  {
    tenant_id: "00000000-0000-0000-0000-000000000001",
    log_id: "log-1",
    timestamp_unix_nano: "1700000000000000000",
    observed_timestamp_unix_nano: "1700000000000000100",
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
    timestamp_unix_nano: "1700000900000000000",
    observed_timestamp_unix_nano: "1700000900000000100",
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

vi.mock("../api/logs", async () => {
  const actual = await vi.importActual<typeof import("../api/logs")>("../api/logs");
  return {
    ...actual,
    searchLogs: vi.fn(async () => ({ logs, total: logs.length, facets: {} })),
    fetchLogHistogram: vi.fn(async (params: { from: string; to: string; buckets?: number }) => {
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

const { searchLogs, fetchLogHistogram } = await import("../api/logs");

beforeEach(() => {
  vi.clearAllMocks();
  window.history.pushState({}, "", "/logs");
});

function renderLogSearch() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  return render(
    <QueryClientProvider client={client}>
      <TimeDisplayProvider>
        <LogSearch />
      </TimeDisplayProvider>
    </QueryClientProvider>,
  );
}

test("queries logs using the selected time range", async () => {
  renderLogSearch();

  await waitFor(() => expect(searchLogs).toHaveBeenCalledWith(expect.objectContaining({ from: expect.any(String) })));

  fireEvent.change(screen.getByLabelText("Log time range"), { target: { value: "360" } });

  await waitFor(() => expect(searchLogs).toHaveBeenLastCalledWith(expect.objectContaining({ from: expect.any(String) })));
});

test("renders histogram and primary Time Level Message columns", async () => {
  renderLogSearch();

  expect(await screen.findByRole("img", { name: "Log volume histogram" })).toBeInTheDocument();

  const table = screen.getByRole("table", { name: "Log results" });
  expect(within(table).getByRole("columnheader", { name: "Time" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "Level" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "Message" })).toBeInTheDocument();
  expect(within(table).queryByRole("columnheader", { name: "Service" })).not.toBeInTheDocument();
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

test("histogram drag selection zooms the log query to the selected bucket range", async () => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    left: 0, top: 0, right: 600, bottom: 112, width: 600, height: 112, x: 0, y: 0,
    toJSON: () => ({}),
  });

  renderLogSearch();
  await screen.findByRole("img", { name: "Log volume histogram" });

  const histogram = screen.getByRole("img", { name: "Log volume histogram" });
  const grid = histogram.querySelector("[aria-hidden='true']") as HTMLElement;

  // Drag across the first 6 buckets (left half)
  fireEvent.pointerDown(grid, { clientX: 0, pointerId: 1 });
  fireEvent.pointerMove(grid, { clientX: 300, pointerId: 1 });
  fireEvent.pointerUp(grid, { pointerId: 1 });

  await waitFor(() => {
    const calls = vi.mocked(searchLogs).mock.calls;
    expect(calls.some(([p]) => p.to !== undefined)).toBe(true);
  });

  vi.restoreAllMocks();
});

test("histogram reset range button restores lookback query", async () => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    left: 0, top: 0, right: 600, bottom: 112, width: 600, height: 112, x: 0, y: 0,
    toJSON: () => ({}),
  });

  renderLogSearch();
  await screen.findByRole("img", { name: "Log volume histogram" });

  const histogram = screen.getByRole("img", { name: "Log volume histogram" });
  const grid = histogram.querySelector("[aria-hidden='true']") as HTMLElement;

  fireEvent.pointerDown(grid, { clientX: 0, pointerId: 1 });
  fireEvent.pointerMove(grid, { clientX: 300, pointerId: 1 });
  fireEvent.pointerUp(grid, { pointerId: 1 });

  // "Reset range" button should appear after selection
  const resetBtn = await screen.findByRole("button", { name: "Reset range" });
  fireEvent.click(resetBtn);

  // Dropdown restored, "Reset range" gone
  await waitFor(() => {
    expect(screen.getByLabelText("Log time range")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Reset range" })).not.toBeInTheDocument();
  });

  vi.restoreAllMocks();
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

test("shows 'Histogram unavailable' when histogram query fails", async () => {
  vi.mocked(fetchLogHistogram).mockRejectedValueOnce(new Error("histogram backend error"));

  renderLogSearch();

  await waitFor(() => {
    expect(screen.getByText("Histogram unavailable")).toBeInTheDocument();
  });
});

test("histogram renders visible bars when API returns non-zero severity counts", async () => {
  vi.mocked(fetchLogHistogram).mockResolvedValueOnce({
    buckets: [
      { start_ms: 1700000000000, end_ms: 1700001000000, counts: { 9: 5, 17: 2 } },
      { start_ms: 1700001000000, end_ms: 1700002000000, counts: {} },
    ],
  });

  renderLogSearch();
  await screen.findByRole("img", { name: "Log volume histogram" });

  const histogram = screen.getByRole("img", { name: "Log volume histogram" });
  // bars have title="<timestamp> <LEVEL>: <count>" — only rendered for count > 0
  const infoBar = histogram.querySelector("[title*='INFO: 5']");
  const errorBar = histogram.querySelector("[title*='ERROR: 2']");
  expect(infoBar).toBeInTheDocument();
  expect(errorBar).toBeInTheDocument();
});
