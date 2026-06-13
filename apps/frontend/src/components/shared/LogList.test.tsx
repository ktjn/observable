import { render, screen, fireEvent } from "@testing-library/react";
import { vi, expect, test } from "vitest";
import type { LogRecord } from "../../api/logs";
import { LogList } from "./LogList";

const log: LogRecord = {
  tenant_id: "t1",
  log_id: "log-1",
  timestamp_unix_nano: 1700000000000000000,
  observed_timestamp_unix_nano: 1700000000000000000,
  severity_number: 9,
  severity_text: "INFO",
  body: "checkout completed",
  trace_id: "trace-abc",
  attributes: {},
  resource_attributes: {},
  service_name: "svc",
  environment: "prod",
  host_id: "node-1",
};

test("renders timestamp, severity, and message for each log row", () => {
  render(<LogList logs={[log]} timeFormat="iso-utc-ms" />);
  expect(screen.getByText("checkout completed")).toBeInTheDocument();
  expect(screen.getByText("INFO")).toBeInTheDocument();
});

test("shows loading text when loading=true", () => {
  render(<LogList logs={[]} loading timeFormat="iso-utc-ms" />);
  expect(screen.getByText(/Loading logs/)).toBeInTheDocument();
});

test("shows custom emptyMessage when no logs", () => {
  render(<LogList logs={[]} emptyMessage="Nothing here." timeFormat="iso-utc-ms" />);
  expect(screen.getByText("Nothing here.")).toBeInTheDocument();
});

test("highlights the pivot row with warn-bg class", () => {
  render(<LogList logs={[log]} pivotId="log-1" timeFormat="iso-utc-ms" />);
  const row = screen.getByText("checkout completed").closest("div[data-log-id]")!;
  expect(row.className).toMatch(/warn-bg/);
});

test("calls onRowClick when row is clicked", () => {
  const onClick = vi.fn();
  render(<LogList logs={[log]} onRowClick={onClick} timeFormat="iso-utc-ms" />);
  fireEvent.click(screen.getByText("checkout completed"));
  expect(onClick).toHaveBeenCalledWith(log);
});

test("renders trace link when showTraceLink=true and log has trace_id", () => {
  render(<LogList logs={[log]} showTraceLink timeFormat="iso-utc-ms" />);
  const link = screen.getByRole("link", { name: /View trace/ });
  expect(link).toHaveAttribute("href", "/traces/trace-abc");
});

test("does not render trace link when showTraceLink is omitted", () => {
  render(<LogList logs={[log]} timeFormat="iso-utc-ms" />);
  expect(screen.queryByRole("link", { name: /View trace/ })).not.toBeInTheDocument();
});
