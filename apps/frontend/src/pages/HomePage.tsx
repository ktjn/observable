import { useMemo, useState } from "react";
import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { Badge } from "../components/ui/badge";
import { DataFreshness } from "../components/ui/data-freshness";
import { EmptyState } from "../components/ui/empty-state";
import { Input } from "../components/ui/input";
import { MetricCard } from "../components/ui/metric-card";
import { Select, SelectOption } from "../components/ui/select";
import { TablePanel } from "../components/ui/table-panel";
import { LoadingState } from "../components/ui/loading-state";
import { useGlobalDateRange } from "../hooks/useGlobalDateRange";
import { useTenantContext } from "../hooks/useTenantContext";
import { useRuntime } from "../hooks/useRuntime";
import { useCatalogQuery } from "../features/catalog/catalogUi";
import { StateLabel } from "../components/ui/state";
import { saveReturnContext, readRecentEntries } from "../components/ScopeBar";
import type { ServiceReview } from "../runtime/catalogContract";
import type { RootSearch } from "../router";

export type WorkspaceRole = "duty_manager" | "service_owner" | "business_stakeholder";

export default function HomePage() {
  const { tenantId, environment } = useTenantContext();
  const { fromMs, toMs } = useGlobalDateRange();
  const runtime = useRuntime();
  const [ownershipFilter, setOwnershipFilter] = useState<"all" | "accountable">("all");
  const [personalFilter, setPersonalFilter] = useState("");
  const [workspaceRole, setWorkspaceRole] = useState<WorkspaceRole>("duty_manager");

  const { data: servicesData, isLoading: servicesLoading, dataUpdatedAt } = useQuery({
    queryKey: ["services-summary", tenantId, environment ?? "all", fromMs, toMs],
    queryFn: () =>
      runtime.services.list(tenantId, {
        environment: environment ?? undefined,
        from: fromMs,
        to: toMs,
      }),
  });

  const { data: alertsData } = useQuery({
    queryKey: ["alert-rules", tenantId],
    queryFn: () => runtime.alerts.list(tenantId),
  });

  const { data: incidentsData } = useQuery({
    queryKey: ["incidents", tenantId, "open"],
    queryFn: () => runtime.incidents.list(tenantId, "open"),
  });

  const { data: savedViewsData } = useQuery({
    queryKey: ["saved-views-home", tenantId],
    queryFn: () => (runtime.savedViews ? runtime.savedViews.list(tenantId, "traces") : Promise.resolve({ items: [] })),
  });
  const savedInvestigations = savedViewsData?.items ?? [];
  const recentEntries = readRecentEntries();

  const services = servicesData?.items ?? [];
  const alerts = alertsData?.items ?? [];
  const incidents = incidentsData?.items ?? [];

  const stats = useMemo(() => {
    const count = services.length;
    const avgP95 = count > 0 ? services.reduce((a, s) => a + s.p95_latency_ms, 0) / count : 0;
    const avgError = count > 0 ? services.reduce((a, s) => a + s.error_rate, 0) / count : 0;
    const firingAlerts = alerts.filter((a) => a.firing).length;
    const openIncidents = incidents.length;
    return { count, avgP95, avgError, firingAlerts, openIncidents };
  }, [services, alerts, incidents]);

  // Status comes from the catalog service review of business-critical production services,
  // never from telemetry error rates (phase 1 C4.3).
  const review = useCatalogQuery(["reviews", "production", "business_critical", 0],
    (catalog, tenant, signal) => catalog.reviews(tenant, "production", "business_critical", 0, signal));
  const allReviewItems = review.data?.items ?? [];
  const filterTerm = personalFilter.trim().toLowerCase();
  const filteredReviewItems = allReviewItems.filter((item) => {
    if (ownershipFilter === "accountable" && !item.service.accountable) return false;
    if (filterTerm) {
      const nameMatch = item.service.business_name.toLowerCase().includes(filterTerm);
      const domainMatch = (item.service.domain ?? "").toLowerCase().includes(filterTerm);
      if (!nameMatch && !domainMatch) return false;
    }
    return true;
  });
  const criticalAccountableServices = allReviewItems.filter(
    (item) =>
      item.service.accountable &&
      (item.health?.state === "unavailable" ||
        item.health?.state === "degraded" ||
        item.expectation?.status === "breached" ||
        (item.issues?.open ?? 0) > 0),
  );
  const status = reviewStatus(allReviewItems.length > 0 ? allReviewItems : review.data?.items, review.isError);
  const errorServices = services
    .filter((s) => s.error_rate > 0)
    .sort((a, b) => b.error_rate - a.error_rate)
    .slice(0, 5);
  const recentIncidents = incidents.slice(0, 5);
  const firingAlerts = alerts.filter((a) => a.firing).slice(0, 5);
  const criticalIncidents = incidents.filter((inc) => inc.severity === "critical");
  const criticalAlerts = alerts.filter((a) => a.firing && a.severity === "critical");
  const businessJourneys = useMemo(() => {
    const map = new Map<string, { journey: string; affectedServices: string[]; degraded: boolean; impactSummary: string }>();
    for (const item of allReviewItems) {
      const journey = item.service.domain?.trim() || "Core Customer Journey";
      const current = map.get(journey) ?? {
        journey,
        affectedServices: [],
        degraded: false,
        impactSummary: "Operating normally",
      };
      const isDegraded =
        item.health?.state === "degraded" ||
        item.health?.state === "unavailable" ||
        item.expectation?.status === "breached";
      if (isDegraded) {
        current.degraded = true;
        current.affectedServices.push(item.service.business_name);
        current.impactSummary = `Degraded experience affecting ${current.affectedServices.join(", ")}`;
      }
      map.set(journey, current);
    }
    return Array.from(map.values()).sort((a, b) => Number(b.degraded) - Number(a.degraded));
  }, [allReviewItems]);

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Overview</div>
          <h1>System Status</h1>
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <Select
            aria-label="Workspace role"
            data-testid="workspace-role-switcher"
            value={workspaceRole}
            onChange={(e) => setWorkspaceRole(e.target.value as WorkspaceRole)}
          >
            <SelectOption value="duty_manager">Duty Manager</SelectOption>
            <SelectOption value="service_owner">Service Owner</SelectOption>
            <SelectOption value="business_stakeholder">Business Stakeholder</SelectOption>
          </Select>
          <DataFreshness dataUpdatedAt={dataUpdatedAt} viewMode="live" />
          <Link
            to="/services"
            className="text-xs text-[var(--brand)] hover:underline"
          >
            All services →
          </Link>
        </div>
      </div>

      {/* Business-critical expectations banner */}
      <div
        role="region"
        aria-label="Business-critical expectations"
        style={{
          background: status.background,
          border: `1px solid ${status.color}`,
          borderLeft: `4px solid ${status.color}`,
          padding: "10px 14px",
          display: "flex",
          alignItems: "center",
          gap: "10px",
        }}
      >
        <span
          style={{
            display: "inline-block",
            width: 8,
            height: 8,
            borderRadius: "50%",
            background: status.color,
            flexShrink: 0,
          }}
        />
        <span className="font-bold text-sm" style={{ color: status.color }}>
          {review.isPending ? "Checking expectations…" : status.label}
        </span>
        <span className="text-xs text-[var(--muted)]">{status.detail}</span>
        <Link to="/catalog" search={{ view: "review" }} className="ml-auto text-xs text-[var(--brand)] hover:underline">
          Service review →
        </Link>
      </div>

      {/* Critical issues for accountable services — never hidden by personal filters (OBS-13-01) */}
      {(criticalAccountableServices.length > 0 || criticalIncidents.length > 0 || criticalAlerts.length > 0) && (
        <div
          role="region"
          aria-label="Critical issues for accountable services"
          data-testid="accountable-critical-issues"
          className="rounded border border-[var(--bad)] bg-[var(--bad-bg)] p-3 text-sm"
        >
          <div className="font-bold text-[var(--bad)]">
            Critical issues requiring attention
          </div>
          <ul className="mt-1 list-inside list-disc space-y-0.5 text-xs">
            {criticalAccountableServices.map((item) => (
              <li key={item.service.id}>
                <Link to="/catalog" search={{ view: "overview", selected: item.service.id }} className="font-semibold underline">
                  {item.service.business_name}
                </Link>
                {" — "}
                {item.health?.state ?? "unknown"}
                {item.expectation?.status === "breached" ? " (expectation breached)" : ""}
                {item.issues?.open ? ` · ${item.issues.open} open issue(s)` : ""}
              </li>
            ))}
            {criticalIncidents.map((inc) => (
              <li key={inc.incident_id}>
                <Link
                  to="/incidents/$incidentId"
                  params={{ incidentId: inc.incident_id }}
                  onClick={() => saveReturnContext(window.location.pathname + window.location.search)}
                  className="font-semibold underline"
                >
                  {inc.title}
                </Link>
                {" — critical incident"}
              </li>
            ))}
            {criticalAlerts.map((a) => (
              <li key={a.rule_id}>
                <Link to="/alerts/$ruleId" params={{ ruleId: a.rule_id }} className="font-semibold underline">
                  {a.name}
                </Link>
                {" — critical alert"}
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* Critical services overview (OBS-13-01, OBS-21-01, OBS-21-08) */}
      {allReviewItems.length > 0 && (
        <div>
          <div className="flex flex-wrap items-center justify-between gap-2 mb-2">
            <div className="text-xs font-bold uppercase text-[var(--muted)]">Critical Services</div>
            <div className="flex flex-wrap items-center gap-2">
              <Select
                aria-label="Accountable scope"
                value={ownershipFilter}
                onChange={(e) => setOwnershipFilter(e.target.value as "all" | "accountable")}
              >
                <SelectOption value="all">All critical services</SelectOption>
                <SelectOption value="accountable">My accountable services</SelectOption>
              </Select>
              <Input
                aria-label="Filter critical services by area or name"
                placeholder="Filter by service or area…"
                value={personalFilter}
                onChange={(e) => setPersonalFilter(e.target.value)}
                className="h-7 w-44 text-xs"
              />
              <Link to="/catalog" search={{ view: "overview" }} className="text-xs text-[var(--brand)] hover:underline">
                Service health overview →
              </Link>
            </div>
          </div>
          <TablePanel>
            {filteredReviewItems.length === 0 ? (
              <EmptyState title="No services match your personal filter" compact />
            ) : (
              <table aria-label="Critical services">
                <thead>
                  <tr>
                    <th>Service</th>
                    <th>Area</th>
                    <th>Health</th>
                    <th>Confidence</th>
                  </tr>
                </thead>
                <tbody>
                  {filteredReviewItems.map((item) => (
                    <tr key={item.service.id}>
                      <td className="strong-cell">
                        <Link
                          to="/catalog"
                          search={{ view: "overview", selected: item.service.id }}
                        >
                          {item.service.business_name}
                        </Link>
                        {item.service.accountable && (
                          <Badge tone="info" className="ml-2">Accountable</Badge>
                        )}
                      </td>
                      <td className="text-xs text-[var(--muted)]">{item.service.domain || "Unassigned"}</td>
                      <td>
                        <StateLabel state={item.health?.state ?? "unknown"} />
                      </td>
                      <td className="text-xs text-[var(--muted)]">
                        {item.confidence?.confidence ?? review.data?.confidence?.confidence ?? "unknown"}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </TablePanel>
        </div>
      )}

      {/* Key metrics */}
      <div
        className="grid gap-3 max-[860px]:grid-cols-2 max-[560px]:grid-cols-1"
        style={{ gridTemplateColumns: "repeat(4, minmax(140px, 1fr))" }}
      >
        <MetricCard label="Services" value={String(stats.count)} tone="info" />
        <MetricCard
          label="Firing Alerts"
          value={String(stats.firingAlerts)}
          tone={stats.firingAlerts > 0 ? "warn" : "good"}
        />
        <MetricCard
          label="Open Incidents"
          value={String(stats.openIncidents)}
          tone={stats.openIncidents > 0 ? "bad" : "good"}
        />
        <MetricCard
          label="Avg P95 Latency"
          value={`${Math.round(stats.avgP95)}ms`}
          tone={stats.avgP95 >= 500 ? "bad" : stats.avgP95 >= 100 ? "warn" : "good"}
        />
      </div>

      {/* Customer Journeys & Business Impact (OBS-21-04, OBS-21-09) */}
      {(businessJourneys.length > 0 || workspaceRole === "business_stakeholder") && (
        <div data-testid="business-journeys-panel">
          <div className="flex items-center justify-between mb-2">
            <div className="text-xs font-bold uppercase text-[var(--muted)]">
              Customer Journeys &amp; Business Impact
            </div>
            <Link to="/catalog" search={{ view: "overview" }} className="text-xs text-[var(--brand)] hover:underline">
              Journey outcomes →
            </Link>
          </div>
          <TablePanel>
            {businessJourneys.length === 0 ? (
              <EmptyState title="No customer journeys registered yet" compact />
            ) : (
              <table aria-label="Customer journeys">
                <thead>
                  <tr>
                    <th>Customer Journey</th>
                    <th>Status</th>
                    <th>Customer &amp; Service Impact</th>
                  </tr>
                </thead>
                <tbody>
                  {businessJourneys.map((j) => (
                    <tr key={j.journey}>
                      <td className="strong-cell">{j.journey}</td>
                      <td>
                        <Badge tone={j.degraded ? "bad" : "good"}>
                          {j.degraded ? "Degraded" : "Healthy"}
                        </Badge>
                      </td>
                      <td className="text-xs text-[var(--muted)]">{j.impactSummary}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </TablePanel>
        </div>
      )}

      {/* Saved & Recent Investigations (OBS-20-06) */}
      <div data-testid="saved-recent-panel">
        <div className="flex items-center justify-between mb-2">
          <div className="text-xs font-bold uppercase text-[var(--muted)]">
            Saved &amp; Recent Investigations
          </div>
          <Link to="/traces" className="text-xs text-[var(--brand)] hover:underline">
            Open explorer →
          </Link>
        </div>
        <TablePanel>
          {savedInvestigations.length === 0 && recentEntries.length === 0 ? (
            <EmptyState title="No saved or recent investigations yet" compact />
          ) : (
            <ul className="divide-y divide-[var(--border)] text-xs">
              {savedInvestigations.map((sv) => {
                const targetPath = sv.signal_kind === "logs" ? "/logs" : sv.signal_kind === "metrics" ? "/metrics" : "/traces";
                const cfg = (sv.config ?? {}) as { query?: string | null; service?: string; time_range?: { mode?: string; preset?: string; from_ms?: number; to_ms?: number } };
                const qs = new URLSearchParams();
                if (cfg.query) qs.set("q", cfg.query);
                if (cfg.service) qs.set("service", cfg.service);
                if (cfg.time_range?.mode === "preset" && cfg.time_range.preset) qs.set("preset", cfg.time_range.preset);
                const href = `#${targetPath}${qs.toString() ? `?${qs.toString()}` : ""}`;
                return (
                  <li key={sv.saved_view_id} className="flex items-center justify-between px-3 py-2">
                    <a href={href} className="font-semibold text-[var(--brand)] hover:underline">
                      {sv.name}
                    </a>
                    <Badge tone="info">Saved {sv.signal_kind}</Badge>
                  </li>
                );
              })}
              {recentEntries.map((entry) => (
                <li key={entry.pathAndSearch} className="flex items-center justify-between px-3 py-2">
                  <a href={`#${entry.pathAndSearch}`} className="font-semibold text-[var(--brand)] hover:underline">
                    {entry.label}
                  </a>
                  <Badge tone="neutral">Recent {entry.kind}</Badge>
                </li>
              ))}
            </ul>
          )}
        </TablePanel>
      </div>

      <div className="grid gap-3" style={{ gridTemplateColumns: "1fr 1fr" }}>
        {/* Highest error rates: a telemetry signal, not a health assessment */}
        <div>
          <div className="flex items-center justify-between mb-2">
            <div className="text-xs font-bold uppercase text-[var(--muted)]">Highest Error Rates</div>
            <Link to="/services" className="text-xs text-[var(--brand)] hover:underline">
              View all →
            </Link>
          </div>
          <TablePanel>
            {servicesLoading ? (
              <LoadingState>Loading…</LoadingState>
            ) : errorServices.length === 0 ? (
              <EmptyState title="No errors in observed telemetry" compact />
            ) : (
              <table aria-label="Highest error rates">
                <thead>
                  <tr>
                    <th>Service</th>
                    <th>Error Rate</th>
                    <th>P95</th>
                  </tr>
                </thead>
                <tbody>
                  {errorServices.map((s) => (
                    <tr key={s.service_name}>
                      <td className="strong-cell">
                        <Link
                          to="/services/$serviceId"
                          params={{ serviceId: s.service_name }}
                          search={(prev: RootSearch) => prev}
                        >
                          {s.service_name}
                        </Link>
                      </td>
                      <td>{(s.error_rate * 100).toFixed(2)}%</td>
                      <td>{Math.round(s.p95_latency_ms)}ms</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </TablePanel>
        </div>

        {/* Firing alerts + open incidents combined */}
        <div className="flex flex-col gap-3">
          {/* Firing alerts */}
          <div>
            <div className="flex items-center justify-between mb-2">
              <div className="text-xs font-bold uppercase text-[var(--muted)]">Firing Alerts</div>
              <Link to="/alerts" className="text-xs text-[var(--brand)] hover:underline">
                View all →
              </Link>
            </div>
            <TablePanel>
              {firingAlerts.length === 0 ? (
                <EmptyState title="No active alerts" compact />
              ) : (
                <table aria-label="Firing alerts">
                  <thead>
                    <tr>
                      <th>Rule</th>
                      <th>Severity</th>
                    </tr>
                  </thead>
                  <tbody>
                    {firingAlerts.map((a) => (
                      <tr key={a.rule_id}>
                        <td className="strong-cell">
                          <Link to="/alerts/$ruleId" params={{ ruleId: a.rule_id }}>
                            {a.name}
                          </Link>
                        </td>
                        <td>
                          <Badge
                            tone={
                              a.severity === "critical"
                                ? "bad"
                                : a.severity === "warning"
                                  ? "warn"
                                  : "info"
                            }
                          >
                            {a.severity}
                          </Badge>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </TablePanel>
          </div>

          {/* Open incidents */}
          <div>
            <div className="flex items-center justify-between mb-2">
              <div className="text-xs font-bold uppercase text-[var(--muted)]">Open Incidents</div>
              <Link to="/incidents" className="text-xs text-[var(--brand)] hover:underline">
                View all →
              </Link>
            </div>
            <TablePanel>
              {recentIncidents.length === 0 ? (
                <EmptyState title="No open incidents" compact />
              ) : (
                <table aria-label="Open incidents">
                  <thead>
                    <tr>
                      <th>Title</th>
                      <th>Severity</th>
                    </tr>
                  </thead>
                  <tbody>
                    {recentIncidents.map((inc) => (
                      <tr key={inc.incident_id}>
                        <td className="strong-cell">
                          <Link
                            to="/incidents/$incidentId"
                            params={{ incidentId: inc.incident_id }}
                            onClick={() => saveReturnContext(window.location.pathname + window.location.search)}
                          >
                            {inc.title}
                          </Link>
                        </td>
                        <td>
                          <Badge
                            tone={
                              inc.severity === "critical"
                                ? "bad"
                                : inc.severity === "high"
                                  ? "warn"
                                  : "info"
                            }
                          >
                            {inc.severity}
                          </Badge>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </TablePanel>
          </div>
        </div>
      </div>

      {/* Quick nav */}
      <div className="flex flex-wrap gap-2 pt-1 border-t border-[var(--border)]">
        <span className="text-xs font-bold uppercase text-[var(--muted)] self-center mr-1">Explore:</span>
        {[
          { label: "Traces", to: "/traces" },
          { label: "Logs", to: "/logs" },
          { label: "Metrics", to: "/metrics" },
          { label: "Dashboards", to: "/dashboards" },
          { label: "Infrastructure", to: "/infrastructure" },
          { label: "Workbench", to: "/workbench" },
        ].map((link) => (
          <Link
            key={link.to}
            to={link.to}
            className="px-2.5 py-1 text-xs border border-[var(--border)] text-[var(--muted)] hover:border-[var(--brand)] hover:text-[var(--text)] transition-colors"
          >
            {link.label}
          </Link>
        ))}
      </div>
    </section>
  );
}

interface ReviewStatus {
  label: string;
  detail: string;
  color: string;
  background: string;
}

/** Worst agreed-expectation state across business-critical production services. */
export function reviewStatus(items: ServiceReview[] | undefined, failed: boolean): ReviewStatus {
  const neutral = { color: "var(--muted)", background: "var(--surface-subtle)" };
  if (failed) return { ...neutral, label: "Expectations unavailable", detail: "The service review could not be loaded." };
  const live = (items ?? []).filter((item) => item.live && item.expectation);
  if (live.length === 0) return { ...neutral, label: "No business-critical services", detail: "No live business-critical services in production." };
  const count = (status: string) => live.filter((item) => item.expectation?.status === status).length;
  const [breached, atRisk, unknown, compliant] = [count("breached"), count("at_risk"), count("unknown"), count("compliant")];
  const detail = `${compliant} compliant · ${atRisk} at risk · ${breached} breached · ${unknown} unknown across ${live.length} business-critical production services`;
  if (breached > 0) return { label: "Expectations breached", detail, color: "var(--bad)", background: "var(--bad-bg)" };
  if (atRisk > 0) return { label: "Expectations at risk", detail, color: "var(--warn)", background: "var(--warn-bg)" };
  if (unknown > 0) return { ...neutral, label: "Expectations unknown", detail };
  return { label: "Within expectations", detail, color: "var(--good)", background: "var(--good-bg)" };
}
