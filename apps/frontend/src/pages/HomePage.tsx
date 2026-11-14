import { useMemo } from "react";
import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { Badge } from "../components/ui/badge";
import { DataFreshness } from "../components/ui/data-freshness";
import { EmptyState } from "../components/ui/empty-state";
import { MetricCard } from "../components/ui/metric-card";
import { TablePanel } from "../components/ui/table-panel";
import { LoadingState } from "../components/ui/loading-state";
import { useTenantContext } from "../hooks/useTenantContext";
import { useRuntime } from "../hooks/useRuntime";
import { useCatalogQuery } from "../features/catalog/catalogUi";
import { StateLabel } from "../components/ui/state";
import { saveReturnContext } from "../components/ScopeBar";
import type { ServiceReview } from "../runtime/catalogContract";
import type { RootSearch } from "../router";

export default function HomePage() {
  const { tenantId } = useTenantContext();
  const runtime = useRuntime();

  const { data: servicesData, isLoading: servicesLoading, dataUpdatedAt } = useQuery({
    queryKey: ["services-summary", tenantId, "all"],
    queryFn: () => runtime.services.list(tenantId, {}),
  });

  const { data: alertsData } = useQuery({
    queryKey: ["alert-rules", tenantId],
    queryFn: () => runtime.alerts.list(tenantId),
  });

  const { data: incidentsData } = useQuery({
    queryKey: ["incidents", tenantId, "open"],
    queryFn: () => runtime.incidents.list(tenantId, "open"),
  });

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
  const status = reviewStatus(review.data?.items, review.isError);
  const errorServices = services
    .filter((s) => s.error_rate > 0)
    .sort((a, b) => b.error_rate - a.error_rate)
    .slice(0, 5);
  const recentIncidents = incidents.slice(0, 5);
  const firingAlerts = alerts.filter((a) => a.firing).slice(0, 5);

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Overview</div>
          <h1>System Status</h1>
        </div>
        <div className="flex items-center gap-3">
          <DataFreshness dataUpdatedAt={dataUpdatedAt} />
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

      {/* Critical services overview (OBS-21-01, OBS-21-08) */}
      {(review.data?.items?.length ?? 0) > 0 && (
        <div>
          <div className="flex items-center justify-between mb-2">
            <div className="text-xs font-bold uppercase text-[var(--muted)]">Critical Services</div>
            <Link to="/catalog" search={{ view: "overview" }} className="text-xs text-[var(--brand)] hover:underline">
              Service health overview →
            </Link>
          </div>
          <TablePanel>
            <table aria-label="Critical services">
              <thead>
                <tr>
                  <th>Service</th>
                  <th>Health</th>
                  <th>Confidence</th>
                </tr>
              </thead>
              <tbody>
                {(review.data?.items ?? []).map((item) => (
                  <tr key={item.service.id}>
                    <td className="strong-cell">
                      <Link
                        to="/catalog"
                        search={{ view: "overview", selected: item.service.id }}
                      >
                        {item.service.business_name}
                      </Link>
                    </td>
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
