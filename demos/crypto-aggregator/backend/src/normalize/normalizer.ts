import { randomUUID } from "node:crypto";
import { EventEmitter } from "node:events";
import { logs, SeverityNumber } from "@opentelemetry/api-logs";
import type { PriceEvent } from "../generated/pipeline.PriceEvent.v1.js";
import type { TxEvent } from "../generated/pipeline.TxEvent.v1.js";

// Re-export for convenience
export type { PriceEvent, TxEvent };

import type { RawPriceEvent as DexRawPrice } from "../ingest/dexpaprika.js";
import type { RawPriceEvent as CoinbaseRawPrice } from "../ingest/coinbase.js";
import type { RawTxEvent } from "../ingest/blockchain.js";

type RawPrice = DexRawPrice | CoinbaseRawPrice;

const logger = logs.getLogger("crypto-demo-pipeline", "0.1.0");

/**
 * Normalizer subscribes to raw ingest events from the EventEmitter and emits
 * typed PriceEvent and TxEvent objects validated against the modelable schema.
 * Any event that fails validation is counted as an error.
 */
export function startNormalizer(
  emitter: EventEmitter
): { errorCount: () => number } {
  let errors = 0;

  emitter.on("raw_price", (raw: RawPrice) => {
    try {
      if (
        typeof raw.asset !== "string" ||
        typeof raw.price_usd !== "number" ||
        !isFinite(raw.price_usd) ||
        raw.price_usd <= 0
      ) {
        errors++;
        logger.emit({
          severityNumber: SeverityNumber.WARN,
          severityText: "WARN",
          body: "Price event failed validation",
          attributes: {
            "normalizer.source": raw.source,
            "normalizer.asset": raw.asset,
          },
        });
        return;
      }

      const event: PriceEvent = {
        event_id: randomUUID(),
        asset: raw.asset.toUpperCase(),
        chain: "chain" in raw ? raw.chain : "ethereum",
        price_usd: raw.price_usd,
        source: raw.source === "dexpaprika" ? "DexPaprika" : "Coinbase",
        ts_unix_ms: raw.ts_unix_ms,
      };
      emitter.emit("price_event", event);

      // Update BTC price for blockchain adapter value estimation
      if (event.asset === "BTC") {
        emitter.emit("btc_price_update", event.price_usd);
      }
    } catch (err) {
      errors++;
      logger.emit({
        severityNumber: SeverityNumber.ERROR,
        severityText: "ERROR",
        body: "Unexpected error normalizing price event",
        attributes: { "error": String(err) },
      });
    }
  });

  emitter.on("raw_tx", (raw: RawTxEvent) => {
    try {
      if (
        typeof raw.tx_hash !== "string" ||
        typeof raw.value_usd !== "number" ||
        !isFinite(raw.value_usd)
      ) {
        errors++;
        logger.emit({
          severityNumber: SeverityNumber.WARN,
          severityText: "WARN",
          body: "Tx event failed validation",
          attributes: { "normalizer.tx_hash": raw.tx_hash },
        });
        return;
      }

      const event: TxEvent = {
        tx_hash: raw.tx_hash,
        value_usd: raw.value_usd,
        block_height: raw.block_height,
        ts_unix_ms: raw.ts_unix_ms,
      };
      emitter.emit("tx_event", event);
    } catch (err) {
      errors++;
      logger.emit({
        severityNumber: SeverityNumber.ERROR,
        severityText: "ERROR",
        body: "Unexpected error normalizing tx event",
        attributes: { "error": String(err) },
      });
    }
  });

  emitter.on("ingest_error", () => {
    errors++;
  });

  return { errorCount: () => errors };
}
