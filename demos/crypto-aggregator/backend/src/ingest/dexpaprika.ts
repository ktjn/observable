import { EventEmitter } from "node:events";

export interface RawPriceEvent {
  source: "dexpaprika";
  asset: string;
  chain: string;
  price_usd: number;
  ts_unix_ms: number;
}

const BASE_URL = "https://api.dexpaprika.com";

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

/**
 * Polls the DexPaprika REST API for token prices and emits raw price events.
 * DexPaprika does not offer an SSE/WebSocket stream; REST polling is the
 * supported integration method.
 */
export function startDexPaprikaIngest(emitter: EventEmitter): () => void {
  let stopped = false;

  async function pollToken(chain: string, address: string, symbol: string) {
    try {
      const res = await fetch(`${BASE_URL}/networks/${chain}/tokens/${address}`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = (await res.json()) as TokenResponse;
      const evt: RawPriceEvent = {
        source: "dexpaprika",
        asset: data.symbol ?? symbol,
        chain: data.chain ?? chain,
        price_usd: data.summary?.price_usd ?? 0,
        ts_unix_ms: Date.now(),
      };
      emitter.emit("raw_price", evt);
    } catch {
      emitter.emit("ingest_error", { source: "dexpaprika" });
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
