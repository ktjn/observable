import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type { LogRecord } from "../../../api/logs";
import { LogResultsTable } from "./LogResultsTable";

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

const logs: LogRecord[] = [
  {
    tenant_id: "00000000-0000-0000-0000-000000000001",
    log_id: "log-1",
    timestamp_unix_nano: 1700000000000000000,
    observed_timestamp_unix_nano: 1700000000001000000,
    severity_number: 9,
    severity_text: "INFO",
    body: "checkout completed",
    trace_id: "trace-1",
    span_id: "span-1",
    service_name: "checkout",
    environment: "prod",
    host_id: "node-1",
    attributes: {},
    resource_attributes: {},
  },
];

test("renders canonical values with severity presentation and message copy", () => {
  const onSelect = vi.fn();

  render(
    <LogResultsTable
      logs={logs}
      selectedLogId={undefined}
      onSelectLog={onSelect}
      timeFormat="iso-utc-ms"
    />,
  );

  const table = screen.getByRole("table", { name: "Log results" });
  expect(within(table).getByRole("columnheader", { name: "time" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "severity_number" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "service.name" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "message" })).toBeInTheDocument();
  expect(within(table).getByText("INFO")).toHaveClass("text-[var(--good)]");
  expect(within(table).getByRole("button", { name: "Copy message" })).toBeInTheDocument();

  fireEvent.click(within(table).getByRole("row", { name: "Open log context for checkout completed" }));

  expect(onSelect).toHaveBeenCalledWith("log-1");
});

test("can hide the service column for already scoped service log views", () => {
  render(
    <LogResultsTable
      logs={logs}
      selectedLogId="log-1"
      onSelectLog={vi.fn()}
      timeFormat="iso-utc-ms"
      visibleColumns={["time", "severity_number", "message"]}
      ariaLabel="Service logs"
    />,
  );

  const table = screen.getByRole("table", { name: "Service logs" });
  expect(within(table).queryByRole("columnheader", { name: "service.name" })).not.toBeInTheDocument();
  expect(within(table).getByText("checkout completed")).toBeInTheDocument();
});

test("renders only the ordered selected field keys", () => {
  render(
    <LogResultsTable
      logs={logs}
      selectedLogId={undefined}
      onSelectLog={vi.fn()}
      timeFormat="iso-utc-ms"
      visibleColumns={["message", "service.name"]}
    />,
  );
  expect(screen.getAllByRole("columnheader").map((header) => header.textContent)).toEqual([
    "message",
    "service.name",
  ]);
});

test("shows all columns when visibleColumns is omitted", () => {
  render(
    <LogResultsTable
      logs={logs}
      selectedLogId={undefined}
      onSelectLog={vi.fn()}
      timeFormat="iso-utc-ms"
    />,
  );
  expect(screen.getByRole("columnheader", { name: "severity_number" })).toBeInTheDocument();
  expect(screen.getByRole("columnheader", { name: "service.name" })).toBeInTheDocument();
});

test("renders an accessible state when no columns are selected", () => {
  render(
    <LogResultsTable
      logs={logs}
      selectedLogId={undefined}
      onSelectLog={vi.fn()}
      timeFormat="iso-utc-ms"
      visibleColumns={[]}
    />,
  );
  expect(screen.getByRole("columnheader", { name: "No columns selected" })).toBeInTheDocument();
  expect(screen.getByRole("cell", { name: "No columns selected" })).toBeInTheDocument();
});
