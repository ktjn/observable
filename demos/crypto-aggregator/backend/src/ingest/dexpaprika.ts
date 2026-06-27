import { EventEmitter } from "node:events";
import { trace, SpanStatusCode } from "@opentelemetry/api";
import { logs, SeverityNumber } from "@opentelemetry/api-logs";

export interface RawPriceEvent {
  source: "dexpaprika";
  asset: string;
  chain: string;
  price_usd: number;
  ts_unix_ms: number;
}

const BASE_URL =
  process.env.DEXPAPRIKA_BASE_URL ?? "https://api.dexpaprika.com";

/** Tokens to poll: [chain, token_address, symbol] */
const TOKENS: [string, string, string][] = [
  ["ethereum", "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599", "WBTC"],
  ["ethereum", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "WETH"],
  ["solana",   "So11111111111111111111111111111111111111112", "SOL"],
];

const POLL_INTERVAL_MS = 5_000;

interface TokenResponse {
  symbol: string;
  chain: string;
  summary: { price_usd: number };
}

const tracer = trace.getTracer("crypto-demo-pipeline", "0.1.0");
const logger = logs.getLogger("crypto-demo-pipeline", "0.1.0");

/**
 * Polls the DexPaprika REST API for token prices and emits raw price events.
 * DexPaprika does not offer an SSE/WebSocket stream; REST polling is the
 * supported integration method.
 */
export function startDexPaprikaIngest(emitter: EventEmitter): () => void {
  let stopped = false;

  async function pollToken(chain: string, address: string, symbol: string) {
    const span = tracer.startSpan("dexpaprika.poll_token", {
      attributes: {
        "dexpaprika.chain": chain,
        "dexpaprika.symbol": symbol,
        "dexpaprika.address": address,
      },
    });
    try {
      const res = await fetch(`${BASE_URL}/networks/${chain}/tokens/${address}`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = (await res.json()) as TokenResponse;
      const price = data.summary?.price_usd ?? 0;
      span.setAttributes({ "dexpaprika.price_usd": price });
      const evt: RawPriceEvent = {
        source: "dexpaprika",
        asset: data.symbol ?? symbol,
        chain: data.chain ?? chain,
        price_usd: price,
        ts_unix_ms: Date.now(),
      };
      emitter.emit("raw_price", evt);
    } catch (err) {
      span.setStatus({ code: SpanStatusCode.ERROR, message: String(err) });
      logger.emit({
        severityNumber: SeverityNumber.WARN,
        severityText: "WARN",
        body: "DexPaprika poll failed",
        attributes: { "dexpaprika.chain": chain, "dexpaprika.symbol": symbol, "error": String(err) },
      });
      emitter.emit("ingest_error", { source: "dexpaprika" });
    } finally {
      span.end();
    }
  }

  async function pollAll() {
    if (stopped) return;
    await Promise.all(TOKENS.map(([chain, addr, sym]) => pollToken(chain, addr, sym)));
    if (!stopped) setTimeout(pollAll, POLL_INTERVAL_MS);
  }

  void pollAll();

  return () => { stopped = true; };
}
