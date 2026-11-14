import { useLocation } from "@tanstack/react-router";
import { useGlobalDateRange, PRESET_OPTIONS } from "../hooks/useGlobalDateRange";
import { useTenantContext } from "../hooks/useTenantContext";

const RETURN_TO_KEY = "observable.returnTo";

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
    </div>
  );
}
