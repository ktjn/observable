import { randomUUID } from "node:crypto";
import { EventEmitter } from "node:events";
import { trace } from "@opentelemetry/api";
import { logs, SeverityNumber } from "@opentelemetry/api-logs";
import type { PriceEvent, TxEvent } from "../normalize/normalizer.js";
import type { CorrelatedEvent } from "../generated/pipeline.CorrelatedEvent.v1.js";

export type { CorrelatedEvent };

const WINDOW_MS = 5_000; // 5-second sliding window

interface WindowEntry {
  event: PriceEvent;
  expiresAt: number;
}

const tracer = trace.getTracer("crypto-demo-pipeline", "0.1.0");
const logger = logs.getLogger("crypto-demo-pipeline", "0.1.0");

/**
 * The Correlator maintains a sliding time-window of recent PriceEvents keyed
 * by asset. When a TxEvent arrives it is paired with the most-recent PriceEvent
 * for the primary asset (BTC by default) within the window, and a
 * CorrelatedEvent is emitted. The lag is the milliseconds between the price
 * tick and the transaction.
 *
 * Returns a stats accessor used by the OTel metrics module.
 */
export function startCorrelator(emitter: EventEmitter): {
  latestLagMs: () => number;
  bufferSize: () => number;
  bufferCapacity: number;
} {
  const BUFFER_CAPACITY = 200;
  const priceWindow = new Map<string, WindowEntry>();
  let latestLagMs = 0;

  function evict() {
    const now = Date.now();
    for (const [key, entry] of priceWindow) {
      if (entry.expiresAt < now) priceWindow.delete(key);
    }
  }

  emitter.on("price_event", (event: PriceEvent) => {
    evict();
    priceWindow.set(event.asset, {
      event,
      expiresAt: Date.now() + WINDOW_MS,
    });
  });

  emitter.on("tx_event", (tx: TxEvent) => {
    const span = tracer.startSpan("correlator.correlate_tx", {
      attributes: {
        "correlator.tx_hash": tx.tx_hash,
        "correlator.value_usd": tx.value_usd,
        "correlator.window_size": priceWindow.size,
      },
    });

    evict();

    // Match against BTC first (most transactions are BTC), then any asset
    const entry =
      priceWindow.get("BTC") ??
      (priceWindow.size > 0
        ? [...priceWindow.values()].sort(
            (a, b) => b.event.ts_unix_ms - a.event.ts_unix_ms
          )[0]
        : undefined);

    if (!entry) {
      span.end();
      return;
    }

    const lagMs = Math.max(0, tx.ts_unix_ms - entry.event.ts_unix_ms);
    latestLagMs = lagMs;

    const correlated: CorrelatedEvent = {
      correlation_id: randomUUID(),
      asset: entry.event.asset,
      tx_hash: tx.tx_hash,
      price_usd: entry.event.price_usd,
      lag_ms: lagMs,
      price_source: entry.event.source,
      ts_unix_ms: Date.now(),
    };

    span.setAttributes({
      "correlator.asset": correlated.asset,
      "correlator.lag_ms": lagMs,
      "correlator.price_source": correlated.price_source,
    });
    span.end();

    logger.emit({
      severityNumber: SeverityNumber.DEBUG,
      severityText: "DEBUG",
      body: "Correlated price+tx",
      attributes: {
        "correlator.asset": correlated.asset,
        "correlator.tx_hash": correlated.tx_hash,
        "correlator.lag_ms": lagMs,
        "correlator.price_usd": correlated.price_usd,
      },
    });

    emitter.emit("correlated_event", correlated);
  });

  return {
    latestLagMs: () => latestLagMs,
    bufferSize: () => priceWindow.size,
    bufferCapacity: BUFFER_CAPACITY,
  };
}
