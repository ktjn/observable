import { describe, expect, it } from "vitest";
import type { InfrastructureEntitySummary } from "../api/infrastructure";
import {
  buildLiveTelemetryMicroBatch,
  buildLiveTickNotification,
  modulateInfrastructureEntity,
} from "./liveTick";

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

describe("playground liveTick", () => {
  it("builds a correlated live micro-batch anchored to nowMs with unique UUIDv4 IDs", () => {
    const batch = buildLiveTelemetryMicroBatch({
      spans: [
        {
          tenant_id: "t1",
          trace_id: "tr-1",
          span_id: "sp-1",
          parent_span_id: null,
          service_name: "api-gateway",
          operation_name: "GET /checkout",
          duration_ns: "20000000",
          status_code: "OK",
          environment: "production",
          start_time_unix_nano: "1000000000",
        },
        {
          tenant_id: "t1",
          trace_id: "tr-1",
          span_id: "sp-2",
          parent_span_id: "sp-1",
          service_name: "checkout",
          operation_name: "POST /pay",
          duration_ns: "10000000",
          status_code: "OK",
          environment: "production",
          start_time_unix_nano: "1005000000",
        },
        {
          tenant_id: "t1",
          trace_id: "tr-2",
          span_id: "sp-3",
          parent_span_id: null,
          service_name: "web",
          operation_name: "GET /",
          duration_ns: "5000000",
          status_code: "OK",
          environment: "production",
          start_time_unix_nano: "1000000000",
        },
      ],
      logs: [
        {
          tenant_id: "t1",
          log_id: "lg-1",
          timestamp_unix_nano: "1006000000",
          observed_timestamp_unix_nano: "1006000000",
          severity_number: 9,
          severity_text: "INFO",
          body: "checkout started",
          trace_id: "tr-1",
          span_id: "sp-2",
          service_name: "checkout",
          environment: "production",
          host_id: "h1",
        },
      ],
      generatedSeries: [
        {
          tenant_id: "t1",
          metric_series_id: "gen-s1",
          metric_name: "http.server.duration",
          description: "",
          unit: "ms",
          metric_type: "gauge",
          is_monotonic: false,
          aggregation_temporality: "unspecified",
          service_name: "checkout",
          environment: "production",
        },
      ],
      generatedPoints: [
        {
          tenant_id: "t1",
          metric_series_id: "gen-s1",
          time_unix_nano: "1000000000",
          start_time_unix_nano: "1000000000",
          value_double: 123.4,
        },
      ],
      existingSeries: [
        {
          tenant_id: "t1",
          metric_series_id: "seed-s1",
          metric_name: "http.server.duration",
          description: "",
          unit: "ms",
          metric_type: "gauge",
          is_monotonic: false,
          aggregation_temporality: "unspecified",
          service_name: "checkout",
          environment: "production",
        },
      ],
      tickSeq: 4,
      nowMs: 1_700_000_000_000,
    });

    expect(batch.spans).toHaveLength(3);
    expect(batch.spans[0].trace_id).toMatch(UUID_RE);
    expect(batch.spans[0].span_id).toMatch(UUID_RE);
    expect(batch.spans[1].trace_id).toBe(batch.spans[0].trace_id);
    expect(batch.spans[1].parent_span_id).toBe(batch.spans[0].span_id);
    expect(batch.spans[0].start_time_unix_nano).toBe("1699999999980000000");

    expect(batch.logs).toHaveLength(1);
    expect(batch.logs[0].trace_id).toBe(batch.spans[0].trace_id);
    expect(batch.logs[0].span_id).toBe(batch.spans[1].span_id);
    expect(batch.logs[0].log_id).toMatch(UUID_RE);

    expect(batch.series).toHaveLength(0);
    expect(batch.points).toHaveLength(1);
    expect(batch.points[0].metric_series_id).toBe("seed-s1");
    expect(batch.points[0].value_double).toBe(123.4);
  });

  it("modulates infrastructure entity metrics across time while preserving bounds", () => {
    const base: InfrastructureEntitySummary = {
      entity_type: "host",
      entity_id: "playground-host-1",
      display_name: "demo-node-1",
      parent_id: null,
      parent_display_name: null,
      environment: "production",
      health_state: "healthy",
      last_seen_unix_nano: 1000,
      related_services: ["checkout"],
      log_rate_per_minute: 42,
      error_rate: 0.02,
      restart_count: 0,
      cpu_usage: 38,
      memory_usage: 61,
      disk_usage: 47,
      network_io: 1204,
    };

    const t1 = modulateInfrastructureEntity(base, 1_700_000_000_000);
    const t2 = modulateInfrastructureEntity(base, 1_700_000_012_000);
    expect(t1.cpu_usage).not.toBe(t2.cpu_usage);
    expect(t1.network_io).not.toBe(t2.network_io);
    expect(t1.last_seen_unix_nano).toBe(1_700_000_000_000_000_000);
  });

  it("builds a live-tick notification anchored to nowMs, with per-signal counts from the batch", () => {
    const notification = buildLiveTickNotification(
      {
        spans: [{ span_id: "s1" }, { span_id: "s2" }] as unknown as never[],
        logs: [{ log_id: "l1" }] as unknown as never[],
        series: [],
        points: [{ metric_series_id: "m1" }, { metric_series_id: "m2" }, { metric_series_id: "m3" }] as unknown as never[],
      },
      1_700_000_000_000,
    );
    expect(notification).toEqual({
      type: "live-tick",
      nowMs: 1_700_000_000_000,
      logs: { watermarkNs: "1700000000000000000", count: 1 },
      spans: { watermarkNs: "1700000000000000000", count: 2 },
      metrics: { watermarkNs: "1700000000000000000", count: 3 },
    });
  });

  it("reports zero counts for an empty batch", () => {
    const notification = buildLiveTickNotification(
      { spans: [], logs: [], series: [], points: [] },
      1_700_000_000_000,
    );
    expect(notification.logs.count).toBe(0);
    expect(notification.spans.count).toBe(0);
    expect(notification.metrics.count).toBe(0);
  });
});
