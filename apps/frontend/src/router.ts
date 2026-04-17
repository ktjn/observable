import { createRouter, createRoute, createRootRoute } from "@tanstack/react-router";
import TraceSearch from "./pages/TraceSearch";

const rootRoute = createRootRoute();
const traceSearchRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: TraceSearch,
});
export const router = createRouter({
  routeTree: rootRoute.addChildren([traceSearchRoute]),
});
