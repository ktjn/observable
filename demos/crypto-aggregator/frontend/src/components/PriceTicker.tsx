import type { PriceEvent } from "../generated/pipeline.PriceEvent.v1";

interface Props {
  prices: PriceEvent[];
}

const SOURCE_BADGE: Record<string, string> = {
  DexPaprika: "bg-violet-800 text-violet-200",
  Coinbase: "bg-blue-800 text-blue-200",
};

export function PriceTicker({ prices }: Props) {
  return (
    <div
      data-testid="price-ticker"
      className="flex flex-col gap-1 overflow-hidden"
    >
      {prices.length === 0 && (
        <p className="text-sm text-muted animate-pulse">Waiting for price events…</p>
      )}
      {prices.slice(0, 13).map((p) => (
        <div
          key={p.event_id}
          className="flex items-center justify-between rounded-lg bg-card px-3 py-1.5 text-sm"
        >
          <span className="font-mono font-semibold text-accent-2">
            {p.asset}
            <span className="ml-1 text-xs text-muted">/{p.chain}</span>
          </span>
          <span className="font-mono text-positive">
            ${p.price_usd.toLocaleString(undefined, { maximumFractionDigits: 4 })}
          </span>
          <span
            className={`rounded px-1.5 py-0.5 text-xs font-medium ${SOURCE_BADGE[p.source] ?? "bg-slate-700 text-slate-300"}`}
          >
            {p.source}
          </span>
        </div>
      ))}
    </div>
  );
}
