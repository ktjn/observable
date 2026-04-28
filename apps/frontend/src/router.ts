import { createRouter, createRoute, createRootRoute } from "@tanstack/react-router";
import { createElement } from "react";
import { AppShell } from "./components/AppShell";
import InfrastructureDetailPage from "./pages/InfrastructureDetailPage";
import InfrastructureInventoryPage from "./pages/InfrastructureInventoryPage";
import { ProductAreaPage } from "./pages/ProductAreaPage";
import { AlertsPage } from "./features/alerts/AlertsPage";
import ServiceDetailPage from "./pages/ServiceDetailPage";
import ServiceOverview from "./pages/ServiceOverview";
import SetupPage from "./pages/SetupPage";
import TraceSearch from "./pages/TraceSearch";
import TraceDetailPage from "./pages/TraceDetailPage";
import LogSearch from "./pages/LogSearch";

const rootRoute = createRootRoute({
  component: AppShell,
});

const homeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: () => createElement(ProductAreaPage, { area: "services" }),
});
const servicesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/services",
  component: () => createElement(ProductAreaPage, { area: "services" }),
});
const setupRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/setup",
  component: SetupPage,
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
  component: ServiceOverview,
});
const dashboardsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/dashboards",
  component: () => createElement(ProductAreaPage, { area: "dashboards" }),
});
const alertsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/alerts",
  component: AlertsPage,
});
const adminRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/admin",
  component: () => createElement(ProductAreaPage, { area: "admin" }),
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
export const router = createRouter({
  routeTree: rootRoute.addChildren([
    homeRoute,
    setupRoute,
    servicesRoute,
    serviceDetailRoute,
    serviceLogsRoute,
    serviceMetricsRoute,
    serviceTracesRoute,
    infrastructureRoute,
    infrastructureDetailRoute,
    serviceOverviewRoute,
    dashboardsRoute,
    alertsRoute,
    adminRoute,
    traceSearchRoute,
    traceDetailRoute,
    logSearchRoute,
  ]),
});
