import { fetchTraceHistogram, searchTraces } from "../api/traces";
import { listTenants, listEnvironments } from "../api/tenants";
import { submitNlqQuery } from "../api/nlq";
import { createDashboard } from "../api/dashboards";
import type { RuntimeApi } from "./types";

/** Delegates to the existing production `fetch` calls with zero behavior change. */
export const httpRuntime: RuntimeApi = {
  mode: "http",
  tenants: {
    list: listTenants,
    listEnvironments,
  },
  traces: {
    search: searchTraces,
    histogram: fetchTraceHistogram,
  },
  nlq: {
    execute: submitNlqQuery,
  },
  dashboards: {
    create: createDashboard,
  },
};
