import { fetchTraceHistogram, getTrace, searchTraces } from "../api/traces";
import { fetchLogHistogram } from "../api/logs";
import { listTenants, listEnvironments } from "../api/tenants";
import { submitNlqQuery } from "../api/nlq";
import {
  createDashboard,
  listDashboards,
  getDashboard,
  updateDashboard,
  deleteDashboard,
  exportDashboard,
  importDashboard,
} from "../api/dashboards";
import {
  listServiceSummaries,
  listServices,
  getTopology,
  getServiceSummary,
  getServiceResponseTimeHistory,
} from "../api/services";
import { listMetrics, getMetricGroupPoints } from "../api/metrics";
import { listChangeEvents } from "../api/changeEvents";
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
    get: getTrace,
    histogram: fetchTraceHistogram,
  },
  logs: {
    histogram: fetchLogHistogram,
  },
  services: {
    list: listServiceSummaries,
    listNames: listServices,
    summary: getServiceSummary,
    responseTimeHistory: getServiceResponseTimeHistory,
  },
  topology: {
    get: getTopology,
  },
  metrics: {
    list: listMetrics,
    points: getMetricGroupPoints,
  },
  changeEvents: {
    list: listChangeEvents,
  },
  nlq: {
    execute: submitNlqQuery,
  },
  dashboards: {
    list: listDashboards,
    get: getDashboard,
    create: createDashboard,
    update: updateDashboard,
    delete: deleteDashboard,
    export: exportDashboard,
    import: importDashboard,
  },
};
