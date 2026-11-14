import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { IncidentInvestigationP1Panel } from "./IncidentInvestigationP1Panel";

test("OBS-23-06: responder sees a related deployment in the incident timeline", () => {
  render(
    <IncidentInvestigationP1Panel
      deployments={[
        {
          eventTime: "2026-11-14T12:00:00Z",
          deploymentId: "dep-884",
          version: "checkout@v2.4.1",
          actor: "ci-bot",
        },
      ]}
      lifecycleStages={[]}
      selectedObservations={[]}
      openQuestions={[]}
    />,
  );
  expect(screen.getByText("Related deployments in timeline")).toBeTruthy();
  expect(screen.getByText("checkout@v2.4.1")).toBeTruthy();
  expect(screen.getByText("(dep-884)")).toBeTruthy();
});

test("OBS-23-07: shift handover preserves documented evidence, decisions, and open questions", () => {
  render(
    <IncidentInvestigationP1Panel
      deployments={[]}
      lifecycleStages={[]}
      handover={{
        fromResponder: "alice",
        toResponder: "bob",
        handedOverAt: "2026-11-14T12:30:00Z",
        evidenceRefs: ["trace:9912-slow-db"],
        decisions: ["Rolled back feature flag checkout_v2"],
        openQuestions: ["Does connection pool leak persist under EU traffic?"],
      }}
      selectedObservations={[]}
      openQuestions={[]}
    />,
  );
  expect(screen.getByText("Shift handover")).toBeTruthy();
  expect(screen.getByText("trace:9912-slow-db")).toBeTruthy();
  expect(screen.getByText("Rolled back feature flag checkout_v2")).toBeTruthy();
  expect(screen.getByText("Does connection pool leak persist under EU traffic?")).toBeTruthy();
});

test("OBS-23-08: reviewer can follow the incident from detection through resolution", () => {
  render(
    <IncidentInvestigationP1Panel
      deployments={[]}
      lifecycleStages={[
        {
          stage: "detected",
          timestamp: "2026-11-14T12:00:00Z",
          actor: "alert-evaluator",
          summary: "SLO burn alert triggered",
        },
        {
          stage: "acknowledged",
          timestamp: "2026-11-14T12:02:00Z",
          actor: "alice",
          summary: "Responder engaged",
        },
        {
          stage: "mitigated",
          timestamp: "2026-11-14T12:18:00Z",
          actor: "alice",
          summary: "Traffic shifted away from degraded zone",
        },
        {
          stage: "resolved",
          timestamp: "2026-11-14T12:40:00Z",
          actor: "bob",
          summary: "Error rate normalized for 20m",
        },
      ]}
      selectedObservations={[]}
      openQuestions={[]}
    />,
  );
  expect(screen.getByText("detected")).toBeTruthy();
  expect(screen.getByText("acknowledged")).toBeTruthy();
  expect(screen.getByText("mitigated")).toBeTruthy();
  expect(screen.getByText("resolved")).toBeTruthy();
  expect(screen.getByText("Error rate normalized for 20m")).toBeTruthy();
});

test("OBS-23-09: responder inspects evidence and basis for critical status without hidden logic", () => {
  render(
    <IncidentInvestigationP1Panel
      deployments={[]}
      lifecycleStages={[]}
      criticalBasis={{
        statusLabel: "critical",
        ruleName: "Checkout error rate > 5%",
        thresholdBasis: "5.0% over 5m window",
        observedValue: "14.2% (426 / 3000 requests)",
        evidenceRefs: ["metric:http_5xx_ratio@12:00Z", "log:pool_exhausted#881"],
      }}
      selectedObservations={[]}
      openQuestions={[]}
    />,
  );
  expect(screen.getByText("Critical status basis")).toBeTruthy();
  expect(screen.getByText("Checkout error rate > 5%")).toBeTruthy();
  expect(screen.getByText("Threshold: 5.0% over 5m window")).toBeTruthy();
  expect(screen.getByText("Observed: 14.2% (426 / 3000 requests)")).toBeTruthy();
  expect(screen.getByText("metric:http_5xx_ratio@12:00Z")).toBeTruthy();
});

test("OBS-23-10: second responder opens incident and reviews selected observations and open questions", () => {
  render(
    <IncidentInvestigationP1Panel
      deployments={[]}
      lifecycleStages={[]}
      viewerRole="second-responder:bob"
      selectedObservations={[
        {
          id: "obs-1",
          summary: "p99 latency spiked in us-east-1b after pool resize",
          selectedBy: "alice",
        },
      ]}
      openQuestions={["Verify replica lag in us-east-1b"]}
    />,
  );
  expect(screen.getByText("Opened as second-responder:bob")).toBeTruthy();
  expect(
    screen.getByText("p99 latency spiked in us-east-1b after pool resize (selected by alice)"),
  ).toBeTruthy();
  expect(screen.getByText("Verify replica lag in us-east-1b")).toBeTruthy();
});
