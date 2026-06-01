import { createRouter, createRoute, createRootRoute } from "@tanstack/react-router";
import { AppShell } from "./components/AppShell";
import HomePage from "./pages/HomePage";
import AdminPage from "./pages/AdminPage";
import AdminConfigPage from "./pages/AdminConfigPage";
import AdminFleetPage from "./pages/AdminFleetPage";
import InfrastructureDetailPage from "./pages/InfrastructureDetailPage";
import InfrastructureInventoryPage from "./pages/InfrastructureInventoryPage";
import { AlertsPage } from "./features/alerts/AlertsPage";
import { AlertRuleDetailPage } from "./features/alerts/AlertRuleDetailPage";
import { IncidentsPage } from "./features/incidents/IncidentsPage";
import { IncidentDetailPage } from "./features/incidents/IncidentDetailPage";
import ServiceDetailPage from "./pages/ServiceDetailPage";
import ServicesPage from "./pages/ServicesPage";
import SetupPage from "./pages/SetupPage";
import SetupLlmPage from "./pages/SetupLlmPage";
import SetupTokensPage from "./pages/SetupTokensPage";
import TraceSearch from "./pages/TraceSearch";
import TraceComparePage from "./pages/TraceComparePage";
import TraceDetailPage from "./pages/TraceDetailPage";
import LogSearch from "./pages/LogSearch";
import MetricsSearch from "./pages/MetricsSearch";
import DashboardsPage from "./pages/DashboardsPage";
import DashboardDetailPage from "./pages/DashboardDetailPage";
import QueryWorkbenchPage from "./pages/QueryWorkbenchPage";
import NlqQueryPage from "./pages/NlqQueryPage";
import LoginPage from "./pages/LoginPage";
import AuthCallbackPage from "./pages/AuthCallbackPage";
import IdentitySettingsPage from "./pages/IdentitySettingsPage";

export type Preset = "5m" | "15m" | "30m" | "1h" | "3h" | "12h";
export const DEFAULT_PRESET: Preset = "1h";

export type RootSearch = {
  preset?: Preset;
  from?: number;
  to?: number;
  service?: string;
};

const VALID_PRESETS = new Set<string>(["5m", "15m", "30m", "1h", "3h", "12h"]);

const rootRoute = createRootRoute({
  component: AppShell,
  validateSearch: (search: Record<string, unknown>): RootSearch => {
    const raw = search.preset;
    const preset = typeof raw === "string" && VALID_PRESETS.has(raw)
      ? (raw as Preset)
      : undefined;
    const from = typeof search.from === "number" ? search.from
      : typeof search.from === "string" ? Number(search.from) || undefined
      : undefined;
    const to = typeof search.to === "number" ? search.to
      : typeof search.to === "string" ? Number(search.to) || undefined
      : undefined;
    const service = typeof search.service === "string" && search.service.trim()
      ? search.service.trim()
      : undefined;
    return { preset, from, to, service };
  },
});

const homeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: HomePage,
});
const servicesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/services",
  component: ServicesPage,
});
const setupRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/setup",
  component: SetupPage,
});
const setupLlmRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/setup/llm",
  component: SetupLlmPage,
});
const setupTokensRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/setup/tokens",
  component: SetupTokensPage,
});
const serviceDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/services/$serviceId",
  component: ServiceDetailPage,
});
const serviceLogsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/services/$serviceId/logs",
  component: ServiceDetailPage,
});
const serviceMetricsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/services/$serviceId/metrics",
  component: ServiceDetailPage,
});
const serviceTracesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/services/$serviceId/traces",
  component: ServiceDetailPage,
});
const serviceDeploymentsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/services/$serviceId/deployments",
  component: ServiceDetailPage,
});
const serviceAlertsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/services/$serviceId/alerts",
  component: ServiceDetailPage,
});
const serviceReliabilityRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/services/$serviceId/reliability",
  component: ServiceDetailPage,
});
const infrastructureRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/infrastructure",
  component: InfrastructureInventoryPage,
});
const infrastructureDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/infrastructure/$entityType/$entityId",
  component: InfrastructureDetailPage,
});
const serviceOverviewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/service-overview",
  component: ServicesPage,
});
const dashboardsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/dashboards",
  component: DashboardsPage,
});
const dashboardDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/dashboards/$dashboardId",
  component: DashboardDetailPage,
});
const alertsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/alerts",
  component: AlertsPage,
});
const alertRuleDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/alerts/$ruleId",
  component: AlertRuleDetailPage,
});
const incidentsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/incidents",
  component: IncidentsPage,
});
const incidentDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/incidents/$incidentId",
  component: IncidentDetailPage,
});
const adminRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/admin",
  component: AdminPage,
});
const identitySettingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/admin/identity",
  component: IdentitySettingsPage,
});
const adminConfigRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/admin/config",
  component: AdminConfigPage,
});
const adminFleetRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/admin/fleet",
  component: AdminFleetPage,
});
const workbenchRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/workbench",
  component: QueryWorkbenchPage,
  validateSearch: (search: Record<string, unknown>) => ({
    state: typeof search.state === "string" && search.state.trim() ? search.state.trim() : undefined,
  }),
});
const traceSearchRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/traces",
  component: TraceSearch,
});
const traceCompareRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/traces/compare",
  component: TraceComparePage,
  validateSearch: (search: Record<string, unknown>) => ({
    left: typeof search.left === "string" && search.left.trim() ? search.left.trim() : undefined,
    right: typeof search.right === "string" && search.right.trim() ? search.right.trim() : undefined,
  }),
});
const traceDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/traces/$traceId",
  component: TraceDetailPage,
});
const logSearchRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/logs",
  component: LogSearch,
});
const metricsSearchRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/metrics",
  component: MetricsSearch,
});
const nlqRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/nlq",
  component: NlqQueryPage,
});

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/login",
  component: LoginPage,
  validateSearch: (s: Record<string, unknown>) => ({
    error: typeof s.error === "string" ? s.error : undefined,
  }),
});

const authCallbackRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/auth/callback",
  component: AuthCallbackPage,
});

export const router = createRouter({
  routeTree: rootRoute.addChildren([
    homeRoute,
    setupRoute,
    setupLlmRoute,
    setupTokensRoute,
    loginRoute,
    authCallbackRoute,
    servicesRoute,
    serviceDetailRoute,
    serviceLogsRoute,
    serviceMetricsRoute,
    serviceTracesRoute,
    serviceDeploymentsRoute,
    serviceAlertsRoute,
    serviceReliabilityRoute,
    infrastructureRoute,
    infrastructureDetailRoute,
    serviceOverviewRoute,
    dashboardsRoute,
    dashboardDetailRoute,
    alertsRoute,
    alertRuleDetailRoute,
    incidentsRoute,
    incidentDetailRoute,
    adminRoute,
    identitySettingsRoute,
    adminConfigRoute,
    adminFleetRoute,
    workbenchRoute,
  traceSearchRoute,
  traceCompareRoute,
  traceDetailRoute,
    logSearchRoute,
    metricsSearchRoute,
    nlqRoute,
  ]),
});
