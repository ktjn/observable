import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import type { LogRecord } from "../api/logs";
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
    getLogHistogram: vi.fn(async () => ({
      bucket_count: 12,
      entries: [
        { bucket_index: 0, severity_number: 9, count: 1 },
        { bucket_index: 11, severity_number: 17, count: 1 },
      ],
    })),
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

const { searchLogs } = await import("../api/logs");

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
      <LogSearch />
    </QueryClientProvider>,
  );
}

test("queries logs using the selected time range", async () => {
  renderLogSearch();

  await waitFor(() =>
    expect(searchLogs).toHaveBeenCalledWith(
      expect.objectContaining({ from: expect.any(String), to: expect.any(String) }),
    ),
  );

  fireEvent.change(screen.getByLabelText("Log time range"), { target: { value: "360" } });

  await waitFor(() =>
    expect(searchLogs).toHaveBeenLastCalledWith(
      expect.objectContaining({ from: expect.any(String), to: expect.any(String) }),
    ),
  );
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

test("selecting a log opens context properties in the left sidebar", async () => {
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
  const buckets = buildLogHistogram(logs, 60);

  expect(buckets).toHaveLength(12);
  expect(buckets.reduce((sum, bucket) => sum + bucket.total, 0)).toBe(2);
  expect(buckets.some((bucket) => bucket.levels.INFO === 1)).toBe(true);
  expect(buckets.some((bucket) => bucket.levels.ERROR === 1)).toBe(true);
});
