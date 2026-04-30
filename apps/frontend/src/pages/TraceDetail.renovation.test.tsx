import { render, screen, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, beforeEach } from "vitest";
import { TimeDisplayProvider } from "../lib/timeDisplay";
import { TraceDetail } from "./TraceDetail";
import * as logsApi from "../api/logs";

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}><TimeDisplayProvider>{children}</TimeDisplayProvider></QueryClientProvider>;
}

const baseSpan = {
  trace_id: "abc",
  tenant_id: "t1",
  span_id: "111",
  service_name: "checkout",
  operation_name: "POST /order",
  start_time_unix_nano: 0,
  end_time_unix_nano: 5000000,
  duration_ns: 5_000_000,
  status_code: "OK",
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
