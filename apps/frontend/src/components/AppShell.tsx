import { Outlet } from "@tanstack/react-router";
import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useTheme, type ThemePreference } from "../lib/theme";
import { useTimeDisplay, TIME_FORMAT_OPTIONS } from "../lib/timeDisplay";
import { GlobalDateRangePicker } from "./GlobalDateRangePicker";
import { useTenantContext } from "../hooks/useTenantContext";
import { listTenants, listEnvironments } from "../api/tenants";
import { Select, SelectOption } from "./ui/select";

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
          <div className="segmented-control" role="radiogroup" aria-label="Theme preference">
            {themeOptions.map((option) => (
              <button
                key={option.value}
                type="button"
                role="radio"
                aria-checked={preference === option.value}
                className={preference === option.value ? "segment active" : "segment"}
                onClick={() => setPreference(option.value)}
              >
                {option.label}
              </button>
            ))}
          </div>
        </div>
      </aside>

      <div className="workspace">
        <header className="topbar">
          <div className="topbar-title">Platform — {tenantName}</div>
          <div className="topbar-controls" aria-label="Global context">
            <Select
              aria-label="Tenant"
              value={tenantId}
              onChange={(e) => {
                const selected = tenants.find((t) => t.id === e.target.value);
                if (selected) setTenant({ id: selected.id, name: selected.name });
              }}
            >
              {tenants.map((t) => (
                <SelectOption key={t.id} value={t.id}>{t.name}</SelectOption>
              ))}
            </Select>
            <Select
              aria-label="Environment"
              value={environment ?? ""}
              onChange={(e) => setEnvironment(e.target.value === "" ? null : e.target.value)}
            >
              <SelectOption value="">All environments</SelectOption>
              {environments.map((e) => (
                <SelectOption key={e.environment} value={e.environment}>{e.environment}</SelectOption>
              ))}
            </Select>
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
            <GlobalDateRangePicker />
          </div>
        </header>

        <main className="content-shell">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
