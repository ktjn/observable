import { fireEvent, render, screen, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { expect, test, vi } from "vitest";
import type { TraceResponse } from "../../../api/traces";
import { TraceResultsTable } from "./TraceResultsTable";

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
      for (const [key, value] of Object.entries(params)) {
        href = href.replace(`$${key}`, value);
      }
    }
    return <a href={href}>{children}</a>;
  },
}));

const traces: TraceResponse[] = [
  {
    trace_id: "trace-abc-1234567890",
    events: [],
    spans: [
      {
        tenant_id: "00000000-0000-0000-0000-000000000001",
        trace_id: "trace-abc-1234567890",
        span_id: "span-root",
        service_name: "checkout",
        service_namespace: "shop",
        service_version: "2026.04.30",
        operation_name: "GET /checkout",
        span_kind: "SERVER",
        start_time_unix_nano: 1,
        end_time_unix_nano: 5000001,
        duration_ns: 5000000,
        status_code: "OK",
        status_message: "",
        attributes: {},
        resource_attributes: {},
        environment: "prod",
        host_id: "host-1",
        workload: "checkout-api",
        deployment_id: "deploy-1",
      },
    ],
  },
];

test("renders selectable trace rows for the global explorer", () => {
  const onSelect = vi.fn();

  render(
    <TraceResultsTable
      traces={traces}
      selectedTraceId={undefined}
      onSelectTrace={onSelect}
    />,
  );

  const table = screen.getByRole("table", { name: "Trace results" });
  expect(within(table).getByRole("columnheader", { name: "Trace ID" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "Service" })).toBeInTheDocument();
  expect(within(table).getByText("GET /checkout")).toBeInTheDocument();

  // The link itself stops propagation (it navigates to the full trace page),
  // so clicking elsewhere in the row exercises the row-level select handler.
  fireEvent.click(within(table).getByText("checkout"));

  expect(onSelect).toHaveBeenCalledWith("trace-abc-1234567890");
});

test("renders linked trace rows for scoped service views", () => {
  render(
    <TraceResultsTable
      traces={traces}
      selectedTraceId={undefined}
      onSelectTrace={vi.fn()}
      mode="link"
      showServiceColumn={false}
      ariaLabel="Service traces"
    />,
  );

  const table = screen.getByRole("table", { name: "Service traces" });
  expect(within(table).queryByRole("columnheader", { name: "Service" })).not.toBeInTheDocument();
  expect(within(table).getByRole("link", { name: "trace-abc-123456" })).toHaveAttribute(
    "href",
    "/traces/trace-abc-1234567890",
  );
});

test("renders a Time column using the provided timeFormat", () => {
  render(
    <TraceResultsTable
      traces={traces}
      selectedTraceId={undefined}
      onSelectTrace={vi.fn()}
      timeFormat="iso-utc-ms"
    />,
  );

  const table = screen.getByRole("table", { name: "Trace results" });
  expect(within(table).getByRole("columnheader", { name: "Time" })).toBeInTheDocument();
  expect(within(table).getByText("1970-01-01 00:00:00.000Z")).toBeInTheDocument();
});
