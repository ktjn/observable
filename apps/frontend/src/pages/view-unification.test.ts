import { describe, expect, it } from "vitest";
import productAreaSource from "./ProductAreaPage.tsx?raw";
import serviceDetailSource from "./ServiceDetailPage.tsx?raw";
import logSearchSource from "./LogSearch.tsx?raw";
import traceSearchSource from "./TraceSearch.tsx?raw";
import routerSource from "../router.ts?raw";
import signalExplorerSource from "../components/shared/SignalExplorer.tsx?raw";
import logListSource from "../components/shared/LogList.tsx?raw";

describe("view unification", () => {
  it("keeps ProductAreaPage focused on the canonical services catalog only", () => {
    expect(productAreaSource).not.toContain('"dashboards"');
    expect(productAreaSource).not.toContain('"alerts"');
    expect(productAreaSource).not.toContain('"admin"');
    expect(productAreaSource).not.toContain("This workspace will use the same dense operational layout");
  });

  it("uses a topology page name instead of a duplicate service overview page name", () => {
    expect(routerSource).toContain("ServiceTopologyPage");
    expect(routerSource).not.toContain("import ServiceOverview from");
  });

  it("routes all log viewing through the LogExplorer component", () => {
    expect(logSearchSource).toContain("export function LogExplorer");
    expect(serviceDetailSource).toContain("import { LogExplorer } from \"./LogSearch\"");
    expect(serviceDetailSource).not.toContain("searchLogs");
    expect(serviceDetailSource).not.toContain("LogResultsTable");
  });

  it("routes all trace viewing through the TraceExplorer component", () => {
    expect(traceSearchSource).toContain("export function TraceExplorer");
    expect(serviceDetailSource).toContain("import { TraceExplorer } from \"./TraceSearch\"");
    expect(serviceDetailSource).not.toContain("searchTraces");
    expect(serviceDetailSource).not.toContain("TraceResultsTable");
  });

  it("LogExplorer and TraceExplorer both delegate to the shared SignalExplorer shell", () => {
    expect(logSearchSource).toContain("import { SignalExplorer");
    expect(traceSearchSource).toContain("import { SignalExplorer");
  });

  it("SignalExplorer owns the panel/table layout and toolbar structure", () => {
    expect(signalExplorerSource).toContain("renderTable");
    expect(signalExplorerSource).toContain("renderPanel");
    expect(signalExplorerSource).toContain("w-1/4");
  });

  it("LogList is the shared mono log-row renderer", () => {
    expect(logListSource).toContain("export function LogList");
    expect(logListSource).toContain("pivotId");
    expect(logListSource).toContain("showTraceLink");
  });
});
