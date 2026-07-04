import { describe, expect, it } from "vitest";
import type { LogRecord } from "../api/logs";
import { getLogFieldValue, isPromotableLogKey, logContextEntries } from "./logContext";

const log: LogRecord = {
  tenant_id: "00000000-0000-0000-0000-000000000001",
  log_id: "log-1",
  timestamp_unix_nano: 1700000000000000000,
  observed_timestamp_unix_nano: 1700000000001000000,
  severity_number: 17,
  severity_text: "ERROR",
  body: "checkout failed",
  trace_id: "trace-1",
  span_id: "span-1",
  service_name: "checkout",
  environment: "prod",
  host_id: "node-1",
  fingerprint: 12345,
  attributes: { "error.type": "TimeoutError" },
  resource_attributes: { "k8s.pod.name": "checkout-7f9" },
};

describe("getLogFieldValue", () => {
  it("resolves fixed fields", () => {
    expect(getLogFieldValue(log, "service.name", "iso-utc-ms")).toBe("checkout");
    expect(getLogFieldValue(log, "severity_number", "iso-utc-ms")).toBe("17");
    expect(getLogFieldValue(log, "message", "iso-utc-ms")).toBe("checkout failed");
    expect(getLogFieldValue(log, "trace_id", "iso-utc-ms")).toBe("trace-1");
  });

  it("resolves log.<attr> keys from log.attributes", () => {
    expect(getLogFieldValue(log, "log.error.type", "iso-utc-ms")).toBe("TimeoutError");
  });

  it("resolves bare keys from resource_attributes", () => {
    expect(getLogFieldValue(log, "k8s.pod.name", "iso-utc-ms")).toBe("checkout-7f9");
  });

  it("returns an empty string for a missing attribute", () => {
    expect(getLogFieldValue(log, "log.missing", "iso-utc-ms")).toBe("");
  });
});

describe("isPromotableLogKey", () => {
  it("rejects fixed fields", () => {
    expect(isPromotableLogKey("time")).toBe(false);
    expect(isPromotableLogKey("message")).toBe(false);
    expect(isPromotableLogKey("severity_number")).toBe(false);
  });

  it("accepts attribute and resource attribute keys", () => {
    expect(isPromotableLogKey("log.error.type")).toBe(true);
    expect(isPromotableLogKey("k8s.pod.name")).toBe(true);
  });
});

describe("logContextEntries", () => {
  it("includes fixed fields and attribute entries in order", () => {
    const entries = logContextEntries(log, "iso-utc-ms");
    const keys = entries.map(([k]) => k);
    expect(keys).toEqual([
      "time",
      "service.name",
      "severity_number",
      "message",
      "observed_time",
      "environment",
      "host_id",
      "trace_id",
      "span_id",
      "fingerprint",
      "log.error.type",
      "k8s.pod.name",
    ]);
  });
});
