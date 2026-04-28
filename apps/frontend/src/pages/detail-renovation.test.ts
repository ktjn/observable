import { describe, expect, it } from "vitest";
import serviceDetailSource from "./ServiceDetailPage.tsx?raw";
import infrastructureDetailSource from "./InfrastructureDetailPage.tsx?raw";
import serviceInfraPanelSource from "../components/ServiceInfraPanel.tsx?raw";
import deploymentTimelineSource from "../components/DeploymentTimeline.tsx?raw";

const uiR1Targets = [
  ["pages/ServiceDetailPage.tsx", serviceDetailSource],
  ["pages/InfrastructureDetailPage.tsx", infrastructureDetailSource],
  ["components/ServiceInfraPanel.tsx", serviceInfraPanelSource],
  ["components/DeploymentTimeline.tsx", deploymentTimelineSource],
] as const;

const legacyPatterns = [
  "detail-panel",
  "detail-panel-header",
  "metric-tile",
  "signal-panel",
  "status ",
  "style={{",
];

describe("UI-R1 detail renovation", () => {
  it("keeps service and infrastructure detail surfaces on modern primitives", () => {
    const offenders = uiR1Targets.flatMap(([target, source]) =>
      legacyPatterns
        .filter((pattern) => source.includes(pattern))
        .map((pattern) => `${target}: ${pattern}`)
    );

    expect(offenders).toEqual([]);
  });
});
