import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import {
  type InfrastructureEntitySummary,
  type InfrastructureEntityType,
} from "../api/infrastructure";
import { submitNlqQuery } from "../api/nlq";
import type { NlqIrLike } from "../features/nlq/queryFilters";
import { formatTimestamp } from "../utils/formatTimestamp";
import { useTimeDisplay } from "../lib/timeDisplay";
import { useGlobalDateRange } from "../hooks/useGlobalDateRange";
import { Badge } from "../components/ui/badge";
import { LoadingState } from "../components/ui/loading-state";
import { MetricCard } from "../components/ui/metric-card";
import { TablePanel } from "../components/ui/table-panel";
import { QueryFilterInput } from "../features/nlq/QueryFilterInput";

const INFRA_BASE_IR: NlqIrLike = {
  operation: "inventory",
  signals: ["metrics"],
  filters: [],
  time_range: { from: "now-1h", to: "now" },
};

type InfrastructureTypeFilter = "all" | InfrastructureEntityType;

export default function InfrastructureInventoryPage() {
  const [userQuery, setUserQuery] = useState<string | null>(null);
  const [healthFilter, setHealthFilter] = useState("all");
  const [entityTypeFilter, setEntityTypeFilter] = useState<InfrastructureTypeFilter>("all");
  const [search, setSearch] = useState("");
  const { format } = useTimeDisplay();
  const { fromMs, toMs } = useGlobalDateRange();
  const from = new Date(fromMs).toISOString();
  const to = new Date(toMs).toISOString();

  const { data, isLoading, isError } = useQuery({
    queryKey: ["infrastructure", "surface", userQuery, fromMs, toMs],
    queryFn: async () => {
      const response = await submitNlqQuery({
        base_ir: { ...INFRA_BASE_IR, time_range: { from, to } },
        question: userQuery ?? undefined,
        mode: "execute",
      });
      if (response.type !== "frame") return [];
      return response.frame.data as unknown as InfrastructureEntitySummary[];
    },
  });

  const items = data ?? [];

  const filteredItems = useMemo(() => {
    const searchValue = search.trim().toLowerCase();
    return items.filter((item) => {
      const matchesType = entityTypeFilter === "all" || item.entity_type === entityTypeFilter;
      const matchesHealth = healthFilter === "all" || item.health_state === healthFilter;
      const matchesSearch =
        searchValue.length === 0 ||
        item.display_name.toLowerCase().includes(searchValue) ||
        item.entity_id.toLowerCase().includes(searchValue) ||
        item.parent_display_name?.toLowerCase().includes(searchValue) === true ||
        item.related_services.some((service) => service.toLowerCase().includes(searchValue));
      return matchesType && matchesHealth && matchesSearch;
    });
  }, [items, entityTypeFilter, healthFilter, search]);

  const summary = useMemo(() => summarizeInfrastructure(filteredItems), [filteredItems]);

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Inventory</div>
          <h1>Infrastructure</h1>
        </div>
      </div>

      <div className="toolbar-row">
        <QueryFilterInput
          baseIr={INFRA_BASE_IR}
          placeholder='Filter infrastructure, e.g. "prod pods for checkout in breach" or raw NLQ IR JSON'
          onSubmit={(text) => {
            setUserQuery(text);
            // Secondary client-side filters — reset so they don't conflict with the merged IR.
            setEntityTypeFilter("all");
            setHealthFilter("all");
            setSearch("");
          }}
        />
      </div>

      <div
        className="grid gap-3 max-[860px]:grid-cols-2 max-[560px]:grid-cols-1"
        style={{ gridTemplateColumns: "repeat(4, minmax(140px, 1fr))" }}
        aria-label="Infrastructure summary"
      >
        <MetricCard label="Entities" value={String(filteredItems.length)} tone="info" />
        <MetricCard label="Healthy" value={String(summary.healthy)} tone="good" />
        <MetricCard label="Watch" value={String(summary.watch)} tone="warn" />
        <MetricCard label="Breach" value={String(summary.breach)} tone="bad" />
      </div>

      <TablePanel>
        {isLoading ? (
          <LoadingState>Loading infrastructure…</LoadingState>
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
                <InfrastructureRow key={`${item.entity_type}:${item.entity_id}`} item={item} format={format} />
              ))}
            </tbody>
          </table>
        )}
      </TablePanel>
    </section>
  );
}

function InfrastructureRow({ item, format }: { item: InfrastructureEntitySummary; format: import("../lib/timeDisplay").TimeFormat }) {
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
      <td>{formatTimestamp(item.last_seen_unix_nano, format)}</td>
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

function HealthStatus({ healthState }: { healthState: InfrastructureEntitySummary["health_state"] }) {
  const tone = healthState === "breach" ? "bad" : healthState === "watch" ? "warn" : "good";
  const label = healthState === "breach" ? "Breach" : healthState === "watch" ? "Watch" : "Healthy";
  return <Badge tone={tone}>{label}</Badge>;
}
