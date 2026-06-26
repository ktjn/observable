import { EventEmitter } from "node:events";
import { startDexPaprikaIngest } from "./ingest/dexpaprika.js";
import { startCoinbaseIngest } from "./ingest/coinbase.js";
import { startBlockchainIngest } from "./ingest/blockchain.js";
import { startNormalizer } from "./normalize/normalizer.js";
import { startCorrelator } from "./correlate/correlator.js";
import { startOtelMetrics } from "./otel/metrics.js";
import { startServer } from "./server.js";

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

// OTel instrumentation (must start before ingest adapters)
startOtelMetrics(emitter, stats);

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
