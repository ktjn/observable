import { describe, expect, test } from "vitest";
import { deriveViewFiltersFromIr } from "./queryFilters";

describe("deriveViewFiltersFromIr", () => {
  test("maps service catalog fields from NLQ IR filters", () => {
    const filters = deriveViewFiltersFromIr(
      {
        operation: "catalog",
        signals: ["metrics"],
        filters: [
          { field: "service_name", op: "=", value: "checkout" },
          { field: "environment", op: "=", value: "prod" },
          { field: "health_state", op: "=", value: "watch" },
        ],
      },
      "services",
    );

    expect(filters).toEqual({
      text: "checkout",
      service: "checkout",
      environment: "prod",
      health: "watch",
    });
  });

  test("maps metric browse fields from NLQ IR filters and metric name", () => {
    const filters = deriveViewFiltersFromIr(
      {
        operation: "timeseries",
        signals: ["metrics"],
        metric: "request_duration_ms",
        filters: [
          { field: "metric_type", op: "=", value: "histogram" },
          { field: "environment", op: "=", value: "staging" },
        ],
      },
      "metrics",
    );

    expect(filters).toEqual({
      metricName: "request_duration_ms",
      metricType: "histogram",
      environment: "staging",
    });
  });

  test("maps topology focus from service and environment filters", () => {
    const filters = deriveViewFiltersFromIr(
      {
        operation: "catalog",
        signals: ["metrics"],
        filters: [
          { field: "service.name", op: "=", value: "payments" },
          { field: "deployment.environment", op: "=", value: "prod" },
        ],
      },
      "topology",
    );

    expect(filters).toEqual({ service: "payments", environment: "prod" });
  });

  test("extracts environment from infrastructure NLQ IR (timeseries with env filter)", () => {
    const filters = deriveViewFiltersFromIr(
      {
        operation: "timeseries",
        signals: ["metrics"],
        metric: "order_processing_duration_ms",
        filters: [{ field: "environment", op: "=", value: "observable" }],
      },
      "infrastructure",
    );

    expect(filters).toEqual({ environment: "observable" });
  });

  test("preserves log text search when a service filter is also present", () => {
    const filters = deriveViewFiltersFromIr(
      {
        operation: "logs",
        signals: ["logs"],
        query: "timeout",
        filters: [{ field: "service_name", op: "=", value: "checkout" }],
      },
      "logs",
    );

    expect(filters).toEqual({ service: "checkout", text: "timeout" });
  });
});
