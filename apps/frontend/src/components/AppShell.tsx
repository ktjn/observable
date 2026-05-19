import { Outlet } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useTheme, type ThemePreference } from "../lib/theme";
import { useTimeDisplay, TIME_FORMAT_OPTIONS } from "../lib/timeDisplay";
import { GlobalDateRangePicker } from "./GlobalDateRangePicker";
import { UserMenu } from "./UserMenu";
import { useTenantContext } from "../hooks/useTenantContext";
import { listTenants, listEnvironments } from "../api/tenants";
import { useEffect } from "react";
import { useLocation } from "@tanstack/react-router";
import { useAuth } from "../hooks/useAuth";
import { initiateLogin } from "../api/auth";
import { TreeNav, type NavTreeItem } from "./TreeNav";

const navTree: NavTreeItem[] = [
  { id: "home", label: "Home", to: "/" },
  {
    id: "setup",
    label: "Setup",
    children: [
      { id: "setup-ingest", label: "Ingest", to: "/setup" },
      { id: "setup-llm", label: "LLM", to: "/setup/llm" },
      { id: "setup-tokens", label: "Tokens", to: "/setup/tokens" },
    ],
  },
  { id: "nlq", label: "Ask (NLQ)", to: "/nlq" },
  { id: "services", label: "Services", to: "/services" },
  {
    id: "signals",
    label: "Signals",
    children: [
      { id: "traces", label: "Traces", to: "/traces" },
      { id: "logs", label: "Logs", to: "/logs" },
      { id: "metrics", label: "Metrics", to: "/metrics" },
    ],
  },
  { id: "infrastructure", label: "Infrastructure", to: "/infrastructure" },
  { id: "dashboards", label: "Dashboards", to: "/dashboards" },
  { id: "alerts", label: "Alerts & SLOs", to: "/alerts" },
  { id: "incidents", label: "Incidents", to: "/incidents" },
  {
    id: "admin",
    label: "Administration",
    children: [
      { id: "admin-fleet", label: "Fleet / Billing", to: "/admin" },
      { id: "admin-identity", label: "Identity", to: "/admin/identity" },
    ],
  },
];

const themeOptions: { label: string; value: ThemePreference }[] = [
  { label: "Light", value: "light" },
  { label: "Dark", value: "dark" },
  { label: "Sys", value: "system" },
  { label: "VT220", value: "vt220" },
];

export function AppShell() {
  const { preference, setPreference } = useTheme();
  const { format, setFormat } = useTimeDisplay();
  const { tenantId, tenantName, environment, setTenant, setEnvironment } = useTenantContext();

  const { data: tenantsData } = useQuery({
    queryKey: ["tenants"],
    queryFn: listTenants,
  });

  const { data: environmentsData } = useQuery({
    queryKey: ["environments", tenantId],
    queryFn: () => listEnvironments(tenantId),
    enabled: !!tenantId,
  });

  const tenants = tenantsData?.tenants ?? [];
  const environments = environmentsData?.environments ?? [];

  const { data: user, isLoading: authLoading } = useAuth();
  const location = useLocation();

  useEffect(() => {
    const pathname = location.pathname;
    if (!authLoading && !user && !/^\/(login|auth\/callback)$/.test(pathname)) {
      initiateLogin();
    }
  }, [authLoading, user, location.pathname]);

  // After login the session is filtered to only the tenants the user has access
  // to.  If the current context (from localStorage) is no longer in that list,
  // switch to the first available tenant so all API calls use a valid tenant.
  useEffect(() => {
    const available = tenantsData?.tenants;
    if (!user || !available || available.length === 0) return;
    const isValid = available.some((t) => t.id === tenantId);
    if (!isValid) {
      setTenant({ id: available[0].id, name: available[0].name });
    }
  }, [user, tenantsData, tenantId, setTenant]);

  return (
    <div className="app-shell">
      <aside className="sidebar" aria-label="Application sidebar">
        <div className="brand-lockup">
          <div className="brand-mark" aria-label="Observable">OBSERVABLE</div>
        </div>

        <TreeNav items={navTree} />

        <div className="sidebar-footer">
          <div className="field-label">Theme</div>
          <select
            aria-label="Theme preference"
            value={preference}
            onChange={(e) => setPreference(e.target.value as ThemePreference)}
            style={{ cursor: "pointer", background: "var(--surface)", color: "var(--text)", border: "1px solid var(--border-strong)", padding: "2px 6px", fontSize: "inherit", width: "100%" }}
          >
            {themeOptions.map((option) => (
              <option key={option.value} value={option.value}>{option.label}</option>
            ))}
          </select>
        </div>
      </aside>

      <div className="workspace">
        <header className="topbar">
          <div className="topbar-title">Platform — {tenantName}</div>
          <div className="topbar-controls" aria-label="Global context">
            <GlobalDateRangePicker />
            <select
              aria-label="Time display format"
              className="context-pill"
              value={format}
              onChange={(e) => setFormat(e.target.value as typeof format)}
              style={{ cursor: "pointer", background: "var(--surface)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: "var(--radius, 4px)", padding: "2px 6px", fontSize: "inherit" }}
            >
              {TIME_FORMAT_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>{opt.label}</option>
              ))}
            </select>
            <select
              aria-label="Tenant"
              className="context-pill"
              value={tenantId}
              onChange={(e) => {
                const selected = tenants.find((t) => t.id === e.target.value);
                if (selected) {
                  setTenant({ id: selected.id, name: selected.name });
                  window.location.reload();
                }
              }}
              style={{ cursor: "pointer", background: "var(--surface)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: "var(--radius, 4px)", padding: "2px 6px", fontSize: "inherit", maxWidth: "10rem" }}
            >
              {tenants.map((t) => (
                <option key={t.id} value={t.id}>{t.name}</option>
              ))}
            </select>
            <select
              aria-label="Environment"
              className="context-pill"
              value={environment ?? ""}
              onChange={(e) => {
                setEnvironment(e.target.value === "" ? null : e.target.value);
                window.location.reload();
              }}
              style={{ cursor: "pointer", background: "var(--surface)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: "var(--radius, 4px)", padding: "2px 6px", fontSize: "inherit", maxWidth: "9rem" }}
            >
              <option value="">All envs</option>
              {environments.map((env) => (
                <option key={env.environment} value={env.environment}>{env.environment}</option>
              ))}
            </select>
            <UserMenu />
          </div>
        </header>

        <main className="content-shell">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
