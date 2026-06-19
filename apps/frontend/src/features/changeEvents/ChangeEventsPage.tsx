import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { listChangeEvents, type ChangeEvent, type ChangeEventType } from "../../api/changeEvents";
import { Badge } from "../../components/ui/badge";
import { EmptyState } from "../../components/ui/empty-state";
import { LoadingState } from "../../components/ui/loading-state";
import { Select, SelectOption } from "../../components/ui/select";
import { useGlobalDateRange } from "../../hooks/useGlobalDateRange";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useTimeDisplay } from "../../lib/timeDisplay";
import { formatTimestamp } from "../../utils/formatTimestamp";

const EVENT_TYPES: ChangeEventType[] = ["config_change", "feature_flag", "migration", "incident", "other"];

function eventTypeTone(eventType: ChangeEventType): "good" | "bad" | "warn" | "info" {
  switch (eventType) {
    case "incident":      return "bad";
    case "feature_flag":  return "info";
    case "migration":     return "warn";
    default:              return "info";
  }
}

export default function ChangeEventsPage() {
  const { tenantId } = useTenantContext();
  const { fromMs, toMs } = useGlobalDateRange();
  const { format } = useTimeDisplay();
  const [serviceFilter, setServiceFilter] = useState("");
  const [typeFilter, setTypeFilter] = useState<ChangeEventType | "all">("all");

  const { data, isLoading, error } = useQuery({
    queryKey: ["change-events-explorer", tenantId, fromMs, toMs, serviceFilter, typeFilter],
    queryFn: () =>
      listChangeEvents(tenantId, {
        service_name: serviceFilter || undefined,
        event_type: typeFilter === "all" ? undefined : typeFilter,
        start_time: new Date(fromMs).toISOString(),
        end_time: new Date(toMs).toISOString(),
        limit: 200,
      }),
  });

  const items = useMemo(() => data?.items ?? [], [data]);

  if (isLoading) return <LoadingState>Loading change events...</LoadingState>;
  if (error) return <div className="signal-empty">Change events could not be loaded.</div>;

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Explorer</div>
          <h1>Change Events</h1>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <input
          type="search"
          value={serviceFilter}
          onChange={(e) => setServiceFilter(e.target.value)}
          placeholder="Filter by service…"
          aria-label="Filter by service"
          className="min-w-[180px] border border-[var(--border)] bg-[var(--surface)] px-2.5 py-1 text-xs text-[var(--text)] placeholder:text-[var(--muted)] focus:border-[var(--brand)] focus:outline-none"
        />
        <Select
          value={typeFilter}
          onChange={(e) => setTypeFilter(e.target.value as ChangeEventType | "all")}
          aria-label="Filter by event type"
        >
          <SelectOption value="all">All types</SelectOption>
          {EVENT_TYPES.map((t) => (
            <SelectOption key={t} value={t}>{t}</SelectOption>
          ))}
        </Select>
      </div>

      {items.length === 0 ? (
        <EmptyState
          title="No change events found"
          description="No change events match the current filters and time range."
        />
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm" aria-label="Change events">
            <thead>
              <tr className="border-b text-left">
                <th className="pb-2 pr-4">Title</th>
                <th className="pb-2 pr-4">Type</th>
                <th className="pb-2 pr-4">Service</th>
                <th className="pb-2 pr-4">Environment</th>
                <th className="pb-2 pr-4">Occurred</th>
                <th className="pb-2 pr-4">Source</th>
              </tr>
            </thead>
            <tbody>
              {items.map((ev: ChangeEvent) => (
                <tr key={ev.change_event_id} className="modern-table-row">
                  <td className="py-2 pr-4">
                    <div className="font-semibold text-[var(--text-strong)]">{ev.title}</div>
                    {ev.description && (
                      <div className="text-xs text-[var(--muted)]">{ev.description}</div>
                    )}
                  </td>
                  <td className="py-2 pr-4">
                    <Badge tone={eventTypeTone(ev.event_type)}>{ev.event_type}</Badge>
                  </td>
                  <td className="py-2 pr-4">{ev.service_name ?? "—"}</td>
                  <td className="py-2 pr-4">{ev.environment}</td>
                  <td className="py-2 pr-4 text-[var(--muted)]">
                    {formatTimestamp(new Date(ev.occurred_at).getTime() * 1_000_000, format)}
                  </td>
                  <td className="py-2 pr-4 text-[var(--muted)]">{ev.source ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
