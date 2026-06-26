import { useEffect, useRef, useState } from "react";
import type { PriceEvent } from "../generated/pipeline.PriceEvent.v1";
import type { TxEvent } from "../generated/pipeline.TxEvent.v1";
import type { CorrelatedEvent } from "../generated/pipeline.CorrelatedEvent.v1";

export type StreamEvent =
  | { type: "price"; data: PriceEvent }
  | { type: "tx"; data: TxEvent }
  | { type: "correlated"; data: CorrelatedEvent };

const MAX_ITEMS = 100;

export interface EventStreamState {
  prices: PriceEvent[];
  txs: TxEvent[];
  correlations: CorrelatedEvent[];
  connected: boolean;
}

/**
 * Subscribes to the backend /events SSE stream and maintains rolling buffers
 * of the last MAX_ITEMS events for each type. Reconnects automatically on
 * disconnection.
 */
export function useEventStream(url = "/events"): EventStreamState {
  const [prices, setPrices] = useState<PriceEvent[]>([]);
  const [txs, setTxs] = useState<TxEvent[]>([]);
  const [correlations, setCorrelations] = useState<CorrelatedEvent[]>([]);
  const [connected, setConnected] = useState(false);

  const esRef = useRef<EventSource | null>(null);

  useEffect(() => {
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

    function connect() {
      const es = new EventSource(url);
      esRef.current = es;

      es.addEventListener("open", () => setConnected(true));

      es.addEventListener("price", (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data as string) as PriceEvent;
          setPrices((prev) => [data, ...prev].slice(0, MAX_ITEMS));
        } catch { /* ignore malformed events */ }
      });

      es.addEventListener("tx", (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data as string) as TxEvent;
          setTxs((prev) => [data, ...prev].slice(0, MAX_ITEMS));
        } catch { /* ignore malformed events */ }
      });

      es.addEventListener("correlated", (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data as string) as CorrelatedEvent;
          setCorrelations((prev) => [data, ...prev].slice(0, MAX_ITEMS));
        } catch { /* ignore malformed events */ }
      });

      es.addEventListener("error", () => {
        setConnected(false);
        es.close();
        reconnectTimer = setTimeout(connect, 3_000);
      });
    }

    connect();

    return () => {
      if (reconnectTimer) clearTimeout(reconnectTimer);
      esRef.current?.close();
    };
  }, [url]);

  return { prices, txs, correlations, connected };
}
