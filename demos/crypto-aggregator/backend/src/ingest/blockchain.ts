import WebSocket from "ws";
import { EventEmitter } from "node:events";

export interface RawTxEvent {
  source: "blockchain";
  tx_hash: string;
  value_usd: number;
  block_height?: number;
  ts_unix_ms: number;
}

/**
 * Connects to the Blockchain.com WebSocket feed and emits unconfirmed
 * transaction events. Each event carries the transaction hash and estimated
 * USD value derived from the output total.
 *
 * API docs: https://www.blockchain.com/api/api_websocket
 */
export function startBlockchainIngest(emitter: EventEmitter): () => void {
  const WS_URL =
    process.env.BLOCKCHAIN_WS_URL ?? "wss://ws.blockchain.info/inv";

  let ws: WebSocket | null = null;
  let reconnectTimeout: NodeJS.Timeout | null = null;

  // Approximate BTC/USD price for value estimation (updated when price events arrive)
  let btcPriceUsd = 60_000;
  emitter.on("btc_price_update", (price: number) => {
    btcPriceUsd = price;
  });

  function connect() {
    ws = new WebSocket(WS_URL);

    ws.on("open", () => {
      ws!.send(JSON.stringify({ op: "unconfirmed_sub" }));
    });

    ws.on("message", (data: Buffer) => {
      try {
        const msg = JSON.parse(data.toString()) as {
          op?: string;
          x?: {
            hash?: string;
            out?: Array<{ value?: number }>;
            block_height?: number;
          };
        };

        if (msg.op !== "utx" || !msg.x) return;

        const satoshis =
          msg.x.out?.reduce((sum, o) => sum + (o.value ?? 0), 0) ?? 0;
        const btc = satoshis / 1e8;
        const valueUsd = btc * btcPriceUsd;

        // Filter out dust transactions (< $1)
        if (valueUsd < 1) return;

        const evt: RawTxEvent = {
          source: "blockchain",
          tx_hash: msg.x.hash ?? crypto.randomUUID(),
          value_usd: valueUsd,
          block_height: msg.x.block_height,
          ts_unix_ms: Date.now(),
        };
        emitter.emit("raw_tx", evt);
      } catch {
        emitter.emit("ingest_error", { source: "blockchain" });
      }
    });

    ws.on("error", () => {
      emitter.emit("ingest_error", { source: "blockchain" });
    });

    ws.on("close", () => {
      reconnectTimeout = setTimeout(connect, 5_000);
    });
  }

  connect();

  return () => {
    if (reconnectTimeout) clearTimeout(reconnectTimeout);
    ws?.close();
  };
}
