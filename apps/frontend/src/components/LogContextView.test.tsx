import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, beforeEach } from "vitest";
import { LogContextView } from "./LogContextView";
import { TimeDisplayProvider } from "../lib/timeDisplay";
import { TenantContextProvider } from "../hooks/useTenantContext";
import type { LogListResponse } from "../api/logs";
import type { RuntimeApi } from "../runtime/types";

vi.mock("../hooks/useRuntime", () => ({
  useRuntime: vi.fn(),
}));

import { useRuntime } from "../hooks/useRuntime";

function mockLogContext(response: Promise<LogListResponse>) {
  vi.mocked(useRuntime).mockReturnValue({
    logs: { context: vi.fn(() => response) },
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

const pivotLog = {
  tenant_id: "t1",
  log_id: "pivot-id",
  timestamp_unix_nano: 1000000000,
  observed_timestamp_unix_nano: 1000000000,
  severity_number: 9,
  severity_text: "WARN",
  body: "pivot message",
  attributes: {},
  resource_attributes: {},
  service_name: "checkout",
  environment: "prod",
  host_id: "node-1",
};

const beforeLog = {
  tenant_id: "t1",
  log_id: "before-id",
  timestamp_unix_nano: 500000000,
  observed_timestamp_unix_nano: 500000000,
  severity_number: 5,
  severity_text: "INFO",
  body: "before message",
  attributes: {},
  resource_attributes: {},
  service_name: "checkout",
  environment: "prod",
  host_id: "node-1",
};

const traceLinkedLog = {
  tenant_id: "t1",
  log_id: "trace-linked-id",
  timestamp_unix_nano: 1500000000,
  observed_timestamp_unix_nano: 1500000000,
  severity_number: 5,
  severity_text: "INFO",
  body: "trace linked message",
  trace_id: "trace-abc",
  span_id: "span-xyz",
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
  mockLogContext(new Promise<LogListResponse>(() => {}));
  render(<LogContextView logId="pivot-id" onClose={vi.fn()} />, { wrapper });
  expect(screen.getByText(/Loading logs/)).toBeInTheDocument();
});

test("renders log lines with pivot highlighted", async () => {
  mockLogContext(Promise.resolve({
    logs: [beforeLog, pivotLog],
    total: 2,
    facets: {},
  }));

  render(<LogContextView logId="pivot-id" onClose={vi.fn()} />, { wrapper });

  await waitFor(() => expect(screen.getByText("pivot message")).toBeInTheDocument());
  expect(screen.getByText("before message")).toBeInTheDocument();

  expect(screen.getByText("[PIVOT]")).toBeInTheDocument();

  const pivotRow = screen.getByText("pivot message").closest("div")!;
  expect(pivotRow.className).toMatch(/warn-bg/);
});

test("trace-linked log renders a link with correct href and aria-label", async () => {
  mockLogContext(Promise.resolve({
    logs: [traceLinkedLog],
    total: 1,
    facets: {},
  }));

  render(<LogContextView logId="trace-linked-id" onClose={vi.fn()} />, { wrapper });
  await waitFor(() => expect(screen.getByText("trace linked message")).toBeInTheDocument());

  const link = screen.getByRole("link", { name: "View span span-xyz" });
  expect(link).toHaveAttribute("href", "/traces/trace-abc");
});

test("calls onClose when Close button is clicked", async () => {
  mockLogContext(Promise.resolve({
    logs: [pivotLog],
    total: 1,
    facets: {},
  }));
  const onClose = vi.fn();

  render(<LogContextView logId="pivot-id" onClose={onClose} />, { wrapper });
  await waitFor(() => screen.getByText("pivot message"));

  fireEvent.click(screen.getByRole("button", { name: /close/i }));
  expect(onClose).toHaveBeenCalledOnce();
});
