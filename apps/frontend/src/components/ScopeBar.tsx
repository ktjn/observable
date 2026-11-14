import { useLocation } from "@tanstack/react-router";
import { useGlobalDateRange, PRESET_OPTIONS } from "../hooks/useGlobalDateRange";
import { useTenantContext } from "../hooks/useTenantContext";

const RETURN_TO_KEY = "observable.returnTo";
const TRAIL_KEY = "observable.investigationTrail";
const RECENT_KEY = "observable.recentEntries";
const MAX_TRAIL_STEPS = 8;
const MAX_RECENT_ENTRIES = 8;

export interface InvestigationStep {
  pathAndSearch: string;
  label: string;
  service?: string | null;
  environment?: string | null;
  preset?: string | null;
}

export interface RecentEntry {
  pathAndSearch: string;
  label: string;
  kind: "service" | "incident" | "investigation" | "dashboard";
  updatedAt: number;
}

export function saveReturnContext(pathAndSearch: string) {
  try {
    sessionStorage.setItem(RETURN_TO_KEY, pathAndSearch);
  } catch {
    // Ignore storage errors in restricted environments.
  }
}

export function readReturnContext(): string | null {
  try {
    return sessionStorage.getItem(RETURN_TO_KEY);
  } catch {
    return null;
  }
}

