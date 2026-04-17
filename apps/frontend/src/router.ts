import { createRouter, createRoute, createRootRoute } from "@tanstack/react-router";
import TraceSearch from "./pages/TraceSearch";
import TraceDetailPage from "./pages/TraceDetailPage";

const rootRoute = createRootRoute();
const traceSearchRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: TraceSearch,
});
const traceDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/traces/$traceId",
  component: TraceDetailPage,
});
export const router = createRouter({
  routeTree: rootRoute.addChildren([traceSearchRoute, traceDetailRoute]),
});
