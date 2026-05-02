import { describe, expect, it } from "vitest";
import productAreaSource from "./ProductAreaPage.tsx?raw";
import serviceDetailSource from "./ServiceDetailPage.tsx?raw";
import logSearchSource from "./LogSearch.tsx?raw";
import traceSearchSource from "./TraceSearch.tsx?raw";
import routerSource from "../router.ts?raw";

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
});
