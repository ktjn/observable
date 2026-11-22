import { Outlet } from "@tanstack/react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTheme, type ThemePreference } from "../lib/theme";
import { useTimeDisplay, TIME_FORMAT_OPTIONS } from "../lib/timeDisplay";
import { GlobalDateRangePicker } from "./GlobalDateRangePicker";
import { UserMenu } from "./UserMenu";
import { Button } from "./ui/button";
import { ConfirmDialog } from "./ui/confirm-dialog";
import { useTenantContext } from "../hooks/useTenantContext";
import { useRuntime } from "../hooks/useRuntime";
import { useEffect, useState } from "react";
import { useLocation } from "@tanstack/react-router";
import { useAuth } from "../hooks/useAuth";
import { initiateLogin } from "../api/auth";
import { useTenantSwitch } from "../hooks/useTenantSwitch";
import { TreeNav, type NavTreeItem } from "./TreeNav";
import { isOnboardingComplete } from "../features/onboarding/onboardingState";
import { CommandPalette } from "./ui/command-palette";
import { catalogModule } from "../features/catalog/module";
import { ScopeBar } from "./ScopeBar";
import {
  Home as HomeIcon,
  Database,
  Workflow,
  Network,
  LayoutDashboard,
  BellRing,
  Siren,
  Settings,
  Server,
  HeartPulse,
  Search,
  Users,
  FileBarChart2,
} from "lucide-react";

function buildNavTree(showGettingStarted: boolean): NavTreeItem[] {
  const base: NavTreeItem[] = [
    { id: "home", label: "Home", to: "/", icon: HomeIcon },
    { id: "service-health", label: "Service Health", to: "/catalog?view=overview", icon: HeartPulse },
    { id: "incidents", label: "Incidents", to: "/incidents", icon: Siren },
    { id: "investigation", label: "Investigation", to: "/traces", icon: Search },
    { id: "customer-experience", label: "Customer Experience", to: "/services", icon: Users },
    { id: "reporting", label: "Reporting", to: "/catalog?view=review", icon: FileBarChart2 },
    { id: "services", label: "Services", to: "/services", icon: Workflow },
    catalogModule.navigation,
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
    { id: "workbench", label: "Workbench", to: "/workbench", icon: Database },
    {
      id: "admin",
      label: "Administration",
      icon: Settings,
      children: [
        { id: "admin-overview", label: "Overview", to: "/admin" },
        { id: "admin-members", label: "Members", to: "/admin/members" },
        { id: "admin-fleet", label: "Fleet management", to: "/admin/fleet" },
        { id: "setup-ingest", label: "Ingest", to: "/setup" },
        { id: "setup-llm", label: "LLM", to: "/setup/llm" },
        { id: "setup-tokens", label: "Tokens", to: "/setup/tokens" },
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
];

export function AppShell() {
  const { preference, setPreference } = useTheme();
  const { format, setFormat } = useTimeDisplay();
  const { tenantId, tenantName, environment, setTenant, setEnvironment } = useTenantContext();
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [resetting, setResetting] = useState(false);
  const [confirmResetOpen, setConfirmResetOpen] = useState(false);
  const runtime = useRuntime();
  const queryClient = useQueryClient();
  const switchToTenant = useTenantSwitch();
  const isPlayground = import.meta.env.VITE_OBSERVABLE_RUNTIME === "playground";

  async function handleResetPlayground() {
    setResetting(true);
    try {
      // Dynamically imported (not a static import): engineClient statically
      // references a Worker URL, which Vite's dep scanner eagerly resolves
      // at import time — see the identical reasoning in playgroundRuntime.ts.
      const { resetPlayground } = await import("../playground/engineClient");
      await resetPlayground();
      await queryClient.invalidateQueries();
    } finally {
      setResetting(false);
      setConfirmResetOpen(false);
    }
  }

  const { data: tenantsData } = useQuery({
    queryKey: ["tenants"],
    queryFn: runtime.tenants.list,
  });

  const { data: environmentsData } = useQuery({
    queryKey: ["environments", tenantId],
    queryFn: () => runtime.tenants.listEnvironments(tenantId),
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

  // After login, sync the UI tenant context with the server-authoritative active
  // tenant on the session (if valid), or fall back to the first available tenant.
  useEffect(() => {
    const available = tenantsData?.tenants;
    if (!user || !available || available.length === 0) return;
    const serverTenant = user.active_tenant_id
      ? available.find((t) => t.id === user.active_tenant_id)
      : undefined;
    if (serverTenant && serverTenant.id !== tenantId) {
      setTenant({ id: serverTenant.id, name: serverTenant.name });
      return;
    }
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
              onChange={async (e) => {
                const selected = tenants.find((t) => t.id === e.target.value);
                if (!selected) return;
                await switchToTenant({ id: selected.id, name: selected.name });
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
              }}
            >
              <option value="">All envs</option>
              {environments.map((env) => (
                <option key={env.environment} value={env.environment}>{env.environment}</option>
              ))}
            </select>
            {isPlayground && (
              <Button
                variant="secondary"
                className="context-pill"
                onClick={() => setConfirmResetOpen(true)}
                disabled={resetting}
              >
                {resetting ? "Resetting…" : "Reset playground"}
              </Button>
            )}
            <UserMenu />
          </div>
        </header>

        <ConfirmDialog
          open={confirmResetOpen}
          title="Reset Playground"
          description="Reset the playground? All local data will be wiped and regenerated."
          confirmLabel="Reset Playground"
          cancelLabel="Cancel"
          variant="destructive"
          isPending={resetting}
          onConfirm={handleResetPlayground}
          onCancel={() => setConfirmResetOpen(false)}
        />

        {isPlayground && (
          <div className="playground-banner" role="note">
            Playground mode — all data is synthetic and generated locally in your browser. This is not
            connected to any backend and does not reflect production behavior.
          </div>
        )}

        <ScopeBar />

        <main className="content-shell">
          <Outlet />
        </main>
      </div>

      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        tenantId={tenantId}
        runtime={runtime}
      />
    </div>
  );
}
