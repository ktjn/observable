import type { TxEvent } from "../generated/pipeline.TxEvent.v1";

interface Props {
  txs: TxEvent[];
}

export function TxList({ txs }: Props) {
  return (
    <div data-testid="tx-list" className="flex flex-col gap-1 overflow-hidden">
      {txs.length === 0 && (
        <p className="text-sm text-muted animate-pulse">Waiting for transactions…</p>
      )}
      {txs.slice(0, 8).map((tx) => (
        <div
          key={tx.tx_hash}
          className="flex items-center justify-between rounded-lg bg-card px-3 py-1.5 text-sm"
        >
          <span className="font-mono text-xs text-muted truncate max-w-[140px]">
            {tx.tx_hash.slice(0, 12)}…
          </span>
          <span className="font-mono text-accent">
            ${tx.value_usd.toLocaleString(undefined, { maximumFractionDigits: 2 })}
          </span>
          {tx.block_height != null && (
            <span className="text-xs text-muted">#{tx.block_height}</span>
          )}
        </div>
      ))}
    </div>
  );
}
