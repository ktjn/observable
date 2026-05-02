import { describe, expect, it } from "vitest";
import productAreaSource from "./ProductAreaPage.tsx?raw";
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
});
