import { useQuery } from "@tanstack/react-query";
import { listInfrastructure, type InfrastructureEntitySummary } from "../api/infrastructure";
import { Badge, HealthDot } from "./ui/badge";
import { EmptyState } from "./ui/empty-state";
import { ErrorState } from "./ui/error-state";
import { LoadingState } from "./ui/loading-state";
import { Panel } from "./ui/panel";
import { useTenantContext } from "../hooks/useTenantContext";

interface Props {
  serviceName: string;
}

export function ServiceInfraPanel({ serviceName }: Props) {
  const { tenantId } = useTenantContext();
  const { data, isLoading, isError } = useQuery({
    queryKey: ["service-infra", tenantId, serviceName],
    queryFn: () => listInfrastructure(tenantId, { service: serviceName }),
  });

  if (isLoading) return <LoadingState>Loading infrastructure…</LoadingState>;
if (isError) return <ErrorState title="Could not load infrastructure" />;
if (!data?.items.length) {
    return (
      <EmptyState title="No infrastructure entities" description="No infrastructure entities observed for this service." />
    );
  }

  const items = data.items.slice(0, 10);

  return (
    <Panel eyebrow="Infrastructure" title="Running On">
      <div className="flex flex-col gap-2">
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
    <div className="flex items-center gap-3 border-b border-[var(--border)] py-2 last:border-0">
      <Badge tone="info" className="min-w-[72px] justify-center">
        {entity.entity_type}
      </Badge>
      <a
        href={href}
        className="flex-1 min-w-0 font-[650] text-[var(--text)] no-underline hover:text-[var(--brand-strong)]"
      >
        {entity.display_name}
      </a>
      <HealthDot state={entity.health_state} />
      {entity.cpu_usage !== null && (
        <span className="text-[var(--muted)] text-xs">
          CPU {Math.round(entity.cpu_usage * 100)}%
        </span>
      )}
      {entity.memory_usage !== null && (
        <span className="text-[var(--muted)] text-xs">
          Mem {Math.round(entity.memory_usage * 100)}%
        </span>
      )}
    </div>
  );
}
