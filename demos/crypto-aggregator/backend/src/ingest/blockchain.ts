import WebSocket from "ws";
import { EventEmitter } from "node:events";
import { trace, SpanStatusCode } from "@opentelemetry/api";
import { logs, SeverityNumber } from "@opentelemetry/api-logs";

export interface RawTxEvent {
  source: "blockchain";
  tx_hash: string;
  value_usd: number;
  block_height?: number;
  ts_unix_ms: number;
}

const WS_URL =
  process.env.BLOCKCHAIN_WS_URL ?? "wss://ws.blockchain.info/inv";
const REST_URL =
  process.env.BLOCKCHAIN_REST_URL ??
  "https://blockchain.info/unconfirmed-transactions?format=json";

const REST_POLL_MS = 15_000;
const WS_QUICK_RECONNECT_MS = 5_000;
const WS_RETRY_FROM_REST_MS = 60_000;
const SEEN_HASH_CAP = 2_000;

const tracer = trace.getTracer("crypto-demo-pipeline", "0.1.0");
const logger = logs.getLogger("crypto-demo-pipeline", "0.1.0");

interface UnconfirmedTx {
  hash?: string;
  out?: Array<{ value?: number }>;
  block_height?: number;
}

/**
 * Connects to the Blockchain.com WebSocket feed (primary) and falls back to
 * REST polling of the unconfirmed-transactions endpoint when the WebSocket is
 * unavailable. Automatically switches back to WebSocket once it recovers.
 *
 * State machine:
 *   ws-connecting → ws-live        (first message received)
 *   ws-connecting → rest-polling   (error or close before any message)
 *   ws-live       → ws-connecting  (close — quick 5 s reconnect)
 *   ws-live       → rest-polling   (error — 60 s WS retry)
 *   rest-polling  → ws-connecting  (WS retry fires every 60 s)
 *
 * WebSocket API docs: https://www.blockchain.com/api/api_websocket
 * REST API docs:      https://www.blockchain.com/api/blockchain_api
 */
