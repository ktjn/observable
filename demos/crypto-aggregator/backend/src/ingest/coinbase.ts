import WebSocket from "ws";
import { EventEmitter } from "node:events";
import { logs, SeverityNumber } from "@opentelemetry/api-logs";

export interface RawPriceEvent {
  source: "coinbase";
  asset: string;
  chain: string;
  price_usd: number;
  ts_unix_ms: number;
}

// BTC and ETH product IDs to subscribe to
const PRODUCTS = ["BTC-USD", "ETH-USD", "SOL-USD"];

const logger = logs.getLogger("crypto-demo-pipeline", "0.1.0");

/**
 * Connects to the Coinbase Advanced Trade WebSocket feed and emits price
 * events for the configured product IDs.
 */
export function startCoinbaseIngest(emitter: EventEmitter): () => void {
  const WS_URL =
    process.env.COINBASE_WS_URL ?? "wss://advanced-trade-ws.coinbase.com";

  let ws: WebSocket | null = null;
  let reconnectTimeout: NodeJS.Timeout | null = null;

  function connect() {
    ws = new WebSocket(WS_URL);

    ws.on("open", () => {
      logger.emit({
        severityNumber: SeverityNumber.INFO,
        severityText: "INFO",
        body: "Coinbase WebSocket connected",
        attributes: { "coinbase.products": PRODUCTS.join(",") },
      });
      ws!.send(
        JSON.stringify({
          type: "subscribe",
          product_ids: PRODUCTS,
          channel: "ticker",
        })
      );
    });

    ws.on("message", (data: Buffer) => {
      try {
        const msg = JSON.parse(data.toString()) as {
          channel?: string;
          events?: Array<{
            type?: string;
            tickers?: Array<{
              product_id?: string;
              price?: string;
            }>;
          }>;
        };

        if (msg.channel !== "ticker") return;

        for (const event of msg.events ?? []) {
          for (const ticker of event.tickers ?? []) {
            const [base] = (ticker.product_id ?? "").split("-");
            const evt: RawPriceEvent = {
              source: "coinbase",
              asset: base ?? "UNKNOWN",
              chain: "ethereum", // Coinbase spot prices are chain-agnostic; use ethereum as convention
              price_usd: parseFloat(ticker.price ?? "0"),
              ts_unix_ms: Date.now(),
            };
            emitter.emit("raw_price", evt);
          }
        }
      } catch {
        emitter.emit("ingest_error", { source: "coinbase" });
      }
    });

    ws.on("error", (err) => {
      logger.emit({
        severityNumber: SeverityNumber.WARN,
        severityText: "WARN",
        body: "Coinbase WebSocket error",
        attributes: { "error": String(err) },
      });
      emitter.emit("ingest_error", { source: "coinbase" });
    });

    ws.on("close", () => {
      logger.emit({
        severityNumber: SeverityNumber.INFO,
        severityText: "INFO",
        body: "Coinbase WebSocket closed — reconnecting in 5s",
      });
      reconnectTimeout = setTimeout(connect, 5_000);
    });
  }

  connect();

  return () => {
    if (reconnectTimeout) clearTimeout(reconnectTimeout);
    ws?.close();
  };
}
