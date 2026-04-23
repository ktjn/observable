import { useQuery } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { Link, useParams } from "@tanstack/react-router";
import {
  getInfrastructureDetail,
  type InfrastructureEntitySummary,
  type InfrastructureEntityType,
} from "../api/infrastructure";

export default function InfrastructureDetailPage() {
  const { entityType, entityId } = useParams({ strict: false });

  if (!entityType || !entityId) {
    return <div className="loading-state">Loading infrastructure detail...</div>;
  }

  const canonicalEntityId = decodeURIComponent(entityId);

  const { data, isLoading, isError } = useQuery({
    queryKey: ["infrastructure-detail", entityType, canonicalEntityId],
    queryFn: () => getInfrastructureDetail(entityType as InfrastructureEntityType, canonicalEntityId),
  });

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
        <div className="empty-panel">
          <div className="empty-title">Infrastructure entity not found</div>
          <div className="empty-metrics">
            <span>{canonicalEntityId}</span>
          </div>
        </div>
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
        <MetricTile label="Type" value={entity.entity_type} tone="info" />
        <MetricTile label="Environment" value={formatNullableText(entity.environment)} tone="info" />
        <MetricTile
          label="Health"
          value={<HealthStatus healthState={entity.health_state} />}
          tone={healthTone(entity.health_state)}
        />
        <MetricTile
          label="Related services"
          value={entity.related_services.length > 0 ? String(entity.related_services.length) : "Unavailable"}
          tone="info"
        />
      </div>

      <div className="detail-grid">
        <section className="detail-panel">
          <div className="detail-panel-header">
            <div>
              <div className="field-label">Relationship</div>
              <h2>Hierarchy</h2>
            </div>
          </div>
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
              <dd>{String(entity.last_seen_unix_nano)}</dd>
            </div>
          </dl>
        </section>

        <section className="detail-panel">
          <div className="detail-panel-header">
            <div>
              <div className="field-label">Related</div>
              <h2>Services</h2>
            </div>
          </div>
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
        </section>

        <section className="detail-panel">
          <div className="detail-panel-header">
            <div>
              <div className="field-label">Investigate</div>
              <h2>Resource signals</h2>
            </div>
          </div>
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
              <dd>{formatNullablePercent(entity.network_io)}</dd>
            </div>
          </dl>
        </section>

        <section className="detail-panel">
          <div className="detail-panel-header">
            <div>
              <div className="field-label">Actions</div>
              <h2>Entry points</h2>
            </div>
          </div>
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
        </section>
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

function healthTone(healthState: InfrastructureEntitySummary["health_state"]) {
  if (healthState === "breach") return "bad";
  if (healthState === "watch") return "warn";
  return "good";
}

function HealthStatus({ healthState }: { healthState: InfrastructureEntitySummary["health_state"] }) {
  if (healthState === "breach") return <span className="status bad">Breach</span>;
  if (healthState === "watch") return <span className="status warn">Watch</span>;
  return <span className="status good">Healthy</span>;
}

function MetricTile({
  label,
  value,
  tone,
}: {
  label: string;
  value: ReactNode;
  tone: "good" | "warn" | "bad" | "info";
}) {
  return (
    <div className={`metric-tile ${tone}`}>
      <div className="metric-label">{label}</div>
      <div className="metric-value">{value}</div>
    </div>
  );
}
