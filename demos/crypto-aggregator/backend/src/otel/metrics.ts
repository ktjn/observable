import { metrics } from "@opentelemetry/api";
import {
  MeterProvider,
  PeriodicExportingMetricReader,
} from "@opentelemetry/sdk-metrics";
import { OTLPMetricExporter } from "@opentelemetry/exporter-metrics-otlp-grpc";
import { Resource } from "@opentelemetry/resources";
import {
  ATTR_SERVICE_NAME,
  ATTR_SERVICE_VERSION,
} from "@opentelemetry/semantic-conventions";
import { EventEmitter } from "node:events";

export interface PipelineStats {
  errorCount: () => number;
  latestLagMs: () => number;
  bufferSize: () => number;
  bufferCapacity: number;
}

/**
 * Initialises the OpenTelemetry metrics SDK and registers pipeline-health
 * gauges/counters. Call once at startup with the shared EventEmitter so
 * throughput can be tracked from ingest events.
 */
export function startOtelMetrics(
  emitter: EventEmitter,
  stats: PipelineStats
): void {
  const endpoint =
    process.env.OTEL_EXPORTER_OTLP_ENDPOINT ?? "http://localhost:4317";

  const headers: Record<string, string> = {};
  const rawHeaders = process.env.OTEL_EXPORTER_OTLP_HEADERS ?? "";
  for (const pair of rawHeaders.split(",")) {
    const [k, v] = pair.split("=");
    if (k && v) headers[k.trim()] = v.trim();
  }

  const exporter = new OTLPMetricExporter({ url: endpoint, headers });

  const resource = new Resource({
    [ATTR_SERVICE_NAME]:
      process.env.OTEL_SERVICE_NAME ?? "crypto-demo-pipeline",
    [ATTR_SERVICE_VERSION]: "0.1.0",
  });

  const provider = new MeterProvider({
    resource,
    readers: [
      new PeriodicExportingMetricReader({
        exporter,
        exportIntervalMillis: 5_000,
      }),
    ],
  });

  metrics.setGlobalMeterProvider(provider);

  const meter = metrics.getMeter("crypto-demo-pipeline", "0.1.0");

  // Throughput counter — incremented on every ingest event
  let ingestTotal = 0;
  let windowStart = Date.now();
  emitter.on("raw_price", () => ingestTotal++);
  emitter.on("raw_tx", () => ingestTotal++);

  // pipeline.ingest_rate — events/s computed over last export interval
  meter.createObservableGauge("pipeline.ingest_rate", {
    description: "Events received per second across all ingest adapters",
    unit: "events/s",
  }).addCallback((result) => {
    const now = Date.now();
    const elapsed = (now - windowStart) / 1_000;
    result.observe(elapsed > 0 ? ingestTotal / elapsed : 0);
    // Reset window
    ingestTotal = 0;
    windowStart = now;
  });

  // pipeline.correlation_lag_ms — rolling average lag between price and tx
  meter.createObservableGauge("pipeline.correlation_lag_ms", {
    description: "Latest lag between matched price and tx event",
    unit: "ms",
  }).addCallback((result) => {
    result.observe(stats.latestLagMs());
  });

  // pipeline.buffer_fill_ratio — correlator ring-buffer utilisation
  meter.createObservableGauge("pipeline.buffer_fill_ratio", {
    description: "Correlator price-window fill level (0–1)",
    unit: "1",
  }).addCallback((result) => {
    result.observe(stats.bufferSize() / stats.bufferCapacity);
  });

  // pipeline.error_count — cumulative normalisation/correlation errors
  meter.createObservableCounter("pipeline.error_count", {
    description: "Cumulative normalisation and correlation errors since startup",
    unit: "errors",
  }).addCallback((result) => {
    result.observe(stats.errorCount());
  });

  // pipeline.exporter_latency_ms — round-trip time measured around each export
  let lastExportLatency = 0;
  const origExport = exporter.export.bind(exporter);
  (exporter as { export: typeof origExport }).export = (items, cb) => {
    const t0 = Date.now();
    origExport(items, (result) => {
      lastExportLatency = Date.now() - t0;
      cb(result);
    });
  };

  meter.createObservableGauge("pipeline.exporter_latency_ms", {
    description: "OTel exporter round-trip latency",
    unit: "ms",
  }).addCallback((result) => {
    result.observe(lastExportLatency);
  });
}