export function readInvestigationTrail(): InvestigationStep[] {
  try {
    const raw = sessionStorage.getItem(TRAIL_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

export function pushInvestigationStep(step: InvestigationStep): InvestigationStep[] {
  const existing = readInvestigationTrail();
  const last = existing[existing.length - 1];
  if (last && last.pathAndSearch === step.pathAndSearch) {
    return existing;
  }
  const existingIdx = existing.findIndex((s) => s.pathAndSearch === step.pathAndSearch);
  const next =
    existingIdx >= 0
      ? existing.slice(0, existingIdx + 1)
      : [...existing, step].slice(-MAX_TRAIL_STEPS);
  try {
    sessionStorage.setItem(TRAIL_KEY, JSON.stringify(next));
  } catch {
    // Ignore storage errors in restricted environments.
  }
  return next;
}

export function clearInvestigationTrail() {
  try {
    sessionStorage.removeItem(TRAIL_KEY);
  } catch {
    // Ignore storage errors in restricted environments.
  }
}

export function readRecentEntries(): RecentEntry[] {
  try {
    const raw = localStorage.getItem(RECENT_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

export function recordRecentEntry(entry: Omit<RecentEntry, "updatedAt">): RecentEntry[] {
  const next: RecentEntry[] = [
    { ...entry, updatedAt: Date.now() },
    ...readRecentEntries().filter((e) => e.pathAndSearch !== entry.pathAndSearch),
  ].slice(0, MAX_RECENT_ENTRIES);
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(next));
  } catch {
    // Ignore storage errors in restricted environments.
  }
  return next;
}

export function describeRouteLabel(pathname: string, activeService: string | null): string {
  if (pathname === "/" || pathname === "") return "Overview";
  if (pathname.startsWith("/services/")) {
    return activeService ? `Service: ${activeService}` : "Service Detail";
  }
  if (pathname.startsWith("/services")) return "Services";
  if (pathname.startsWith("/incidents/")) {
    const id = decodeURIComponent(pathname.replace("/incidents/", "").split("/")[0] || "");
    return id ? `Incident: ${id}` : "Incident";
  }
  if (pathname.startsWith("/incidents")) return "Incidents";
  if (pathname.startsWith("/traces/")) return "Trace Detail";
  if (pathname.startsWith("/traces")) return "Traces";
  if (pathname.startsWith("/logs")) return "Logs";
  if (pathname.startsWith("/metrics")) return "Metrics";
  if (pathname.startsWith("/dashboards")) return "Dashboards";
  if (pathname.startsWith("/alerts")) return "Alerts";
  if (pathname.startsWith("/catalog")) return "Service Catalog";
  if (pathname.startsWith("/workbench")) return "Workbench";
  return pathname.replace(/^\//, "") || "Overview";
}

function formatTimePeriod(preset: string, fromMs: number, toMs: number, hasCustomRange: boolean): string {
  if (!hasCustomRange) {
    const opt = PRESET_OPTIONS.find((p) => p.value === preset);
    if (opt) return opt.label;
  }
  const from = new Date(fromMs).toISOString().slice(11, 16);
  const to = new Date(toMs).toISOString().slice(11, 16);
  return `${from}–${to} UTC`;
}

function activeServiceFromPath(pathname: string, params: URLSearchParams): string | null {
  const fromParam = params.get("service")?.trim() || params.get("selected")?.trim();
  if (fromParam) return fromParam;
  const match = pathname.match(/^\/(?:services|catalog\/services)\/([^/]+)/);
  return match ? decodeURIComponent(match[1]) : null;
}

function activeFiltersFromParams(params: URLSearchParams): Array<[string, string]> {
  const keys = ["state", "status", "q", "scope", "sort", "severity", "env"] as const;
  const out: Array<[string, string]> = [];
  for (const key of keys) {
    const val = params.get(key)?.trim();
    if (val) out.push([key, val]);
  }
  return out;
}

export function ScopeBar() {
  const location = useLocation();
  const { tenantName, environment } = useTenantContext();
  const { preset, fromMs, toMs } = useGlobalDateRange();

  const rawSearch =
    typeof (location as { searchStr?: unknown }).searchStr === "string"
      ? ((location as { searchStr: string }).searchStr)
      : typeof window !== "undefined"
        ? window.location.search
        : "";
  const params = new URLSearchParams(rawSearch);
  const hasCustomRange = params.has("from") && params.has("to");
  const periodLabel = formatTimePeriod(preset ?? "1h", fromMs, toMs, hasCustomRange);
  const activeService = activeServiceFromPath(location.pathname || "/", params);
  const envLabel = params.get("env")?.trim() || environment || "All envs";
  const filters = activeFiltersFromParams(params).filter(([k]) => k !== "env");

  const currentPathAndSearch = `${location.pathname || "/"}${rawSearch.startsWith("?") || !rawSearch ? rawSearch : `?${rawSearch}`}`;
  const currentLabel = describeRouteLabel(location.pathname || "/", activeService);
  const trail = pushInvestigationStep({
    pathAndSearch: currentPathAndSearch,
    label: currentLabel,
    service: activeService,
    environment: envLabel,
    preset: periodLabel,
  });
  const previousStep = trail.length > 1 ? trail[trail.length - 2] : null;

  return (
    <div
      role="region"
      aria-label="Active scope"
      data-testid="global-scope-bar"
      className="flex flex-wrap items-center gap-2 border-b border-[var(--border)] bg-[var(--surface-1)] px-4 py-1.5 text-xs text-[var(--muted)]"
    >
      <span className="font-semibold uppercase tracking-wider text-[10px]">Scope</span>
      <span className="rounded border border-[var(--border)] px-2 py-0.5 text-[var(--text)]">
        Tenant: <strong>{tenantName}</strong>
      </span>
      <span className="rounded border border-[var(--border)] px-2 py-0.5 text-[var(--text)]">
        Environment: <strong>{envLabel}</strong>
      </span>
      <span className="rounded border border-[var(--border)] px-2 py-0.5 text-[var(--text)]">
        Period: <strong>{periodLabel}</strong>
      </span>
      {activeService && (
        <span className="rounded border border-[var(--border)] px-2 py-0.5 text-[var(--text)]">
          Service: <strong>{activeService}</strong>
        </span>
      )}
      <span className="rounded border border-[var(--border)] px-2 py-0.5 text-[var(--text)]">
        Filters:{" "}
        <strong>
          {filters.length > 0 ? filters.map(([k, v]) => `${k}=${v}`).join(", ") : "None"}
        </strong>
      </span>
      {trail.length > 0 && (
        <nav
          aria-label="Investigation path"
          data-testid="investigation-trail"
          className="ml-auto flex flex-wrap items-center gap-1.5"
        >
          {previousStep && (
            <a
              href={`#${previousStep.pathAndSearch}`}
              data-testid="investigation-back-link"
              className="rounded border border-[var(--border)] px-2 py-0.5 font-medium text-[var(--brand)] hover:underline"
            >
              ← Back to {previousStep.label}
            </a>
          )}
          <ol className="flex flex-wrap items-center gap-1 text-[11px]">
            {trail.map((step, idx) => {
              const isCurrent = idx === trail.length - 1;
              return (
                <li key={step.pathAndSearch} className="flex items-center gap-1">
                  {idx > 0 && <span aria-hidden="true">/</span>}
                  {isCurrent ? (
                    <span aria-current="page" className="font-semibold text-[var(--text)]">
                      {step.label}
                    </span>
                  ) : (
                    <a
                      href={`#${step.pathAndSearch}`}
                      className="text-[var(--brand)] hover:underline"
                    >
                      {step.label}
                    </a>
                  )}
                </li>
              );
            })}
          </ol>
        </nav>
      )}
    </div>
  );
}
