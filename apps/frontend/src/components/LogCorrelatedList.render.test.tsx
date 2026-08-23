import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, beforeEach } from "vitest";
import { LogCorrelatedList } from "./LogCorrelatedList";
import { TimeDisplayProvider } from "../lib/timeDisplay";
import { TenantContextProvider } from "../hooks/useTenantContext";
import type { LogListResponse } from "../api/logs";
import type { RuntimeApi } from "../runtime/types";

vi.mock("../hooks/useRuntime", () => ({
  useRuntime: vi.fn(),
}));

import { useRuntime } from "../hooks/useRuntime";

function mockLogs({
  search,
  context,
}: {
  search?: Promise<LogListResponse>;
  context?: Promise<LogListResponse>;
}) {
  vi.mocked(useRuntime).mockReturnValue({
    logs: {
      search: search ? vi.fn(() => search) : vi.fn(),
      context: context ? vi.fn(() => context) : vi.fn(),
    },
  } as unknown as RuntimeApi);
}

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
  timestamp_unix_nano: 1000000000,
  observed_timestamp_unix_nano: 1000000000,
  severity_number: 5,
  severity_text: "INFO",
  body: "trace level message",
  trace_id: "trace-abc",
  attributes: {},
  resource_attributes: {},
  service_name: "checkout",
  environment: "prod",
  host_id: "node-1",
};

const spanLog = {
  tenant_id: "t1",
  log_id: "span-log-1",
  timestamp_unix_nano: 2000000000,
  observed_timestamp_unix_nano: 2000000000,
  severity_number: 9,
  severity_text: "WARN",
  body: "span level message",
  trace_id: "trace-abc",
  span_id: "span-111",
  attributes: {},
  resource_attributes: {},
  service_name: "checkout",
  environment: "prod",
  host_id: "node-1",
};

beforeEach(() => {
  vi.clearAllMocks();
});

test("shows loading state while fetching", () => {
  mockLogs({ search: new Promise<LogListResponse>(() => {}) });
  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  expect(screen.getByText(/Loading logs/)).toBeInTheDocument();
});

test("shows empty message when no correlated logs found", async () => {
  mockLogs({ search: Promise.resolve({ logs: [], total: 0, facets: {} }) });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() =>
    expect(screen.getByText(/No correlated logs found/)).toBeInTheDocument()
  );
});

test("shows all logs when no span selected", async () => {
  mockLogs({
    search: Promise.resolve({ logs: [traceLog, spanLog], total: 2, facets: {} }),
  });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() =>
    expect(screen.getByText("trace level message")).toBeInTheDocument()
  );
  expect(screen.getByText("span level message")).toBeInTheDocument();
});

test("filters to span and trace-level logs when spanId is provided", async () => {
  mockLogs({
    search: Promise.resolve({ logs: [traceLog, spanLog], total: 2, facets: {} }),
  });

  render(<LogCorrelatedList traceId="trace-abc" spanId="span-111" />, { wrapper });
  await waitFor(() =>
    expect(screen.getByText("span level message")).toBeInTheDocument()
  );
  expect(screen.getByText("trace level message")).toBeInTheDocument();
});

test("clicking a log row opens the context view with Surrounding Logs heading", async () => {
  mockLogs({
    search: Promise.resolve({ logs: [traceLog], total: 1, facets: {} }),
    context: new Promise<LogListResponse>(() => {}),
  });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() => screen.getByText("trace level message"));

  const row = screen.getByText("trace level message").closest('[role="listitem"]')!;
  fireEvent.click(row);
  expect(screen.getByText(/Surrounding Logs/)).toBeInTheDocument();
});

test("span-linked log renders trace link with span aria-label", async () => {
  mockLogs({
    search: Promise.resolve({ logs: [spanLog], total: 1, facets: {} }),
  });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() =>
    expect(screen.getByRole("link", { name: `View span ${spanLog.span_id}` })).toBeInTheDocument()
  );
  const link = screen.getByRole("link", { name: `View span ${spanLog.span_id}` });
  expect(link).toHaveAttribute("href", "/traces/trace-abc");
});

test("trace-level log renders trace link with trace aria-label", async () => {
  mockLogs({
    search: Promise.resolve({ logs: [traceLog], total: 1, facets: {} }),
  });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() =>
    expect(screen.getByRole("link", { name: `View trace ${traceLog.trace_id}` })).toBeInTheDocument()
  );
});
