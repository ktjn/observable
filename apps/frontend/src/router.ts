import { createRouter, createRoute, createRootRoute } from "@tanstack/react-router";
import { AppShell } from "./components/AppShell";
import AdminPage from "./pages/AdminPage";
import InfrastructureDetailPage from "./pages/InfrastructureDetailPage";
import InfrastructureInventoryPage from "./pages/InfrastructureInventoryPage";
import { ProductAreaPage } from "./pages/ProductAreaPage";
import { AlertsPage } from "./features/alerts/AlertsPage";
import { IncidentsPage } from "./features/incidents/IncidentsPage";
import { IncidentDetailPage } from "./features/incidents/IncidentDetailPage";
import ServiceDetailPage from "./pages/ServiceDetailPage";
import ServiceTopologyPage from "./pages/ServiceTopologyPage";
import SetupPage from "./pages/SetupPage";
import SetupLlmPage from "./pages/SetupLlmPage";
import SetupTokensPage from "./pages/SetupTokensPage";
import TraceSearch from "./pages/TraceSearch";
import TraceDetailPage from "./pages/TraceDetailPage";
import LogSearch from "./pages/LogSearch";
import MetricsSearch from "./pages/MetricsSearch";
import DashboardsPage from "./pages/DashboardsPage";
import DashboardDetailPage from "./pages/DashboardDetailPage";
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
    return { preset, from, to };
  },
});

const homeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: ProductAreaPage,
});
const servicesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/services",
  component: ProductAreaPage,
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
  component: ServiceTopologyPage,
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
const traceSearchRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/traces",
  component: TraceSearch,
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
    infrastructureRoute,
    infrastructureDetailRoute,
    serviceOverviewRoute,
    dashboardsRoute,
    dashboardDetailRoute,
    alertsRoute,
    incidentsRoute,
    incidentDetailRoute,
    adminRoute,
    identitySettingsRoute,
    traceSearchRoute,
    traceDetailRoute,
    logSearchRoute,
    metricsSearchRoute,
    nlqRoute,
  ]),
});
