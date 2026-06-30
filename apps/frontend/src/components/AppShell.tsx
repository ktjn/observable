import { Outlet } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useTheme, type ThemePreference } from "../lib/theme";
import { useTimeDisplay, TIME_FORMAT_OPTIONS } from "../lib/timeDisplay";
import { GlobalDateRangePicker } from "./GlobalDateRangePicker";
import { UserMenu } from "./UserMenu";
import { useTenantContext } from "../hooks/useTenantContext";
import { listTenants, listEnvironments } from "../api/tenants";
import { useEffect, useState } from "react";
import { useLocation } from "@tanstack/react-router";
import { useAuth } from "../hooks/useAuth";
import { initiateLogin } from "../api/auth";
import { TreeNav, type NavTreeItem } from "./TreeNav";
import { isOnboardingComplete } from "../features/onboarding/onboardingState";
import { CommandPalette } from "./ui/command-palette";
import {
  Home as HomeIcon,
  Wrench,
  Database,
  Workflow,
  Network,
  LayoutDashboard,
  BellRing,
  Siren,
  Settings,
  Server,
} from "lucide-react";

function buildNavTree(showGettingStarted: boolean): NavTreeItem[] {
  const base: NavTreeItem[] = [
    { id: "home", label: "Home", to: "/", icon: HomeIcon },
    {
      id: "setup",
      label: "Setup",
      icon: Wrench,
      children: [
        { id: "setup-ingest", label: "Ingest", to: "/setup" },
        { id: "setup-llm", label: "LLM", to: "/setup/llm" },
        { id: "setup-tokens", label: "Tokens", to: "/setup/tokens" },
      ],
    },
    { id: "workbench", label: "Workbench", to: "/workbench", icon: Database },
    { id: "services", label: "Services", to: "/services", icon: Workflow },
    {
      id: "signals",
      label: "Signals",
      icon: Network,
      children: [
        { id: "traces", label: "Traces", to: "/traces" },
        { id: "logs", label: "Logs", to: "/logs" },
        { id: "metrics", label: "Metrics", to: "/metrics" },
        { id: "change-events", label: "Change Events", to: "/change-events" },
      ],
    },
    { id: "infrastructure", label: "Infrastructure", to: "/infrastructure", icon: Server },
    { id: "dashboards", label: "Dashboards", to: "/dashboards", icon: LayoutDashboard },
    { id: "alerts", label: "Alerts & SLOs", to: "/alerts", icon: BellRing },
    { id: "incidents", label: "Incidents", to: "/incidents", icon: Siren },
    {
      id: "admin",
      label: "Administration",
      icon: Settings,
      children: [
        { id: "admin-overview", label: "Overview", to: "/admin" },
        { id: "admin-config", label: "Tenant configuration", to: "/admin/config" },
        { id: "admin-fleet", label: "Fleet management", to: "/admin/fleet" },
        { id: "admin-identity", label: "Identity", to: "/admin/identity" },
      ],
    },
  ];
  if (showGettingStarted) {
    return [{ id: "getting-started", label: "Getting Started ✦", to: "/getting-started" }, ...base];
  }
  return base;
}

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
  const [paletteOpen, setPaletteOpen] = useState(false);

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

  const [navTree, setNavTree] = useState<NavTreeItem[]>(() =>
    buildNavTree(!isOnboardingComplete()),
  );

  useEffect(() => {
    setNavTree(buildNavTree(!isOnboardingComplete()));
  }, [location.pathname]);

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

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if ((e.ctrlKey || e.metaKey) && e.key === "k") {
        e.preventDefault();
        setPaletteOpen(true);
      }
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, []);

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
            className="themed-select"
            value={preference}
            onChange={(e) => setPreference(e.target.value as ThemePreference)}
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
              className="context-pill themed-select"
              value={format}
              onChange={(e) => setFormat(e.target.value as typeof format)}
            >
              {TIME_FORMAT_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>{opt.label}</option>
              ))}
            </select>
            <select
              aria-label="Tenant"
              className="context-pill themed-select"
              value={tenantId}
              onChange={(e) => {
                const selected = tenants.find((t) => t.id === e.target.value);
                if (selected) {
                  setTenant({ id: selected.id, name: selected.name });
                  window.location.reload();
                }
              }}
            >
              {tenants.map((t) => (
                <option key={t.id} value={t.id}>{t.name}</option>
              ))}
            </select>
            <select
              aria-label="Environment"
              className="context-pill themed-select"
              value={environment ?? ""}
              onChange={(e) => {
                setEnvironment(e.target.value === "" ? null : e.target.value);
                window.location.reload();
              }}
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

      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
    </div>
  );
}
