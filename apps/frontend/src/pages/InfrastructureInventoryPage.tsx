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
import { useTenantContext } from "../hooks/useTenantContext";
import { liveViewQueryOptions } from "../hooks/useLiveRefresh";
import { Badge } from "../components/ui/badge";
import { EmptyState } from "../components/ui/empty-state";
import { ErrorState } from "../components/ui/error-state";
import { LoadingState } from "../components/ui/loading-state";
import { MetricCard } from "../components/ui/metric-card";
import { PillFilter } from "../components/ui/pill-filter";
import { TablePanel } from "../components/ui/table-panel";
import { QueryInput } from "../features/nlq/QueryInput";

const INFRA_BASE_IR: NlqIrLike = {
  operation: "inventory",
  signals: ["metrics"],
  filters: [],
  time_range: { from: "now-1h", to: "now" },
};

type InfrastructureTypeFilter = "all" | InfrastructureEntityType;

const ENTITY_TYPES: InfrastructureEntityType[] = ["host", "cluster", "namespace", "pod", "container"];

const ENTITY_TYPE_ICONS: Record<InfrastructureEntityType, React.ReactNode> = {
  host: (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
      <rect x="1" y="2" width="12" height="10" rx="1" />
      <line x1="1" y1="6" x2="13" y2="6" />
      <line x1="1" y1="10" x2="13" y2="10" />
      <circle cx="11.5" cy="4" r="0.8" fill="currentColor" stroke="none" />
      <circle cx="11.5" cy="8" r="0.8" fill="currentColor" stroke="none" />
    </svg>
  ),
  cluster: (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
      <polygon points="7,1 13,4.5 13,9.5 7,13 1,9.5 1,4.5" />
    </svg>
  ),
  namespace: (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
      <path d="M4 1 L1 1 L1 13 L4 13" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M10 1 L13 1 L13 13 L10 13" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  pod: (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
      <ellipse cx="7" cy="7" rx="6" ry="4" />
      <circle cx="4" cy="7" r="1.2" fill="currentColor" stroke="none" />
      <circle cx="7" cy="7" r="1.2" fill="currentColor" stroke="none" />
      <circle cx="10" cy="7" r="1.2" fill="currentColor" stroke="none" />
    </svg>
  ),
  container: (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
      <path d="M7 1 L13 4 L13 10 L7 13 L1 10 L1 4 Z" />
      <line x1="7" y1="1" x2="7" y2="7" />
      <line x1="1" y1="4" x2="7" y2="7" />
      <line x1="13" y1="4" x2="7" y2="7" />
    </svg>
  ),
};

