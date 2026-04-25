import { useQuery } from "@tanstack/react-query";
import { listInfrastructure, InfrastructureEntitySummary } from "../api/infrastructure";

interface Props {
  serviceName: string;
}

export function ServiceInfraPanel({ serviceName }: Props) {
  const { data, isLoading, isError } = useQuery({
    queryKey: ["service-infra", serviceName],
    queryFn: () => listInfrastructure({ service: serviceName }),
  });

  if (isLoading) return <div className="loading-state">Loading infrastructure…</div>;
  if (isError) return <div className="signal-empty">Could not load infrastructure.</div>;
  if (!data?.items.length) {
    return (
      <div className="signal-empty">
        No infrastructure entities observed for this service.
      </div>
    );
  }

  const items = data.items.slice(0, 10);

  return (
    <section className="detail-panel">
      <div className="detail-panel-header">
        <div>
          <div className="field-label">Infrastructure</div>
          <h2>Running On</h2>
        </div>
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        {items.map((entity) => (
          <EntityCard key={`${entity.entity_type}/${entity.entity_id}`} entity={entity} />
        ))}
      </div>
    </section>
  );
}

function EntityCard({ entity }: { entity: InfrastructureEntitySummary }) {
  const href = `/infrastructure/${entity.entity_type}/${encodeURIComponent(entity.entity_id)}`;
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 12,
        padding: "6px 0",
        borderBottom: "1px solid var(--color-border, #e2e8f0)",
      }}
    >
      <span className="status info" style={{ minWidth: 72, textAlign: "center" }}>
        {entity.entity_type}
      </span>
      <a href={href} style={{ flex: 1, fontWeight: 500 }}>
        {entity.display_name}
      </a>
      <HealthDot state={entity.health_state} />
      {entity.cpu_usage !== null && (
        <span style={{ fontSize: 12, color: "var(--color-text-muted, #718096)" }}>
          CPU {Math.round(entity.cpu_usage * 100)}%
        </span>
      )}
      {entity.memory_usage !== null && (
        <span style={{ fontSize: 12, color: "var(--color-text-muted, #718096)" }}>
          Mem {Math.round(entity.memory_usage * 100)}%
        </span>
      )}
    </div>
  );
}

function HealthDot({ state }: { state: InfrastructureEntitySummary["health_state"] }) {
  const color =
    state === "breach" ? "#e53e3e" : state === "watch" ? "#d69e2e" : "#38a169";
  return (
    <span
      aria-label={state}
      style={{
        display: "inline-block",
        width: 8,
        height: 8,
        borderRadius: "50%",
        background: color,
        flexShrink: 0,
      }}
    />
  );
}
