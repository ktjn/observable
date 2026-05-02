import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type { LogRecord } from "../../api/logs";
import { LogResultsTable } from "./LogResultsTable";

const logs: LogRecord[] = [
  {
    tenant_id: "00000000-0000-0000-0000-000000000001",
    log_id: "log-1",
    timestamp_unix_nano: "1700000000000000000",
    observed_timestamp_unix_nano: "1700000000000000100",
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

test("renders the canonical log result columns and selection action", () => {
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
  expect(within(table).getByRole("columnheader", { name: "Time" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "Level" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "Service" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "Message" })).toBeInTheDocument();

  fireEvent.click(within(table).getByRole("button", { name: "Open log context for checkout completed" }));

  expect(onSelect).toHaveBeenCalledWith("log-1");
});

test("can hide the service column for already scoped service log views", () => {
  render(
    <LogResultsTable
      logs={logs}
      selectedLogId="log-1"
      onSelectLog={vi.fn()}
      timeFormat="iso-utc-ms"
      showServiceColumn={false}
    />,
  );

  const table = screen.getByRole("table", { name: "Service logs" });
  expect(within(table).queryByRole("columnheader", { name: "Service" })).not.toBeInTheDocument();
  expect(within(table).getByText("checkout completed")).toBeInTheDocument();
});
