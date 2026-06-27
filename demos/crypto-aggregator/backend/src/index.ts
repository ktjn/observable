import { EventEmitter } from "node:events";
import { startDexPaprikaIngest } from "./ingest/dexpaprika.js";
import { startCoinbaseIngest } from "./ingest/coinbase.js";
import { startBlockchainIngest } from "./ingest/blockchain.js";
import { startNormalizer } from "./normalize/normalizer.js";
import { startCorrelator } from "./correlate/correlator.js";
import { startOtelSetup } from "./otel/setup.js";
import { startServer } from "./server.js";
import { logs } from "@opentelemetry/api-logs";

const emitter = new EventEmitter();
emitter.setMaxListeners(50);

// Pipeline stages
const { errorCount } = startNormalizer(emitter);
const { latestLagMs, bufferSize, bufferCapacity } = startCorrelator(emitter);

// Track ingest rate for /metrics endpoint
let ingestWindow = 0;
let windowStart = Date.now();
emitter.on("raw_price", () => ingestWindow++);
emitter.on("raw_tx", () => ingestWindow++);
const ingestTotal = () => {
  const elapsed = (Date.now() - windowStart) / 1_000;
  const rate = elapsed > 0 ? ingestWindow / elapsed : 0;
  ingestWindow = 0;
  windowStart = Date.now();
  return rate;
};

const stats = { errorCount, latestLagMs, bufferSize, bufferCapacity, ingestTotal };

// OTel instrumentation — initialise traces, logs, and metrics before adapters start.
startOtelSetup(emitter, stats);

// Startup log record
const startupLogger = logs.getLogger("crypto-demo-pipeline", "0.1.0");
startupLogger.emit({
  severityText: "INFO",
  body: "crypto-demo-pipeline started",
  attributes: {
    "otel.endpoint": process.env.OTEL_EXPORTER_OTLP_ENDPOINT ?? "http://localhost:4317",
    "service.name": process.env.OTEL_SERVICE_NAME ?? "crypto-demo-pipeline",
  },
});

// Start all three ingest adapters
const stopDex = startDexPaprikaIngest(emitter);
const stopCoinbase = startCoinbaseIngest(emitter);
const stopBlockchain = startBlockchainIngest(emitter);

// HTTP server (SSE + metrics + health)
startServer(emitter, stats);

// Graceful shutdown
const shutdown = () => {
  stopDex();
  stopCoinbase();
  stopBlockchain();
  process.exit(0);
};
process.on("SIGTERM", shutdown);
process.on("SIGINT", shutdown);
