import { useEventStream } from "./hooks/useEventStream";
import type { SourceName, SourceStatus } from "./hooks/useEventStream";
import { PriceTicker } from "./components/PriceTicker";
import { TxList } from "./components/TxList";
import { CorrelationScatter } from "./components/CorrelationScatter";
import { PriceChart } from "./components/PriceChart";
import { LineageDiagram } from "./components/LineageDiagram";
import { PipelineHealth } from "./components/PipelineHealth";

const STATUS_COLORS: Record<SourceStatus, string> = {
  ok: "bg-positive animate-pulse",
  stale: "bg-amber-400",
  offline: "bg-negative",
};

const STATUS_LABEL: Record<SourceStatus, string> = {
  ok: "Live",
  stale: "Stale",
  offline: "Offline",
};

function SourceIndicator({ name, status }: { name: SourceName; status: SourceStatus }) {
  return (
    <div className="flex items-center gap-1.5 text-xs" data-testid={`source-status-${name.toLowerCase()}`}>
      <span className={`inline-block size-2 rounded-full ${STATUS_COLORS[status]}`} title={STATUS_LABEL[status]} />
      <span className="text-muted">{name}</span>
      <span className={status === "ok" ? "text-positive" : status === "stale" ? "text-amber-400" : "text-negative"}>
        {STATUS_LABEL[status]}
      </span>
    </div>
  );
}

function SectionCard({
  title,
  children,
  testId,
}: {
  title: string;
  children: React.ReactNode;
  testId?: string;
}) {
  return (
    <div
      data-testid={testId}
      className="flex flex-col gap-3 rounded-2xl border border-slate-800 bg-card p-4"
    >
      <h2 className="text-sm font-semibold uppercase tracking-widest text-muted">
        {title}
      </h2>
      <div className="min-h-0 flex-1">{children}</div>
    </div>
  );
}

export function App() {
  const { prices, txs, correlations, connected, sourceStatus } = useEventStream();

  return (
    <div className="min-h-screen bg-surface px-4 py-6">
      {/* Header */}
      <header className="mb-6 flex items-center justify-between">
        <div>
          <h1 className="text-xl font-bold tracking-tight">
            Crypto Live-Data Demo
          </h1>
          <p className="text-sm text-muted">
            Powered by DexPaprika · Coinbase · Blockchain.com
          </p>
        </div>
        <div className="flex items-center gap-4">
          <SourceIndicator name="DexPaprika" status={sourceStatus.DexPaprika} />
          <SourceIndicator name="Coinbase" status={sourceStatus.Coinbase} />
          <SourceIndicator name="Blockchain" status={sourceStatus.Blockchain} />
          {!connected && (
            <span className="text-xs text-negative">Reconnecting…</span>
          )}
        </div>
      </header>

      {/* Top grid — shortened by ~1/3 via item count */}
      <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
        <SectionCard title="Live Prices" testId="section-live-prices">
          <PriceTicker prices={prices} />
        </SectionCard>

        <SectionCard title="Live Transactions" testId="section-live-txs">
          <TxList txs={txs} />
        </SectionCard>

        <SectionCard title="Data Lineage" testId="section-lineage">
          <LineageDiagram />
        </SectionCard>
      </div>

      {/* Landscape chart widget */}
      <div className="mt-4">
        <SectionCard title="Charts" testId="section-charts">
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
            <div>
              <p className="mb-1 text-xs font-medium text-muted uppercase tracking-wider">Price over Time</p>
              <div className="h-36">
                <PriceChart prices={prices} />
              </div>
            </div>
            <div>
              <p className="mb-1 text-xs font-medium text-muted uppercase tracking-wider">Correlation Lag over Time</p>
              <div className="h-36">
                <CorrelationScatter correlations={correlations} />
              </div>
            </div>
          </div>
        </SectionCard>
      </div>

      {/* Pipeline health — full width */}
      <div className="mt-4">
        <SectionCard title="Pipeline Health (OTel)" testId="section-pipeline-health">
          <PipelineHealth />
        </SectionCard>
      </div>
    </div>
  );
}
