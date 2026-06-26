import { useState } from "react";

const MODELS = [
  { name: "PriceEvent", file: "/pipeline.PriceEvent.v1.md" },
  { name: "TxEvent", file: "/pipeline.TxEvent.v1.md" },
  { name: "CorrelatedEvent", file: "/pipeline.CorrelatedEvent.v1.md" },
  { name: "PipelineMetrics", file: "/pipeline.PipelineMetrics.v1.md" },
];

/**
 * Displays the modelable-generated lineage as a clickable model selector.
 * Clicking a model name fetches and renders its markdown documentation,
 * giving an inline schema explorer without a separate page.
 */
export function LineageDiagram() {
  const [selected, setSelected] = useState<string | null>(null);
  const [content, setContent] = useState<string>("");
  const [loading, setLoading] = useState(false);

  const select = async (file: string) => {
    if (selected === file) {
      setSelected(null);
      setContent("");
      return;
    }
    setSelected(file);
    setLoading(true);
    try {
      const res = await fetch(file);
      setContent(await res.text());
    } catch {
      setContent("Failed to load schema.");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div data-testid="lineage-diagram" className="space-y-3">
      {/* Pipeline flow */}
      <div className="flex items-center gap-2 flex-wrap text-xs">
        {["DexPaprika", "Coinbase"].map((src) => (
          <span key={src} className="rounded bg-violet-900/50 px-2 py-1 text-violet-300">
            {src}
          </span>
        ))}
        <Arrow />
        <span className="rounded bg-slate-700 px-2 py-1 text-slate-300">Normalizer</span>
        <Arrow />
        <span className="rounded bg-slate-700 px-2 py-1 text-slate-300">Correlator</span>
        <Arrow />
        <div className="flex gap-1">
          <span className="rounded bg-indigo-900/50 px-2 py-1 text-indigo-300">UI</span>
          <span className="rounded bg-cyan-900/50 px-2 py-1 text-cyan-300">OTel</span>
        </div>
      </div>

      {/* Clickable model cards */}
      <p className="text-xs text-muted">Click a model to view schema</p>
      <div className="grid grid-cols-2 gap-2">
        {MODELS.map((m) => (
          <button
            key={m.file}
            data-testid={`lineage-model-${m.name}`}
            onClick={() => void select(m.file)}
            className={`rounded-lg border px-3 py-2 text-left text-sm transition-colors ${
              selected === m.file
                ? "border-accent bg-accent/10 text-accent"
                : "border-slate-700 bg-card text-slate-300 hover:border-accent/50"
            }`}
          >
            {m.name}
          </button>
        ))}
      </div>

      {/* Schema preview */}
      {loading && (
        <p className="text-xs text-muted animate-pulse">Loading schema…</p>
      )}
      {!loading && content && (
        <pre className="max-h-48 overflow-auto rounded-lg bg-card p-3 text-xs text-slate-300 whitespace-pre-wrap">
          {content}
        </pre>
      )}
    </div>
  );
}

function Arrow() {
  return <span className="text-muted">→</span>;
}
