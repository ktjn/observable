import { NlqPanel } from "../features/nlq/NlqPanel";

export default function NlqQueryPage() {
  return (
    <div className="p-6 max-w-4xl mx-auto space-y-4">
      <div>
        <h1 className="text-xl font-semibold">Natural Language Query</h1>
        <p className="text-sm text-[var(--text-muted)] mt-1">
          Ask questions about your metrics in plain English. Results are advisory only — not
          suitable for billing, SLA enforcement, or regulatory compliance.
        </p>
      </div>
      <NlqPanel />
    </div>
  );
}
