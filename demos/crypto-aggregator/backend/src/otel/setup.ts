import { metrics } from "@opentelemetry/api";
import { logs } from "@opentelemetry/api-logs";
import {
  MeterProvider,
  PeriodicExportingMetricReader,
} from "@opentelemetry/sdk-metrics";
import { OTLPMetricExporter } from "@opentelemetry/exporter-metrics-otlp-grpc";
import { NodeTracerProvider, BatchSpanProcessor } from "@opentelemetry/sdk-trace-node";
import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-grpc";
import { LoggerProvider, BatchLogRecordProcessor } from "@opentelemetry/sdk-logs";
import { OTLPLogExporter } from "@opentelemetry/exporter-logs-otlp-grpc";
import { resourceFromAttributes } from "@opentelemetry/resources";
import {
  ATTR_SERVICE_NAME,
  ATTR_SERVICE_VERSION,
} from "@opentelemetry/semantic-conventions";
import type { ExportResult } from "@opentelemetry/core";
import { EventEmitter } from "node:events";
import { hostname } from "node:os";
import { randomUUID } from "node:crypto";

const EXPORT_INTERVAL_MS = 5_000;
// Declare stale after 3× export interval without a successful export.
const STALE_THRESHOLD_MS = EXPORT_INTERVAL_MS * 3;

export interface PipelineStats {
  errorCount: () => number;
  latestLagMs: () => number;
  bufferSize: () => number;
  bufferCapacity: number;
}

// ---------------------------------------------------------------------------
// Observable connection status — derived from live OTLP export results
// ---------------------------------------------------------------------------

type ObservableStatus = "Ok" | "Degraded" | "Offline";

let _status: ObservableStatus = "Offline"; // safe default until first export
let _lastOkMs = 0;

function recordExportResult(result: ExportResult): void {
  if (result.code === 0 /* ExportResultCode.SUCCESS */) {
    _status = "Ok";
    _lastOkMs = Date.now();
  } else {
    const msg = (result.error?.message ?? "").toLowerCase();
    const isConnection =
      msg.includes("econnrefused") ||
      msg.includes("enotfound") ||
      msg.includes("econnreset") ||
      msg.includes("etimedout") ||
      msg.includes("unavailable") ||
      msg.includes("failed to connect") ||
      msg.includes("connection refused");
    _status = isConnection ? "Offline" : "Degraded";
  }
}

/** Returns the current Observable ingestion connection status. */
export function getObservableStatus(): ObservableStatus {
  if (_status === "Ok" && Date.now() - _lastOkMs > STALE_THRESHOLD_MS) {
    return "Degraded";
  }
  return _status;
}

// ---------------------------------------------------------------------------
// Helper: wrap an exporter's export() to intercept ExportResult callbacks
// ---------------------------------------------------------------------------

function wrapExporter<T extends { export: (items: Parameters<T["export"]>[0], cb: (result: ExportResult) => void) => void }>(
  exporter: T,
): T {
  const original = exporter.export.bind(exporter);
  (exporter as { export: typeof original }).export = (items, cb) => {
    original(items, (result) => {
      recordExportResult(result);
      cb(result);
    });
  };
  return exporter;
}

// ---------------------------------------------------------------------------
// Initialise all three OTel SDK signal types
// ---------------------------------------------------------------------------

/**
 * Initialises the full OpenTelemetry SDK (traces, logs, metrics) and registers
 * pipeline-health gauges. Call once at startup — must be called before any
 * tracer or logger is obtained via the @opentelemetry/api / @opentelemetry/api-logs
 * APIs so that the global providers are installed first.
 */
export function startOtelSetup(
  emitter: EventEmitter,
  stats: PipelineStats,
): void {
  const endpoint =
    process.env.OTEL_EXPORTER_OTLP_ENDPOINT ?? "http://localhost:4317";

  const headers: Record<string, string> = {};
  const rawHeaders = process.env.OTEL_EXPORTER_OTLP_HEADERS ?? "";
  for (const pair of rawHeaders.split(",")) {
    const [k, v] = pair.split("=");
    if (k && v) headers[k.trim()] = v.trim();
  }

  const resource = resourceFromAttributes({
    [ATTR_SERVICE_NAME]:
      process.env.OTEL_SERVICE_NAME ?? "crypto-demo-pipeline",
    [ATTR_SERVICE_VERSION]: "0.1.0",
    "host.name": hostname(),
    "service.instance.id": randomUUID(),
    "process.pid": process.pid,
  });

  // --- Traces -------------------------------------------------------------
  const traceExporter = wrapExporter(new OTLPTraceExporter());
  const tracerProvider = new NodeTracerProvider({
    resource,
    spanProcessors: [new BatchSpanProcessor(traceExporter)],
  });
  tracerProvider.register();

  // --- Logs ---------------------------------------------------------------
  const logExporter = wrapExporter(new OTLPLogExporter());
  const loggerProvider = new LoggerProvider({
    resource,
    processors: [new BatchLogRecordProcessor({ exporter: logExporter })],
  });
  logs.setGlobalLoggerProvider(loggerProvider);

  // --- Metrics ------------------------------------------------------------
  const metricExporter = wrapExporter(
    new OTLPMetricExporter({ url: endpoint, headers }),
  );
  const meterProvider = new MeterProvider({
    resource,
    readers: [
      new PeriodicExportingMetricReader({
        exporter: metricExporter,
        exportIntervalMillis: EXPORT_INTERVAL_MS,
      }),
    ],
  });
  metrics.setGlobalMeterProvider(meterProvider);

  // --- Pipeline-health gauges ---------------------------------------------
  const meter = metrics.getMeter("crypto-demo-pipeline", "0.1.0");

  let ingestTotal = 0;
  let windowStart = Date.now();
  emitter.on("raw_price", () => ingestTotal++);
  emitter.on("raw_tx", () => ingestTotal++);

  meter.createObservableGauge("pipeline.ingest_rate", {
    description: "Events received per second across all ingest adapters",
    unit: "events/s",
  }).addCallback((result) => {
    const now = Date.now();
    const elapsed = (now - windowStart) / 1_000;
    result.observe(elapsed > 0 ? ingestTotal / elapsed : 0);
    ingestTotal = 0;
    windowStart = now;
  });

  meter.createObservableGauge("pipeline.correlation_lag_ms", {
    description: "Latest lag between matched price and tx event",
    unit: "ms",
  }).addCallback((result) => {
    result.observe(stats.latestLagMs());
  });

  meter.createObservableGauge("pipeline.buffer_fill_ratio", {
    description: "Correlator price-window fill level (0–1)",
    unit: "1",
  }).addCallback((result) => {
    result.observe(stats.bufferSize() / stats.bufferCapacity);
  });

  meter.createObservableCounter("pipeline.error_count", {
    description: "Cumulative normalisation and correlation errors since startup",
    unit: "errors",
  }).addCallback((result) => {
    result.observe(stats.errorCount());
  });
}