function applyFilters(
  items: InfrastructureEntitySummary[],
  entityType: InfrastructureTypeFilter,
  health: string,
  search: string,
) {
  const sv = search.trim().toLowerCase();
  return items.filter((item) => {
    if (entityType !== "all" && item.entity_type !== entityType) return false;
    if (health !== "all" && item.health_state !== health) return false;
    if (sv.length > 0) {
      const matchesText =
        item.display_name.toLowerCase().includes(sv) ||
        item.entity_id.toLowerCase().includes(sv) ||
        item.parent_display_name?.toLowerCase().includes(sv) === true ||
        item.related_services.some((s) => s.toLowerCase().includes(sv));
      if (!matchesText) return false;
    }
    return true;
  });
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

export default function InfrastructureInventoryPage() {
  const [userQuery, setUserQuery] = useState<string | null>(null);
  const [healthFilter, setHealthFilter] = useState("all");
  const [entityTypeFilter, setEntityTypeFilter] = useState<InfrastructureTypeFilter>("all");
  const [search, setSearch] = useState("");
  const { format } = useTimeDisplay();
  const { fromMs, toMs } = useGlobalDateRange();
  const { tenantId } = useTenantContext();
  const from = String(BigInt(Math.floor(fromMs)) * 1_000_000n);
  const to = String(BigInt(Math.floor(toMs)) * 1_000_000n);

  const { data, isLoading, isError } = useQuery({
    queryKey: ["infrastructure", "surface", tenantId, userQuery, fromMs, toMs],
    queryFn: async () => {
      const response = await submitNlqQuery(tenantId, {
        base_ir: { ...INFRA_BASE_IR, time_range: { from, to } },
        question: userQuery ?? undefined,
        mode: "execute",
      });
      if (response.type !== "frame") return [];
      return response.frame.data as unknown as InfrastructureEntitySummary[];
    },
    ...liveViewQueryOptions,
  });

  const items = data ?? [];

  const filteredItems = useMemo(
    () => applyFilters(items, entityTypeFilter, healthFilter, search),
    [items, entityTypeFilter, healthFilter, search],
  );

  // Type pill counts: ignore type filter so each pill shows how many of that type match health+search
  const typeCounts = useMemo(() => {
    const base = applyFilters(items, "all", healthFilter, search);
    const counts: Partial<Record<InfrastructureTypeFilter, number>> = { all: base.length };
    for (const item of base) {
      counts[item.entity_type] = (counts[item.entity_type] ?? 0) + 1;
    }
    return counts;
  }, [items, healthFilter, search]);

  // Health pill counts: ignore health filter so each pill shows how many of that state match type+search
  const healthCounts = useMemo(
    () => summarizeInfrastructure(applyFilters(items, entityTypeFilter, "all", search)),
    [items, entityTypeFilter, search],
  );

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
        <QueryInput
          baseIr={INFRA_BASE_IR}
          placeholder='Filter infrastructure, e.g. "prod pods for checkout in breach"'
          onSubmit={(text) => {
            setUserQuery(text);
            setEntityTypeFilter("all");
            setHealthFilter("all");
            setSearch("");
          }}
        />
      </div>

      <div className="toolbar-row flex-wrap gap-y-2">
        <PillFilter
          pills={(["all", ...ENTITY_TYPES] as InfrastructureTypeFilter[]).map((type) => ({
            key: type,
            label: type === "all" ? "All types" : type,
            count: typeCounts[type] ?? 0,
            icon: type !== "all" ? ENTITY_TYPE_ICONS[type] : undefined,
          }))}
          activeKey={entityTypeFilter}
          onSelect={(key) => setEntityTypeFilter(key as InfrastructureTypeFilter)}
          rounded
          ariaLabel="Filter by entity type"
        />

        <PillFilter
          pills={(["all", "healthy", "watch", "breach"] as const).map((health) => {
            const count =
              health === "all"
                ? healthCounts.healthy + healthCounts.watch + healthCounts.breach
                : healthCounts[health];
            const activeColor =
              health === "breach" ? "var(--bad)" : health === "watch" ? "var(--warn)" : health === "healthy" ? "var(--good)" : "var(--brand)";
            return { key: health, label: health === "all" ? "All health" : health, count, activeColor };
          })}
          activeKey={healthFilter}
          onSelect={(key) => setHealthFilter(key)}
          rounded
          ariaLabel="Filter by health status"
        />

        <input
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search entities…"
          className="ml-auto min-w-[180px] px-2.5 py-1 text-xs border border-[var(--border)] bg-transparent text-[var(--text)] placeholder:text-[var(--muted)] rounded focus:outline-none focus:border-[var(--brand)]"
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
          <ErrorState title="Failed to load infrastructure" description="Infrastructure inventory could not be loaded." />
        ) : filteredItems.length === 0 ? (
          <EmptyState title="No infrastructure entities found" description="No entities matched the current filters." />
        ) : (
          <table aria-label="Infrastructure inventory">
            <thead>
              <tr>
                <th>Entity</th>
                <th>Type</th>
                <th>Health</th>
                <th>CPU</th>
                <th>Memory</th>
                <th>Disk</th>
                <th>Error rate</th>
                <th>Log rate</th>
                <th>Related services</th>
                <th>Environment</th>
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

function UtilizationBar({ value }: { value: number | null }) {
  if (value === null) return <span className="text-[var(--muted)]">--</span>;
  const pct = Math.min(100, Math.max(0, value * 100));
  const colorVar = pct >= 80 ? "var(--bad)" : pct >= 60 ? "var(--warn)" : "var(--good)";
  return (
    <div className="flex items-center gap-2">
      <div className="w-16 h-1 rounded-full overflow-hidden" style={{ backgroundColor: "var(--border)" }}>
        <div className="h-full rounded-full" style={{ width: `${pct}%`, backgroundColor: colorVar }} />
      </div>
      <span className="text-xs tabular-nums" style={{ color: colorVar }}>
        {pct.toFixed(0)}%
      </span>
    </div>
  );
}

function InfrastructureRow({
  item,
  format,
}: {
  item: InfrastructureEntitySummary;
  format: import("../lib/timeDisplay").TimeFormat;
}) {
  const rowBorderClass =
    item.health_state === "breach"
      ? "border-l-2 border-l-[var(--bad)]"
      : item.health_state === "watch"
        ? "border-l-2 border-l-[var(--warn)]"
        : "";

  return (
    <tr className={rowBorderClass}>
      <td className="strong-cell">
        <Link
          to="/infrastructure/$entityType/$entityId"
          params={{ entityType: item.entity_type, entityId: item.entity_id }}
        >
          {item.display_name}
        </Link>
      </td>
      <td>
        <span className="flex items-center gap-1.5 text-[var(--muted)]">
          {ENTITY_TYPE_ICONS[item.entity_type]}
          <span>{item.entity_type}</span>
        </span>
      </td>
      <td>
        <HealthStatus healthState={item.health_state} />
      </td>
      <td>
        <UtilizationBar value={item.cpu_usage} />
      </td>
      <td>
        <UtilizationBar value={item.memory_usage} />
      </td>
      <td>
        <UtilizationBar value={item.disk_usage} />
      </td>
      <td>{item.error_rate === null ? <span className="text-[var(--muted)]">--</span> : `${(item.error_rate * 100).toFixed(2)}%`}</td>
      <td>{item.log_rate_per_minute === null ? <span className="text-[var(--muted)]">--</span> : `${item.log_rate_per_minute.toFixed(2)}/min`}</td>
      <td>
        <RelatedServiceLinks services={item.related_services} />
      </td>
      <td>{item.environment ?? <span className="text-[var(--muted)]">--</span>}</td>
      <td>{formatTimestamp(item.last_seen_unix_nano, format)}</td>
    </tr>
  );
}

function RelatedServiceLinks({ services }: { services: string[] }) {
  if (services.length === 0) return <span className="text-[var(--muted)]">--</span>;
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

function HealthStatus({ healthState }: { healthState: InfrastructureEntitySummary["health_state"] }) {
  const tone = healthState === "breach" ? "bad" : healthState === "watch" ? "warn" : "good";
  const label = healthState === "breach" ? "Breach" : healthState === "watch" ? "Watch" : "Healthy";
  return <Badge tone={tone}>{label}</Badge>;
}
