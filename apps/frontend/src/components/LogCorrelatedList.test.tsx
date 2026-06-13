import { expect, test } from "vitest";
import type { LogRecord } from "../api/logs";
import { correlationLabel, filterCorrelatedLogs } from "./LogCorrelatedList";

function makeLog(log_id: string, span_id?: string): LogRecord {
  return {
    tenant_id: "00000000-0000-0000-0000-000000000001",
    log_id,
    timestamp_unix_nano: 100,
    observed_timestamp_unix_nano: 100,
    severity_number: 9,
    severity_text: "WARN",
    body: "message",
    trace_id: "trace-1",
    span_id,
    attributes: {},
    resource_attributes: {},
    service_name: "checkout",
    environment: "prod",
    host_id: "node-1",
  };
}

test("filterCorrelatedLogs keeps trace-level logs when a span is selected", () => {
  const logs = [makeLog("exact", "span-1"), makeLog("trace-level"), makeLog("other", "span-2")];

  const filtered = filterCorrelatedLogs(logs, "span-1");

  expect(filtered.map((log) => log.log_id)).toEqual(["exact", "trace-level"]);
});

test("correlationLabel distinguishes exact span from trace-level logs", () => {
  expect(correlationLabel(makeLog("exact", "span-1"))).toBe("Exact span");
  expect(correlationLabel(makeLog("trace-level"))).toBe("Trace-level");
});
