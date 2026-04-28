import { useQuery } from "@tanstack/react-query";
import { Link, useParams } from "@tanstack/react-router";
import {
  getInfrastructureDetail,
  type InfrastructureEntitySummary,
  type InfrastructureEntityType,
} from "../api/infrastructure";
import { Badge } from "../components/ui/badge";
import { EmptyState } from "../components/ui/empty-state";
import { MetricCard } from "../components/ui/metric-card";
import { Panel } from "../components/ui/panel";

export default function InfrastructureDetailPage() {
  const { entityType, entityId } = useParams({ strict: false });

  const canonicalEntityId = entityId ? decodeURIComponent(entityId) : "";

  const { data, isLoading, isError } = useQuery({
    queryKey: ["infrastructure-detail", entityType, canonicalEntityId],
    queryFn: () => getInfrastructureDetail(entityType as InfrastructureEntityType, canonicalEntityId),
    enabled: !!entityType && !!entityId,
  });

  if (!entityType || !entityId) {
    return <div className="loading-state">Loading infrastructure detail...</div>;
  }

  if (isLoading) {
    return <div className="loading-state">Loading infrastructure detail...</div>;
  }

  if (isError || !data) {
    return (
      <section className="page-stack">
        <div className="page-header">
          <div>
            <div className="field-label">Infrastructure</div>
            <h1>Infrastructure entity not found</h1>
          </div>
          <Link to="/infrastructure" className="secondary-link">
            Back to inventory
          </Link>
        </div>
        <EmptyState title="Infrastructure entity not found" metadata={[canonicalEntityId]} />
      </section>
    );
  }

  const { entity, links } = data;

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="field-label">Infrastructure</div>
          <h1>{entity.display_name}</h1>
        </div>
        <Link to="/infrastructure" className="secondary-link">
          Back to inventory
        </Link>
      </div>

      <div className="metric-grid" aria-label="Infrastructure summary">
        <MetricCard label="Type" value={entity.entity_type} tone="info" />
        <MetricCard label="Environment" value={formatNullableText(entity.environment)} tone="info" />
        <MetricCard
          label="Health"
          value={<HealthStatus healthState={entity.health_state} />}
          tone={healthTone(entity.health_state)}
        />
        <MetricCard
          label="Related services"
          value={entity.related_services.length > 0 ? String(entity.related_services.length) : "Unavailable"}
          tone="info"
        />
      </div>

      <div className="detail-grid">
        <Panel eyebrow="Relationship" title="Hierarchy">
          <dl className="definition-grid">
            <div>
              <dt>Parent relationship</dt>
              <dd>{formatParent(entity)}</dd>
            </div>
            <div>
              <dt>Entity ID</dt>
              <dd>{entity.entity_id}</dd>
            </div>
            <div>
              <dt>Last seen</dt>
              <dd>{formatUnixNano(entity.last_seen_unix_nano)}</dd>
            </div>
          </dl>
        </Panel>

        <Panel eyebrow="Related" title="Services">
          {entity.related_services.length > 0 ? (
            <div className="entry-link-grid" aria-label="Related services">
              {entity.related_services.map((service) => (
                <Link key={service} to="/services/$serviceId" params={{ serviceId: service }} className="entry-link">
                  {service}
                </Link>
              ))}
            </div>
          ) : (
            <div className="signal-empty">Unavailable</div>
          )}
        </Panel>

        <Panel eyebrow="Investigate" title="Resource signals">
          <dl className="definition-grid">
            <div>
              <dt>Log rate</dt>
              <dd>{formatNullablePerMinute(entity.log_rate_per_minute)}</dd>
            </div>
            <div>
              <dt>Error rate</dt>
              <dd>{formatNullablePercent(entity.error_rate)}</dd>
            </div>
            <div>
              <dt>Restart count</dt>
              <dd>{formatNullableInteger(entity.restart_count)}</dd>
            </div>
            <div>
              <dt>CPU usage</dt>
              <dd>{formatNullablePercent(entity.cpu_usage)}</dd>
            </div>
            <div>
              <dt>Memory usage</dt>
              <dd>{formatNullablePercent(entity.memory_usage)}</dd>
            </div>
            <div>
              <dt>Disk usage</dt>
              <dd>{formatNullablePercent(entity.disk_usage)}</dd>
            </div>
            <div>
              <dt>Network I/O</dt>
              <dd>{formatNullableBytes(entity.network_io)}</dd>
            </div>
          </dl>
        </Panel>

        <Panel eyebrow="Actions" title="Entry points">
          <div className="entry-link-grid" aria-label="Infrastructure action links">
            <a href={links.logs} className="entry-link">
              Logs
            </a>
            <a href={links.traces} className="entry-link">
              Traces
            </a>
            <a href={links.metrics} className="entry-link">
              Metrics
            </a>
          </div>
        </Panel>
      </div>
    </section>
  );
}

function formatParent(entity: InfrastructureEntitySummary) {
  return entity.parent_display_name ?? entity.parent_id ?? "Unavailable";
}

function formatNullableText(value: string | null) {
  return value ?? "Unavailable";
}

function formatNullableInteger(value: number | null) {
  return value === null ? "Unavailable" : String(value);
}

function formatNullablePerMinute(value: number | null) {
  return value === null ? "Unavailable" : `${value.toFixed(2)}/min`;
}

function formatNullablePercent(value: number | null) {
  return value === null ? "Unavailable" : `${(value * 100).toFixed(2)}%`;
}

function formatNullableBytes(value: number | null) {
  if (value === null) return "Unavailable";
  if (value >= 1_048_576) return `${(value / 1_048_576).toFixed(1)} MB/s`;
  if (value >= 1_024) return `${(value / 1_024).toFixed(1)} KB/s`;
  return `${value.toFixed(0)} B/s`;
}

function formatUnixNano(nanos: number): string {
  return new Date(nanos / 1_000_000).toLocaleString();
}

function healthTone(healthState: InfrastructureEntitySummary["health_state"]) {
  if (healthState === "breach") return "bad";
  if (healthState === "watch") return "warn";
  return "good";
}

function HealthStatus({ healthState }: { healthState: InfrastructureEntitySummary["health_state"] }) {
  if (healthState === "breach") return <Badge tone="bad">Breach</Badge>;
  if (healthState === "watch") return <Badge tone="warn">Watch</Badge>;
  return <Badge tone="good">Healthy</Badge>;
}
