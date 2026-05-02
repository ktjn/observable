import { EmptyState } from "../components/ui/empty-state";
import { MetricCard } from "../components/ui/metric-card";

export default function AdminPage() {
  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Operations</div>
          <h1>Admin / Fleet / Billing</h1>
        </div>
      </div>
      <div
        className="grid grid-cols-[repeat(4,minmax(140px,1fr))] gap-3 max-[860px]:grid-cols-2 max-[560px]:grid-cols-1"
        aria-label="Admin summary"
      >
        <MetricCard label="Tenants" value="1" tone="info" />
        <MetricCard label="Fleet" value="local-dev" tone="good" />
        <MetricCard label="Config Drift" value="0" tone="good" />
        <MetricCard label="Billing" value="offline" tone="info" />
      </div>
      <EmptyState
        title="Admin workspace"
        description="Administrative workflows will land here without sharing the service catalog implementation."
        metadata={["Tenant: local-dev", "Environment: dev", "Range: Last 1h"]}
      />
    </section>
  );
}
