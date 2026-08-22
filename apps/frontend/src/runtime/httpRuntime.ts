import { fetchTraceHistogram, searchTraces } from "../api/traces";
import { fetchLogHistogram } from "../api/logs";
import { listTenants, listEnvironments } from "../api/tenants";
import { submitNlqQuery } from "../api/nlq";
import { createDashboard } from "../api/dashboards";
import { listServiceSummaries, listServices, getTopology } from "../api/services";
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
  logs: {
    histogram: fetchLogHistogram,
  },
  services: {
    list: listServiceSummaries,
    listNames: listServices,
  },
  topology: {
    get: getTopology,
  },
  nlq: {
    execute: submitNlqQuery,
  },
  dashboards: {
    create: createDashboard,
  },
};
