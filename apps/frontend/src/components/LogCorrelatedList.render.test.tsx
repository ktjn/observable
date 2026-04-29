import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, beforeEach } from "vitest";
import { LogCorrelatedList } from "./LogCorrelatedList";
import * as logsApi from "../api/logs";

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

const traceLog = {
  tenant_id: "t1",
  log_id: "trace-log-1",
  timestamp_unix_nano: "1000000000",
  severity_number: 5,
  severity_text: "INFO",
  body: "trace level message",
  trace_id: "trace-abc",
  service_name: "checkout",
};

const spanLog = {
  tenant_id: "t1",
  log_id: "span-log-1",
  timestamp_unix_nano: "2000000000",
  severity_number: 9,
  severity_text: "WARN",
  body: "span level message",
  trace_id: "trace-abc",
  span_id: "span-111",
  service_name: "checkout",
};

beforeEach(() => {
  vi.restoreAllMocks();
});

test("shows loading state while fetching", () => {
  vi.spyOn(logsApi, "searchLogs").mockReturnValue(new Promise(() => {}));
  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  expect(screen.getByText(/Loading logs/)).toBeInTheDocument();
});

test("shows empty message when no correlated logs found", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [],
    total: 0,
    facets: {},
  });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() =>
    expect(screen.getByText(/No correlated logs found/)).toBeInTheDocument()
  );
});

test("shows trace-correlated heading when no span selected", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [traceLog, spanLog],
    total: 2,
    facets: {},
  });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() =>
    expect(screen.getByText(/Trace-correlated logs/)).toBeInTheDocument()
  );
  expect(screen.getByText("trace level message")).toBeInTheDocument();
  expect(screen.getByText("span level message")).toBeInTheDocument();
});

test("shows span-scoped heading and filters when spanId is provided", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [traceLog, spanLog],
    total: 2,
    facets: {},
  });

  render(<LogCorrelatedList traceId="trace-abc" spanId="span-111" />, { wrapper });
  await waitFor(() =>
    expect(screen.getByText(/Exact span logs and trace-level logs/)).toBeInTheDocument()
  );
  expect(screen.getByText("span level message")).toBeInTheDocument();
  expect(screen.getByText("trace level message")).toBeInTheDocument();
});

test("clicking a log row marks it as focused with surface-subtle background", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [traceLog],
    total: 1,
    facets: {},
  });
  vi.spyOn(logsApi, "getLogContext").mockReturnValue(new Promise(() => {}));

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() => screen.getByText("trace level message"));

  const row = screen.getByText("trace level message").closest('[role="button"]')!;
  expect(row.className).not.toMatch(/surface-subtle/);

  fireEvent.click(row);
  expect(row.className).toMatch(/surface-subtle/);
});

test("Exact span correlation label appears for span-linked logs", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [spanLog],
    total: 1,
    facets: {},
  });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() => expect(screen.getByText("Exact span")).toBeInTheDocument());
});

test("Trace-level correlation label appears for logs without span_id", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [traceLog],
    total: 1,
    facets: {},
  });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() => expect(screen.getByText("Trace-level")).toBeInTheDocument());
});
