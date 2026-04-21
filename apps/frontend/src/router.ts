import { createRouter, createRoute, createRootRoute } from "@tanstack/react-router";
import { createElement } from "react";
import { AppShell } from "./components/AppShell";
import { ProductAreaPage } from "./pages/ProductAreaPage";
import TraceSearch from "./pages/TraceSearch";
import TraceDetailPage from "./pages/TraceDetailPage";

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
const infrastructureRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/infrastructure",
  component: () => createElement(ProductAreaPage, { area: "infrastructure" }),
});
const serviceOverviewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/service-overview",
  component: () => createElement(ProductAreaPage, { area: "service-overview" }),
});
const dashboardsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/dashboards",
  component: () => createElement(ProductAreaPage, { area: "dashboards" }),
});
const alertsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/alerts",
  component: () => createElement(ProductAreaPage, { area: "alerts" }),
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
export const router = createRouter({
  routeTree: rootRoute.addChildren([
    homeRoute,
    servicesRoute,
    infrastructureRoute,
    serviceOverviewRoute,
    dashboardsRoute,
    alertsRoute,
    adminRoute,
    traceSearchRoute,
    traceDetailRoute,
  ]),
});
