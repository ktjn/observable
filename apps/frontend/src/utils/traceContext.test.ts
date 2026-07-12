import { describe, expect, test } from "vitest";
import type { TraceResponse } from "../api/traces";
import { getTraceFieldValue, traceContextEntries } from "./traceContext";

const trace: TraceResponse = {
  trace_id: "trace-1",
  events: [],
  spans: [{
    tenant_id: "tenant-1", trace_id: "trace-1", span_id: "span-1",
    service_name: "checkout", service_namespace: "shop", service_version: "1.0",
    operation_name: "GET /checkout", span_kind: "SERVER",
    start_time_unix_nano: 1, end_time_unix_nano: 5_000_001, duration_ns: 5_000_000,
    status_code: "OK", status_message: "", environment: "prod", host_id: "host-1",
    workload: "checkout-api", deployment_id: "deploy-1",
    attributes: { "http.route": "/checkout", collision: "span value" },
    resource_attributes: { "k8s.pod.name": "checkout-7f9", collision: "resource value" },
  }],
};

describe("getTraceFieldValue", () => {
  test("resolves every built-in trace context field", () => {
    expect(getTraceFieldValue(trace, "trace_id", "iso-utc-ms")).toBe("trace-1");
    expect(getTraceFieldValue(trace, "start_time", "iso-utc-ms")).toBe("1970-01-01 00:00:00.000Z");
    expect(getTraceFieldValue(trace, "service.name", "iso-utc-ms")).toBe("checkout");
    expect(getTraceFieldValue(trace, "operation", "iso-utc-ms")).toBe("GET /checkout");
    expect(getTraceFieldValue(trace, "duration", "iso-utc-ms")).toBe("5.00ms");
    expect(getTraceFieldValue(trace, "status", "iso-utc-ms")).toBe("OK");
  });

  test("resolves collision-safe span and resource attributes", () => {
    expect(getTraceFieldValue(trace, "span.http.route", "iso-utc-ms")).toBe("/checkout");
    expect(getTraceFieldValue(trace, "span.collision", "iso-utc-ms")).toBe("span value");
    expect(getTraceFieldValue(trace, "resource.collision", "iso-utc-ms")).toBe("resource value");
    expect(getTraceFieldValue(trace, "resource.k8s.pod.name", "iso-utc-ms")).toBe("checkout-7f9");
  });

  test("returns an empty string for unavailable fields", () => {
    expect(getTraceFieldValue({ ...trace, spans: [] }, "operation", "iso-utc-ms")).toBe("");
    expect(getTraceFieldValue(trace, "span.missing", "iso-utc-ms")).toBe("");
  });
});

test("traceContextEntries returns built-ins then sorted namespaced attributes", () => {
  expect(traceContextEntries(trace, "iso-utc-ms").map(([key]) => key)).toEqual([
    "start_time", "trace_id", "service.name", "operation", "duration", "status",
    "span.collision", "span.http.route", "resource.collision", "resource.k8s.pod.name",
  ]);
});
