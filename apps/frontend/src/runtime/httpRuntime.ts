import { fetchTraceHistogram, searchTraces } from "../api/traces";
import type { RuntimeApi } from "./types";

/** Delegates to the existing production `fetch` calls with zero behavior change. */
export const httpRuntime: RuntimeApi = {
  mode: "http",
  traces: {
    search: searchTraces,
    histogram: fetchTraceHistogram,
  },
};
