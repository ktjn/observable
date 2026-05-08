import { Outlet } from "@tanstack/react-router";
import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useTheme, type ThemePreference } from "../lib/theme";
import { useTimeDisplay, TIME_FORMAT_OPTIONS } from "../lib/timeDisplay";
import { GlobalDateRangePicker } from "./GlobalDateRangePicker";
import { UserMenu } from "./UserMenu";
import { useTenantContext } from "../hooks/useTenantContext";
import { listTenants, listEnvironments } from "../api/tenants";
import { useEffect } from "react";
import { useLocation, useNavigate } from "@tanstack/react-router";
import { useAuth } from "../hooks/useAuth";

const navItems = [
  { label: "Setup", to: "/setup" },
  { label: "Ask (NLQ)", to: "/nlq" },
  { label: "Services", to: "/services" },
  { label: "Traces", to: "/traces" },
  { label: "Logs", to: "/logs" },
  { label: "Metrics", to: "/metrics" },
  { label: "Infrastructure", to: "/infrastructure" },
  { label: "Service Overview", to: "/service-overview" },
  { label: "Dashboards", to: "/dashboards" },
  { label: "Alerts & SLOs", to: "/alerts" },
  { label: "Admin / Fleet / Billing", to: "/admin" },
] as const;

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
  const navigate = useNavigate();

  useEffect(() => {
    const pathname = location.pathname;
    if (!authLoading && !user && !/^\/(login|auth\/callback)$/.test(pathname)) {
      navigate({ to: "/login" });
    }
  }, [authLoading, user, location.pathname, navigate]);

  return (
    <div className="app-shell">
      <aside className="sidebar" aria-label="Primary navigation">
        <div className="brand-lockup">
          <div className="brand-mark" aria-label="Observable">OBSERVABLE</div>
        </div>

        <nav className="nav-list">
          {navItems.map((item) => (
            <Link
              key={item.to}
              to={item.to}
              className="nav-link"
              activeProps={{ className: "nav-link active" }}
            >
              {item.label}
            </Link>
          ))}
        </nav>

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
