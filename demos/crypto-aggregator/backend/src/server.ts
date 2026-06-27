import express, { Request, Response } from "express";
import cors from "cors";
import { EventEmitter } from "node:events";
import type { PriceEvent } from "./normalize/normalizer.js";
import type { CorrelatedEvent } from "./correlate/correlator.js";
import type { TxEvent } from "./generated/pipeline.TxEvent.v1.js";
import type { PipelineMetrics } from "./generated/pipeline.PipelineMetrics.v1.js";
import { getObservableStatus } from "./otel/setup.js";
import { randomUUID } from "node:crypto";

interface PipelineStats {
  errorCount: () => number;
  latestLagMs: () => number;
  bufferSize: () => number;
  bufferCapacity: number;
  ingestTotal: () => number;
}

const SSE_HEARTBEAT_MS = 15_000;

/**
 * Creates and starts the Express HTTP server.
 *
 * Endpoints:
 *   GET /events        — SSE stream of { type, data } events (price, tx, correlated)
 *   GET /metrics       — JSON snapshot of PipelineMetrics (polled by the UI)
 *   GET /health        — liveness probe
 */
export function startServer(
  emitter: EventEmitter,
  stats: PipelineStats
): void {
  const app = express();
  const PORT = parseInt(process.env.PORT ?? "3100", 10);

  app.use(cors());
  app.use(express.json());

  // --- SSE stream -------------------------------------------------------
  app.get("/events", (req: Request, res: Response) => {
    res.setHeader("Content-Type", "text/event-stream");
    res.setHeader("Cache-Control", "no-cache");
    res.setHeader("Connection", "keep-alive");
    res.flushHeaders();

    const send = (type: string, data: unknown) => {
      res.write(`event: ${type}\n`);
      res.write(`data: ${JSON.stringify(data)}\n\n`);
    };

    const onPrice = (e: PriceEvent) => send("price", e);
    const onTx = (e: TxEvent) => send("tx", e);
    const onCorrelated = (e: CorrelatedEvent) => send("correlated", e);

    emitter.on("price_event", onPrice);
    emitter.on("tx_event", onTx);
    emitter.on("correlated_event", onCorrelated);

    const heartbeat = setInterval(() => {
      res.write(": heartbeat\n\n");
    }, SSE_HEARTBEAT_MS);

    req.on("close", () => {
      clearInterval(heartbeat);
      emitter.off("price_event", onPrice);
      emitter.off("tx_event", onTx);
      emitter.off("correlated_event", onCorrelated);
    });
  });

  // --- Metrics JSON snapshot --------------------------------------------
  app.get("/metrics", (_req: Request, res: Response) => {
    const snapshot: PipelineMetrics = {
      snapshot_id: randomUUID(),
      ingest_rate: stats.ingestTotal(),
      correlation_lag_ms: stats.latestLagMs(),
      buffer_fill_ratio: stats.bufferSize() / stats.bufferCapacity,
      exporter_latency_ms: 0, // filled in by OTel module via emitter
      error_count: stats.errorCount(),
      observable_status: getObservableStatus(),
      ts_unix_ms: Date.now(),
    };
    res.json(snapshot);
  });

  // --- Liveness probe ---------------------------------------------------
  app.get("/health", (_req: Request, res: Response) => {
    res.json({ status: "ok" });
  });

  app.listen(PORT, () => {
    console.log(`crypto-demo backend listening on :${PORT}`);
  });
}
