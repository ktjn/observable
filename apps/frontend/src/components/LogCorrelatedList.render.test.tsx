import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, beforeEach } from "vitest";
import { LogCorrelatedList } from "./LogCorrelatedList";
import { TimeDisplayProvider } from "../lib/timeDisplay";
import { TenantContextProvider } from "../hooks/useTenantContext";
import * as logsApi from "../api/logs";

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <QueryClientProvider client={qc}>
      <TenantContextProvider>
        <TimeDisplayProvider>{children}</TimeDisplayProvider>
      </TenantContextProvider>
    </QueryClientProvider>
  );
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

test("shows all logs when no span selected", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [traceLog, spanLog],
    total: 2,
    facets: {},
  });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() =>
    expect(screen.getByText("trace level message")).toBeInTheDocument()
  );
  expect(screen.getByText("span level message")).toBeInTheDocument();
});

test("filters to span and trace-level logs when spanId is provided", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [traceLog, spanLog],
    total: 2,
    facets: {},
  });

  render(<LogCorrelatedList traceId="trace-abc" spanId="span-111" />, { wrapper });
  await waitFor(() =>
    expect(screen.getByText("span level message")).toBeInTheDocument()
  );
  expect(screen.getByText("trace level message")).toBeInTheDocument();
});

test("clicking a log row opens the context view with Surrounding Logs heading", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [traceLog],
    total: 1,
    facets: {},
  });
  vi.spyOn(logsApi, "getLogContext").mockReturnValue(new Promise(() => {}));

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() => screen.getByText("trace level message"));

  const row = screen.getByText("trace level message").closest('[role="button"]')!;
  fireEvent.click(row);
  expect(screen.getByText(/Surrounding Logs/)).toBeInTheDocument();
});

test("span-linked log renders trace link with span aria-label", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [spanLog],
    total: 1,
    facets: {},
  });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() =>
    expect(screen.getByRole("link", { name: `View span ${spanLog.span_id}` })).toBeInTheDocument()
  );
  const link = screen.getByRole("link", { name: `View span ${spanLog.span_id}` });
  expect(link).toHaveAttribute("href", "/traces/trace-abc");
});

test("trace-level log renders trace link with trace aria-label", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [traceLog],
    total: 1,
    facets: {},
  });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() =>
    expect(screen.getByRole("link", { name: `View trace ${traceLog.trace_id}` })).toBeInTheDocument()
  );
});
