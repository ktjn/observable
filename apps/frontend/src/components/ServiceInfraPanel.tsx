import { useQuery } from "@tanstack/react-query";
import { listInfrastructure, InfrastructureEntitySummary } from "../api/infrastructure";
import { Badge } from "./ui/badge";
import { Panel } from "./ui/panel";

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
    <Panel eyebrow="Infrastructure" title="Running On">
      <div className="entity-card-list">
        {items.map((entity) => (
          <EntityCard key={`${entity.entity_type}/${entity.entity_id}`} entity={entity} />
        ))}
      </div>
    </Panel>
  );
}

function EntityCard({ entity }: { entity: InfrastructureEntitySummary }) {
  const href = `/infrastructure/${entity.entity_type}/${encodeURIComponent(entity.entity_id)}`;
  return (
    <div className="entity-card-row">
      <Badge tone="info" className="min-w-[72px] justify-center">
        {entity.entity_type}
      </Badge>
      <a href={href} className="entity-card-link">
        {entity.display_name}
      </a>
      <HealthDot state={entity.health_state} />
      {entity.cpu_usage !== null && (
        <span className="entity-card-metric">
          CPU {Math.round(entity.cpu_usage * 100)}%
        </span>
      )}
      {entity.memory_usage !== null && (
        <span className="entity-card-metric">
          Mem {Math.round(entity.memory_usage * 100)}%
        </span>
      )}
    </div>
  );
}

function HealthDot({ state }: { state: InfrastructureEntitySummary["health_state"] }) {
  const tone = state === "breach" ? "bad" : state === "watch" ? "warn" : "good";
  return <span role="img" aria-label={state} className={`entity-health-dot ${tone}`} />;
}
