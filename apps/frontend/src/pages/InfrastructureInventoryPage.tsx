import { useMemo, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { listEnvironments } from "../api/services";
import {
  listInfrastructure,
  type InfrastructureEntitySummary,
  type InfrastructureEntityType,
} from "../api/infrastructure";

type InfrastructureTypeFilter = "all" | InfrastructureEntityType;

const infrastructureTypeOptions: InfrastructureTypeFilter[] = [
  "all",
  "host",
  "cluster",
  "namespace",
  "pod",
  "container",
];

export default function InfrastructureInventoryPage() {
  const [environment, setEnvironment] = useState("all");
  const [entityType, setEntityType] = useState<InfrastructureTypeFilter>("all");
  const [search, setSearch] = useState("");

  const { data: environments } = useQuery({
    queryKey: ["environments"],
    queryFn: () => listEnvironments(),
  });

  const { data, isLoading, isError } = useQuery({
    queryKey: ["infrastructure"],
    queryFn: () => listInfrastructure(),
  });

  const filteredItems = useMemo(() => {
    const searchValue = search.trim().toLowerCase();
    return (data?.items ?? []).filter((item) => {
      const matchesType = entityType === "all" || item.entity_type === entityType;
      const matchesEnvironment = environment === "all" || item.environment === environment;
      const matchesSearch =
        searchValue.length === 0 ||
        item.display_name.toLowerCase().includes(searchValue) ||
        item.entity_id.toLowerCase().includes(searchValue) ||
        item.parent_display_name?.toLowerCase().includes(searchValue) === true ||
        item.related_services.some((service) => service.toLowerCase().includes(searchValue));

      return matchesType && matchesEnvironment && matchesSearch;
    });
  }, [data, environment, entityType, search]);

  const summary = useMemo(() => summarizeInfrastructure(filteredItems), [filteredItems]);

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="field-label">Inventory</div>
          <h1>Infrastructure</h1>
        </div>
      </div>

      <div className="toolbar-row">
        <input
          className="search-input"
          aria-label="Search infrastructure"
          placeholder="Search infrastructure"
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
        <select
          className="select-input"
          aria-label="Infrastructure type filter"
          value={entityType}
          onChange={(event) => setEntityType(event.target.value as InfrastructureTypeFilter)}
        >
          {infrastructureTypeOptions.map((option) => (
            <option key={option} value={option}>
              {option === "all" ? "All types" : option}
            </option>
          ))}
        </select>
        <select
          className="select-input"
          aria-label="Environment filter"
          value={environment}
          onChange={(event) => setEnvironment(event.target.value)}
        >
          <option value="all">All environments</option>
          {environments?.items.map((env) => (
            <option key={env} value={env}>
              {env}
            </option>
          ))}
        </select>
      </div>

      <div className="metric-grid" aria-label="Infrastructure summary">
        <MetricTile label="Entities" value={String(filteredItems.length)} tone="info" />
        <MetricTile label="Healthy" value={String(summary.healthy)} tone="good" />
        <MetricTile label="Watch" value={String(summary.watch)} tone="warn" />
        <MetricTile label="Breach" value={String(summary.breach)} tone="bad" />
      </div>

      <div className="table-panel">
        {isLoading ? (
          <div className="loading-state">Loading infrastructure...</div>
        ) : isError ? (
          <div className="signal-empty">Infrastructure inventory could not be loaded.</div>
        ) : filteredItems.length === 0 ? (
          <div className="signal-empty">No infrastructure entities matched the current filters.</div>
        ) : (
          <table aria-label="Infrastructure inventory">
            <thead>
              <tr>
                <th>Entity</th>
                <th>Type</th>
                <th>Environment</th>
                <th>Health</th>
                <th>Related services</th>
                <th>Log rate</th>
                <th>Error rate</th>
              </tr>
            </thead>
            <tbody>
              {filteredItems.map((item) => (
                <InfrastructureRow key={`${item.entity_type}:${item.entity_id}`} item={item} />
              ))}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
}

function InfrastructureRow({ item }: { item: InfrastructureEntitySummary }) {
  return (
    <tr>
      <td className="strong-cell">
        <Link
          to="/infrastructure/$entityType/$entityId"
          params={{ entityType: item.entity_type, entityId: item.entity_id }}
        >
          {item.display_name}
        </Link>
      </td>
      <td>{item.entity_type}</td>
      <td>{item.environment ?? "Unavailable"}</td>
      <td>
        <HealthStatus healthState={item.health_state} />
      </td>
      <td>{formatRelatedServices(item.related_services)}</td>
      <td>{formatPerMinute(item.log_rate_per_minute)}</td>
      <td>{formatPercent(item.error_rate)}</td>
    </tr>
  );
}

function summarizeInfrastructure(items: InfrastructureEntitySummary[]) {
  return items.reduce(
    (acc, item) => {
      if (item.health_state === "healthy") acc.healthy += 1;
      if (item.health_state === "watch") acc.watch += 1;
      if (item.health_state === "breach") acc.breach += 1;
      return acc;
    },
    { healthy: 0, watch: 0, breach: 0 },
  );
}

function formatRelatedServices(relatedServices: string[]) {
  return relatedServices.length > 0 ? relatedServices.join(", ") : "Unavailable";
}

function formatPerMinute(value: number | null) {
  return value === null ? "Unavailable" : `${value.toFixed(2)}/min`;
}

function formatPercent(value: number | null) {
  return value === null ? "Unavailable" : `${(value * 100).toFixed(2)}%`;
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
