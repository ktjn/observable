import { EventSource } from "eventsource";
import { EventEmitter } from "node:events";

export interface RawPriceEvent {
  source: "dexpaprika";
  asset: string;
  chain: string;
  price_usd: number;
  ts_unix_ms: number;
}

/**
 * Connects to the DexPaprika SSE price feed and emits raw price events.
 * DexPaprika provides a public SSE endpoint for real-time DEX prices.
 */
export function startDexPaprikaIngest(emitter: EventEmitter): () => void {
  const ENDPOINT =
    process.env.DEXPAPRIKA_SSE_URL ??
    "https://api.dexpaprika.com/v1/prices/stream";

  let es: EventSource | null = null;

  function connect() {
    es = new EventSource(ENDPOINT);

    es.addEventListener("price", (event: MessageEvent) => {
      try {
        const raw = JSON.parse(event.data as string) as {
          token_symbol?: string;
          chain_id?: string;
          price_usd?: number;
        };
        const evt: RawPriceEvent = {
          source: "dexpaprika",
          asset: raw.token_symbol ?? "UNKNOWN",
          chain: raw.chain_id ?? "unknown",
          price_usd: raw.price_usd ?? 0,
          ts_unix_ms: Date.now(),
        };
        emitter.emit("raw_price", evt);
      } catch {
        emitter.emit("ingest_error", { source: "dexpaprika" });
      }
    });

    es.onerror = () => {
      emitter.emit("ingest_error", { source: "dexpaprika" });
      // Reconnect after 5 seconds on error
      setTimeout(connect, 5_000);
    };
  }

  connect();

  return () => es?.close();
}
