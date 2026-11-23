import { useEffect, useState } from "react";
import { useRouter } from "@tanstack/react-router";
import { CommandPalette as DesignedCommandPalette } from "@ktjn/designed-react";
import type { CommandPaletteItem } from "@ktjn/designed-react";
import type { RuntimeApi } from "../../runtime/types";

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  tenantId?: string;
  runtime?: RuntimeApi;
}

interface NavResult {
  type: "nav";
  label: string;
  path: string;
}

interface TraceResult {
  type: "trace";
  traceId: string;
}

interface ServiceResult {
  type: "service";
  id: string;
  label: string;
  path: string;
}

interface IncidentResult {
  type: "incident";
  id: string;
  title: string;
}

type Result = NavResult | TraceResult | ServiceResult | IncidentResult;

const PAGE_ENTRIES: NavResult[] = [
  { type: "nav", label: "Home", path: "/" },
  { type: "nav", label: "Service Health", path: "/catalog?view=overview" },
  { type: "nav", label: "Services", path: "/services" },
  { type: "nav", label: "Customer Experience", path: "/services" },
  { type: "nav", label: "Reporting", path: "/catalog?view=review" },
  { type: "nav", label: "Investigation", path: "/traces" },
  { type: "nav", label: "Traces", path: "/traces" },
  { type: "nav", label: "Logs", path: "/logs" },
  { type: "nav", label: "Metrics", path: "/metrics" },
  { type: "nav", label: "Infrastructure", path: "/infrastructure" },
  { type: "nav", label: "Alerts & SLOs", path: "/alerts" },
  { type: "nav", label: "Incidents", path: "/incidents" },
  { type: "nav", label: "Dashboards", path: "/dashboards" },
  { type: "nav", label: "Change Events", path: "/change-events" },
  { type: "nav", label: "Query Workbench", path: "/workbench" },
  { type: "nav", label: "Admin", path: "/admin" },
];

const TRACE_ID_RE = /^[0-9a-f]{16,32}$/i;

function getResults(
  query: string,
  services: ServiceResult[],
  incidents: SearchableIncidentResult[],
): Result[] {
  const q = query.trim();

  if (q === "") {
    return PAGE_ENTRIES;
  }

  const lower = q.toLowerCase();
  const matchedIncidents = incidents.filter(
    (inc) =>
      inc.incident_id_matches(lower) ||
      inc.title.toLowerCase().includes(lower),
  );
  const matchedServices = services.filter((svc) =>
    svc.label.toLowerCase().includes(lower),
  );
  const matchedPages: Result[] = PAGE_ENTRIES.filter((e) =>
    e.label.toLowerCase().includes(lower),
  );

  const results: Result[] = [
    ...matchedServices.slice(0, 8),
    ...matchedIncidents.slice(0, 8),
    ...matchedPages,
  ];

  if (TRACE_ID_RE.test(q) && matchedIncidents.length === 0) {
    results.unshift({ type: "trace", traceId: q });
  }

  return results;
}

interface SearchableIncidentResult extends IncidentResult {
  incident_id_matches: (lowerQuery: string) => boolean;
}

function resultLabel(result: Result): string {
  if (result.type === "nav" || result.type === "service") return result.label;
  if (result.type === "incident") return `Incident: ${result.title} (${result.id})`;
  return `Go to trace: ${result.traceId}`;
}

function resultKey(result: Result): string {
  if (result.type === "nav") return `nav-${result.label}-${result.path}`;
  if (result.type === "service") return `svc-${result.id}`;
  if (result.type === "incident") return `inc-${result.id}`;
  return `trace-${result.traceId}`;
}

interface PaletteItem extends CommandPaletteItem {
  result: Result;
}

export function CommandPalette({ open, onClose, tenantId, runtime }: CommandPaletteProps) {
  const router = useRouter();
  const [query, setQuery] = useState("");
  const [serviceResults, setServiceResults] = useState<ServiceResult[]>([]);
  const [incidentResults, setIncidentResults] = useState<SearchableIncidentResult[]>([]);

  useEffect(() => {
    if (!open || !runtime || !tenantId) return;
    const ac = new AbortController();

    Promise.allSettled([
      runtime.catalog?.services
        ? runtime.catalog.services(tenantId, 0, "", ac.signal)
        : Promise.resolve(null),
      runtime.services?.list
        ? runtime.services.list(tenantId, {})
        : Promise.resolve(null),
      runtime.incidents?.list
        ? runtime.incidents.list(tenantId)
        : Promise.resolve(null),
    ]).then(([catRes, svcRes, incRes]) => {
      if (ac.signal.aborted) return;
      const catItems = catRes.status === "fulfilled" && Array.isArray(catRes.value?.items) ? catRes.value.items : [];
      const rtItems = svcRes.status === "fulfilled" && Array.isArray(svcRes.value?.items) ? svcRes.value.items : [];
      const incItems = incRes.status === "fulfilled" && Array.isArray(incRes.value?.items) ? incRes.value.items : [];

      setServiceResults([
        ...catItems.map((svc) => ({
          type: "service" as const,
          id: svc.id,
          label: `Service: ${svc.business_name} (${svc.operational_name}, ${svc.domain})`,
          path: `/catalog/services/${svc.id}`,
        })),
        ...rtItems.map((svc) => ({
          type: "service" as const,
          id: `rt-${svc.service_name}`,
          label: `Service: ${svc.service_name}`,
          path: `/services/${svc.service_name}`,
        })),
      ]);
      setIncidentResults(
        incItems.map((inc) => ({
          type: "incident" as const,
          id: inc.incident_id,
          title: inc.title,
          incident_id_matches: (lowerQuery: string) =>
            inc.incident_id.toLowerCase().includes(lowerQuery),
        })),
      );
    });

    return () => ac.abort();
  }, [open, runtime, tenantId]);

  // Reset query when opening
  useEffect(() => {
    if (open) setQuery("");
  }, [open]);

  const results = getResults(query, serviceResults, incidentResults);
  const items: PaletteItem[] = results.map((result) => ({
    id: resultKey(result),
    label: resultLabel(result),
    result,
  }));

  function activate(result: Result) {
    if (result.type === "nav" || result.type === "service") {
      router.navigate({ to: result.path });
    } else if (result.type === "incident") {
      router.navigate({ to: `/incidents/${result.id}` });
    } else {
      router.navigate({ to: `/traces/${result.traceId}` });
    }
  }

  return (
    <DesignedCommandPalette
      items={items}
      open={open}
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
      query={query}
      onQueryChange={setQuery}
      filter={null}
      renderItem={(item) => <span>{item.label}</span>}
      onSelect={(item) => activate(item.result)}
      title="Command palette"
      placeholder="Search services, incidents, pages, or trace IDs…"
      searchLabel="Search services, incidents, pages, or trace IDs"
      emptyMessage="No results"
    />
  );
}
