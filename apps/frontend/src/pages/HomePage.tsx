import { useMemo } from "react";
import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { listServiceSummaries } from "../api/services";
import { listAlertRules } from "../api/alerts";
import { listIncidents } from "../api/incidents";
import { Badge } from "../components/ui/badge";
import { MetricCard } from "../components/ui/metric-card";
import { TablePanel } from "../components/ui/table-panel";
import { LoadingState } from "../components/ui/loading-state";
import { useTenantContext } from "../hooks/useTenantContext";

export default function HomePage() {
  const { tenantId } = useTenantContext();

  const { data: servicesData, isLoading: servicesLoading } = useQuery({
    queryKey: ["services-summary", tenantId, "all"],
    queryFn: () => listServiceSummaries(tenantId),
  });

  const { data: alertsData } = useQuery({
    queryKey: ["alert-rules", tenantId],
    queryFn: () => listAlertRules(tenantId),
  });

  const { data: incidentsData } = useQuery({
    queryKey: ["incidents", tenantId, "open"],
    queryFn: () => listIncidents(tenantId, "open"),
  });

  const services = servicesData?.items ?? [];
  const alerts = alertsData?.items ?? [];
  const incidents = incidentsData?.items ?? [];

  const stats = useMemo(() => {
    const count = services.length;
    const breach = services.filter((s) => s.health_state === "breach").length;
    const watch = services.filter((s) => s.health_state === "watch").length;
    const healthy = services.filter((s) => s.health_state === "healthy").length;
    const avgP95 = count > 0 ? services.reduce((a, s) => a + s.p95_latency_ms, 0) / count : 0;
    const avgError = count > 0 ? services.reduce((a, s) => a + s.error_rate, 0) / count : 0;
    const firingAlerts = alerts.filter((a) => a.firing).length;
    const openIncidents = incidents.length;
    return { count, breach, watch, healthy, avgP95, avgError, firingAlerts, openIncidents };
  }, [services, alerts, incidents]);

  const systemStatus =
    stats.breach > 0 ? "breach" : stats.watch > 0 ? "watch" : "healthy";

  const statusLabel = systemStatus === "breach" ? "Degraded" : systemStatus === "watch" ? "Warning" : "Operational";
  const statusColor =
    systemStatus === "breach" ? "var(--bad)" : systemStatus === "watch" ? "var(--warn)" : "var(--good)";
  const statusBg =
    systemStatus === "breach" ? "var(--bad-bg)" : systemStatus === "watch" ? "var(--warn-bg)" : "var(--good-bg)";

  const unhealthyServices = services.filter((s) => s.health_state !== "healthy");
  const recentIncidents = incidents.slice(0, 5);
  const firingAlerts = alerts.filter((a) => a.firing).slice(0, 5);

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Overview</div>
          <h1>System Status</h1>
        </div>
        <Link
          to="/services"
          className="text-xs text-[var(--brand)] hover:underline"
        >
          All services →
        </Link>
      </div>

      {/* System status banner */}
      <div
        style={{
          background: statusBg,
          border: `1px solid ${statusColor}`,
          borderLeft: `4px solid ${statusColor}`,
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
            background: statusColor,
            flexShrink: 0,
          }}
        />
        <span className="font-bold text-sm" style={{ color: statusColor }}>
          {statusLabel}
        </span>
        {servicesLoading ? (
          <span className="text-xs text-[var(--muted)]">Checking services…</span>
        ) : (
          <span className="text-xs text-[var(--muted)]">
            {stats.healthy} healthy · {stats.watch} watch · {stats.breach} breach across {stats.count} services
          </span>
        )}
      </div>

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
        {/* Unhealthy services */}
        <div>
          <div className="flex items-center justify-between mb-2">
            <div className="text-xs font-bold uppercase text-[var(--muted)]">Unhealthy Services</div>
            <Link to="/services" className="text-xs text-[var(--brand)] hover:underline">
              View all →
            </Link>
          </div>
          <TablePanel>
            {servicesLoading ? (
              <LoadingState>Loading…</LoadingState>
            ) : unhealthyServices.length === 0 ? (
              <div className="signal-empty text-[var(--good)]">All services healthy</div>
            ) : (
              <table aria-label="Unhealthy services">
                <thead>
                  <tr>
                    <th>Service</th>
                    <th>Health</th>
                    <th>Error Rate</th>
                    <th>P95</th>
                  </tr>
                </thead>
                <tbody>
                  {unhealthyServices.map((s) => (
                    <tr key={s.service_name}>
                      <td className="strong-cell">
                        <Link to="/services/$serviceId" params={{ serviceId: s.service_name }}>
                          {s.service_name}
                        </Link>
                      </td>
                      <td>
                        <Badge tone={s.health_state === "breach" ? "bad" : "warn"}>
                          {s.health_state === "breach" ? "Breach" : "Watch"}
                        </Badge>
                      </td>
                      <td>
                        <span style={{ color: s.error_rate >= 0.05 ? "var(--bad)" : "var(--warn)" }}>
                          {(s.error_rate * 100).toFixed(2)}%
                        </span>
                      </td>
                      <td>
                        <span
                          style={{
                            color: s.p95_latency_ms >= 500 ? "var(--bad)" : "var(--warn)",
                          }}
                        >
                          {Math.round(s.p95_latency_ms)}ms
                        </span>
                      </td>
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
                <div className="signal-empty text-[var(--good)]">No active alerts</div>
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
                <div className="signal-empty text-[var(--good)]">No open incidents</div>
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
                          <Link to="/incidents/$incidentId" params={{ incidentId: inc.incident_id }}>
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
