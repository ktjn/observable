import { render, screen, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, beforeEach } from "vitest";
import { TimeDisplayProvider } from "../lib/timeDisplay";
import { TenantContextProvider } from "../hooks/useTenantContext";
import { TraceDetail } from "./TraceDetail";
import * as logsApi from "../api/logs";

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    Link: ({
      children,
      ...props
    }: React.AnchorHTMLAttributes<HTMLAnchorElement> & { children?: React.ReactNode }) => (
      <a {...props}>{children}</a>
    ),
  };
});

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

const baseSpan = {
  trace_id: "abc",
  tenant_id: "t1",
  span_id: "111",
  service_name: "checkout",
  service_namespace: "production",
  service_version: "1.0.0",
  operation_name: "POST /order",
  span_kind: "INTERNAL",
  start_time_unix_nano: 0,
  end_time_unix_nano: 5000000,
  duration_ns: 5_000_000,
  status_code: "OK",
  status_message: "",
  environment: "prod",
  host_id: "host-1",
  workload: "checkout-service",
  deployment_id: "deploy-123",
};

beforeEach(() => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [],
    total: 0,
    facets: {},
  });
});

test("clicking a waterfall row selects it and clicking again deselects it", () => {
  render(<TraceDetail traceId="abc" spans={[baseSpan]} />, { wrapper });

  const row = screen.getByRole("button", { name: /checkout: POST \/order/ });

  expect(row.className).not.toMatch(/surface-subtle/);

  fireEvent.click(row);
  expect(row.className).toMatch(/surface-subtle/);

  fireEvent.click(row);
  expect(row.className).not.toMatch(/surface-subtle/);
});

test("pressing Enter on a waterfall row selects it", () => {
  render(<TraceDetail traceId="abc" spans={[baseSpan]} />, { wrapper });

  const row = screen.getByRole("button", { name: /checkout: POST \/order/ });
  fireEvent.keyDown(row, { key: "Enter" });
  expect(row.className).toMatch(/surface-subtle/);
});

test("pressing Space on a waterfall row selects it", () => {
  render(<TraceDetail traceId="abc" spans={[baseSpan]} />, { wrapper });

  const row = screen.getByRole("button", { name: /checkout: POST \/order/ });
  fireEvent.keyDown(row, { key: " " });
  expect(row.className).toMatch(/surface-subtle/);
});