export function startBlockchainIngest(emitter: EventEmitter): () => void {
  // Approximate BTC/USD price for value estimation
  let btcPriceUsd = 60_000;
  emitter.on("btc_price_update", (price: number) => {
    btcPriceUsd = price;
  });

  type Mode = "ws-connecting" | "ws-live" | "rest-polling";
  let mode: Mode = "ws-connecting";
  let stopped = false;

  let ws: WebSocket | null = null;
  let wsMessageReceived = false;
  let restPollTimer: NodeJS.Timeout | null = null;
  let wsRetryTimer: NodeJS.Timeout | null = null;

  // Dedup set for REST polling (WS streams only new events, no dedup needed)
  const seenHashes = new Set<string>();

  function emitTx(tx: UnconfirmedTx): void {
    const satoshis = tx.out?.reduce((sum, o) => sum + (o.value ?? 0), 0) ?? 0;
    const valueUsd = (satoshis / 1e8) * btcPriceUsd;
    if (valueUsd < 1) return; // filter dust
    emitter.emit("raw_tx", {
      source: "blockchain",
      tx_hash: tx.hash ?? crypto.randomUUID(),
      value_usd: valueUsd,
      block_height: tx.block_height,
      ts_unix_ms: Date.now(),
    } satisfies RawTxEvent);
  }

  // ---------------------------------------------------------------------------
  // REST polling
  // ---------------------------------------------------------------------------

  async function pollRest(): Promise<void> {
    if (stopped || mode !== "rest-polling") return;

    const span = tracer.startSpan("blockchain.poll_unconfirmed");
    try {
      const res = await fetch(REST_URL);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = (await res.json()) as { txs?: UnconfirmedTx[] };
      let newCount = 0;
      for (const tx of data.txs ?? []) {
        if (!tx.hash || seenHashes.has(tx.hash)) continue;
        seenHashes.add(tx.hash);
        if (seenHashes.size > SEEN_HASH_CAP) {
          // Evict oldest entry (insertion-order in Set)
          seenHashes.delete(seenHashes.values().next().value!);
        }
        emitTx(tx);
        newCount++;
      }
      span.setAttributes({ "blockchain.new_txs": newCount });
    } catch (err) {
      span.setStatus({ code: SpanStatusCode.ERROR, message: String(err) });
      logger.emit({
        severityNumber: SeverityNumber.WARN,
        severityText: "WARN",
        body: "Blockchain REST poll failed",
        attributes: { error: String(err) },
      });
      emitter.emit("ingest_error", { source: "blockchain" });
    } finally {
      span.end();
    }

    if (!stopped && mode === "rest-polling") {
      restPollTimer = setTimeout(() => void pollRest(), REST_POLL_MS);
    }
  }

  function startRest(): void {
    if (restPollTimer !== null) return; // already running
    void pollRest();
  }

  function stopRest(): void {
    if (restPollTimer !== null) {
      clearTimeout(restPollTimer);
      restPollTimer = null;
    }
  }

  // ---------------------------------------------------------------------------
  // WebSocket
  // ---------------------------------------------------------------------------

  function connectWs(): void {
    if (stopped) return;
    mode = "ws-connecting";
    wsMessageReceived = false;

    ws = new WebSocket(WS_URL);

    ws.on("open", () => {
      logger.emit({
        severityNumber: SeverityNumber.INFO,
        severityText: "INFO",
        body: "Blockchain.com WebSocket connected",
      });
      ws!.send(JSON.stringify({ op: "unconfirmed_sub" }));
    });

    ws.on("message", (data: Buffer) => {
      try {
        const msg = JSON.parse(data.toString()) as {
          op?: string;
          x?: UnconfirmedTx;
        };
        if (msg.op !== "utx" || !msg.x) return;

        // First real message — switch to ws-live and stop REST fallback
        if (!wsMessageReceived) {
          wsMessageReceived = true;
          if (mode !== "ws-live") {
            mode = "ws-live";
            stopRest();
            if (wsRetryTimer !== null) {
              clearTimeout(wsRetryTimer);
              wsRetryTimer = null;
            }
            logger.emit({
              severityNumber: SeverityNumber.INFO,
              severityText: "INFO",
              body: "Blockchain.com WebSocket live — switched from REST fallback",
            });
          }
        }

        emitTx(msg.x);
      } catch {
        emitter.emit("ingest_error", { source: "blockchain" });
      }
    });

    ws.on("error", (err) => {
      logger.emit({
        severityNumber: SeverityNumber.WARN,
        severityText: "WARN",
        body: "Blockchain.com WebSocket error — falling back to REST",
        attributes: { error: String(err) },
      });
      emitter.emit("ingest_error", { source: "blockchain" });
      // Switch to REST; schedule WS retry
      mode = "rest-polling";
      startRest();
      scheduleWsRetry();
    });

    ws.on("close", () => {
      if (stopped) return;
      if (wsMessageReceived) {
        // Was working — quick reconnect
        logger.emit({
          severityNumber: SeverityNumber.INFO,
          severityText: "INFO",
          body: "Blockchain.com WebSocket closed — reconnecting in 5 s",
        });
        setTimeout(connectWs, WS_QUICK_RECONNECT_MS);
      } else {
        // Closed without ever delivering data (e.g., 502 upgrade failure)
        logger.emit({
          severityNumber: SeverityNumber.WARN,
          severityText: "WARN",
          body: "Blockchain.com WebSocket closed before first message — falling back to REST",
        });
        mode = "rest-polling";
        startRest();
        scheduleWsRetry();
      }
    });
  }

  function scheduleWsRetry(): void {
    if (wsRetryTimer !== null || stopped) return;
    wsRetryTimer = setTimeout(() => {
      wsRetryTimer = null;
      if (!stopped && mode === "rest-polling") {
        logger.emit({
          severityNumber: SeverityNumber.INFO,
          severityText: "INFO",
          body: "Blockchain.com WebSocket retry attempt",
        });
        connectWs();
      }
    }, WS_RETRY_FROM_REST_MS);
  }

  // Start with WebSocket attempt
  connectWs();

  return () => {
    stopped = true;
    stopRest();
    if (wsRetryTimer !== null) clearTimeout(wsRetryTimer);
    ws?.close();
  };
}

