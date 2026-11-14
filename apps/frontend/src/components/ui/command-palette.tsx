import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useRouter } from "@tanstack/react-router";
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

export function CommandPalette({ open, onClose, tenantId, runtime }: CommandPaletteProps) {
  const router = useRouter();
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const [serviceResults, setServiceResults] = useState<ServiceResult[]>([]);
  const [incidentResults, setIncidentResults] = useState<SearchableIncidentResult[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

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

  // Reset state when opening
  useEffect(() => {
    if (open) {
      setQuery("");
      setActiveIndex(0);
      // Focus the input after the portal renders
      requestAnimationFrame(() => {
        inputRef.current?.focus();
      });
    }
  }, [open]);

  // Escape key listener on document
  useEffect(() => {
    if (!open) return;

    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        onClose();
      }
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [open, onClose]);

  const results = getResults(query, serviceResults, incidentResults);

  function activate(result: Result) {
    if (result.type === "nav" || result.type === "service") {
      router.navigate({ to: result.path });
    } else if (result.type === "incident") {
      router.navigate({ to: `/incidents/${result.id}` });
    } else {
      router.navigate({ to: `/traces/${result.traceId}` });
    }
    onClose();
  }

  function handleDialogKeyDown(e: React.KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIndex((i) => Math.min(i + 1, results.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      if (results[activeIndex]) {
        activate(results[activeIndex]);
      }
    } else if (e.key === "Tab") {
      // Trap focus within dialog — there's only the input + list, keep focus on input
      e.preventDefault();
    }
  }

  if (!open) return null;

  return createPortal(
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 bg-black/40 z-50"
        onClick={onClose}
        aria-hidden="true"
      />

      {/* Dialog */}
      <div
        role="dialog"
        aria-label="Command palette"
        aria-modal="true"
        className="fixed top-[20%] left-1/2 -translate-x-1/2 w-full max-w-lg bg-[var(--surface)] border border-[var(--border)] rounded shadow-xl z-50"
        onKeyDown={handleDialogKeyDown}
      >
        <input
          ref={inputRef}
          role="combobox"
          aria-autocomplete="list"
          aria-expanded={results.length > 0}
          aria-controls="command-palette-listbox"
          className="w-full p-3 text-sm bg-transparent border-b border-[var(--border)] outline-none"
          placeholder="Search services, incidents, pages, or trace IDs…"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setActiveIndex(0);
          }}
        />

        <ul
          id="command-palette-listbox"
          role="listbox"
          className="max-h-64 overflow-y-auto"
        >
          {results.length === 0 ? (
            <li className="px-4 py-3 text-sm text-[var(--muted)]">No results</li>
          ) : (
            results.map((result, i) => {
              const label =
                result.type === "nav" || result.type === "service"
                  ? result.label
                  : result.type === "incident"
                    ? `Incident: ${result.title} (${result.id})`
                    : `Go to trace: ${result.traceId}`;
              const key =
                result.type === "nav"
                  ? `nav-${result.label}-${result.path}`
                  : result.type === "service"
                    ? `svc-${result.id}`
                    : result.type === "incident"
                      ? `inc-${result.id}`
                      : `trace-${result.traceId}`;
              const isActive = i === activeIndex;

              return (
                <li
                  key={key}
                  role="option"
                  aria-selected={isActive}
                  className={
                    "px-4 py-2 text-sm cursor-pointer" +
                    (isActive ? " bg-[var(--surface-subtle)]" : "")
                  }
                  onClick={() => activate(result)}
                  onMouseEnter={() => setActiveIndex(i)}
                >
                  {label}
                </li>
              );
            })
          )}
        </ul>
      </div>
    </>,
    document.body,
  );
}
