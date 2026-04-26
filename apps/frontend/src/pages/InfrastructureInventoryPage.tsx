import { useMemo, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { listEnvironments } from "../api/services";
import {
  listInfrastructure,
  type InfrastructureEntitySummary,
  type InfrastructureEntityType,
} from "../api/infrastructure";
import { Input } from "../components/ui/input";
import { Select, SelectOption } from "../components/ui/select";

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
        <Input
          className="max-w-[360px]"
          aria-label="Search infrastructure"
          placeholder="Search infrastructure"
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
        <Select
          aria-label="Infrastructure type filter"
          value={entityType}
          onChange={(event) => setEntityType(event.target.value as InfrastructureTypeFilter)}
        >
          {infrastructureTypeOptions.map((option) => (
            <SelectOption key={option} value={option}>
              {option === "all" ? "All types" : option}
            </SelectOption>
          ))}
        </Select>
        <Select
          aria-label="Environment filter"
          value={environment}
          onChange={(event) => setEnvironment(event.target.value)}
        >
          <SelectOption value="all">All environments</SelectOption>
          {environments?.items.map((env) => (
            <SelectOption key={env} value={env}>
              {env}
            </SelectOption>
          ))}
        </Select>
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
                <th>Last seen</th>
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
      <td>
        <RelatedServiceLinks services={item.related_services} />
      </td>
      <td>{formatPerMinute(item.log_rate_per_minute)}</td>
      <td>{formatPercent(item.error_rate)}</td>
      <td>{formatUnixNano(item.last_seen_unix_nano)}</td>
    </tr>
  );
}

function RelatedServiceLinks({ services }: { services: string[] }) {
  if (services.length === 0) return <>Unavailable</>;
  return (
    <>
      {services.map((service, i) => (
        <span key={service}>
          {i > 0 && ", "}
          <Link to="/services/$serviceId" params={{ serviceId: service }}>
            {service}
          </Link>
        </span>
      ))}
    </>
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

function formatPerMinute(value: number | null) {
  return value === null ? "Unavailable" : `${value.toFixed(2)}/min`;
}

function formatPercent(value: number | null) {
  return value === null ? "Unavailable" : `${(value * 100).toFixed(2)}%`;
}

function formatUnixNano(nanos: number): string {
  return new Date(nanos / 1_000_000).toLocaleString();
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
