import { test, expect, type Page } from "@playwright/test";

const TENANT_ID = "00000000-0000-0000-0000-000000000001";
const INCIDENT_ID = "11111111-1111-1111-1111-111111111111";

interface IncidentMockOpts {
  owner?: string | null;
  impact_summary?: string | null;
  status?: string;
  timeline?: Array<{
    event_time: string;
    event_type: string;
    actor: string;
    message: string | null;
    clock_uncertain?: boolean;
  }>;
  findings?: Array<{
    id: string;
    title: string;
    kind: "observed_fact" | "relationship" | "hypothesis" | "automated_suggestion" | "confirmed_cause";
    verified?: boolean;
    evidence_refs?: string[];
    summary?: string;
  }>;
}

async function mockIncidentDetailApi(page: Page, opts: IncidentMockOpts = {}) {
  const triggeredAt = "2026-11-14T09:00:00Z";
  let current = {
    incident_id: INCIDENT_ID,
    title: "Checkout 5xx elevated error rate",
    severity: "critical",
    status: opts.status ?? "triggered",
    dedup_key: "dedup-checkout-5xx",
    triggered_at: triggeredAt,
    acknowledged_at: null as string | null,
    acknowledged_by: null as string | null,
    resolved_at: null as string | null,
    resolved_by: null as string | null,
    triggered_by_rule_id: "22222222-2222-2222-2222-222222222222",
    related_deployment_id: null,
    runbook_url: "https://runbooks.example.com/checkout-5xx",
    rule_name: "Checkout 5xx",
    impacted_service: "checkout",
    affected_services: ["checkout", "payments"],
    owner: opts.owner ?? null,
    impact_summary: opts.impact_summary ?? null,
    timeline: opts.timeline ?? [
      {
        event_time: triggeredAt,
        event_type: "triggered",
        actor: "alert-evaluator",
        message: "Error rate > 5%",
      },
    ],
    findings: opts.findings ?? [],
  };

  await page.route("**/v1/auth/me", (route) =>
    route.fulfill({
      json: {
        user_id: "00000000-0000-0000-0000-000000000001",
        email: "responder@example.com",
        tenants: [{ tenant_id: TENANT_ID, role: "admin" }],
      },
    }),
  );
  await page.route("**/v1/tenants", (route) =>
    route.fulfill({ json: { tenants: [{ id: TENANT_ID, name: "observable" }] } }),
  );
  await page.route("**/v1/tenants/**/environments", (route) =>
    route.fulfill({ json: { environments: [{ environment: "prod" }] } }),
  );
  await page.route("**/v1/topology**", (route) =>
    route.fulfill({ json: { edges: [] } }),
  );
  await page.route(`**/v1/incidents/${INCIDENT_ID}`, (route) =>
    route.fulfill({ json: current }),
  );
  await page.route(`**/v1/incidents/${INCIDENT_ID}/acknowledge`, async (route) => {
    const body = route.request().postDataJSON() as { actor?: string; message?: string };
    const actor = body?.actor ?? "responder@example.com";
    current = {
      ...current,
      status: "acknowledged",
      owner: actor,
      acknowledged_by: actor,
      acknowledged_at: "2026-11-14T09:02:00Z",
      timeline: [
        ...current.timeline,
        {
          event_time: "2026-11-14T09:02:00Z",
          event_type: "acknowledged",
          actor,
          message: body?.message ?? "Acknowledged",
        },
      ],
    };
    await route.fulfill({ json: current });
  });
  await page.route(`**/v1/incidents/${INCIDENT_ID}/escalate`, async (route) => {
    current = {
      ...current,
      timeline: [
        ...current.timeline,
        {
          event_time: "2026-11-14T09:03:00Z",
          event_type: "escalated",
          actor: "responder@example.com",
          message: "Escalated to secondary-oncall",
        },
      ],
    };
    await route.fulfill({ json: current });
  });
  await page.route(`**/v1/incidents/${INCIDENT_ID}/status`, async (route) => {
    const body = route.request().postDataJSON() as { status: string; actor?: string; message?: string };
    current = {
      ...current,
      status: body.status,
      timeline: [
        ...current.timeline,
        {
          event_time: "2026-11-14T09:04:00Z",
          event_type: "status_change",
          actor: body.actor ?? "responder@example.com",
          message: body.message ?? `Status changed to ${body.status}`,
        },
      ],
    };
    await route.fulfill({ json: current });
  });
  await page.route("**/v1/services/checkout/**", (route) =>
    route.fulfill({ json: { buckets: [], slos: [], service: { service_name: "checkout", request_rate: 10, request_count: 100, error_rate: 0.08, p95_latency_ms: 400, slo_breaching: true, active_alert_count: 1 } } }),
  );
  await page.route("**/v1/deployments**", (route) => route.fulfill({ json: { items: [] } }));
  await page.route("**/v1/events/changes**", (route) => route.fulfill({ json: { items: [] } }));
  await page.route("**/v1/logs**", (route) => route.fulfill({ json: { logs: [], total: 0 } }));
}

test.describe("Phase 6 S3: Incident triage and investigation UX (OBS-23-01..05, OBS-23-11)", () => {
  test("OBS-23-01: shows severity, status, ownership, impacted services, and Unknown when business impact is unknown", async ({
    page,
  }) => {
    await mockIncidentDetailApi(page, {
      owner: "sre-oncall",
      impact_summary: null,
    });
    await page.goto(`/incidents/${INCIDENT_ID}`);

    await expect(page.getByText("critical")).toBeVisible();
    await expect(page.getByText("Triggered").first()).toBeVisible();
    await expect(page.getByText("sre-oncall")).toBeVisible();
    await expect(page.getByRole("link", { name: "checkout", exact: true })).toBeVisible();
    await expect(page.getByRole("link", { name: "payments", exact: true })).toBeVisible();
    await expect(page.getByTestId("incident-business-impact")).toHaveText("Unknown");
  });

  test("OBS-23-02: chronological timeline orders events consistently and flags clock-uncertain events", async ({
    page,
  }) => {
    await mockIncidentDetailApi(page, {
      timeline: [
        {
          event_time: "2026-11-14T09:05:00Z",
          event_type: "acknowledged",
          actor: "alice",
          message: "Acked after page",
        },
        {
          event_time: "2026-11-14T09:00:00Z",
          event_type: "triggered",
          actor: "alert-evaluator",
          message: "Initial alert",
        },
        {
          event_time: "2026-11-14T09:02:00Z",
          event_type: "comment",
          actor: "edge-collector",
          message: "Skewed node report",
          clock_uncertain: true,
        },
      ],
    });
    await page.goto(`/incidents/${INCIDENT_ID}`);

    await expect(page.getByText("Clock uncertain")).toBeVisible();
    const text = await page.locator("body").innerText();
    const idxTriggered = text.indexOf("Initial alert");
    const idxSkewed = text.indexOf("Skewed node report");
    const idxAcked = text.indexOf("Acked after page");
    expect(idxTriggered).toBeGreaterThan(-1);
    expect(idxSkewed).toBeGreaterThan(idxTriggered);
    expect(idxAcked).toBeGreaterThan(idxSkewed);
  });

  test("OBS-23-03: navigating from incident to service detail retains the incident time window", async ({
    page,
  }) => {
    await mockIncidentDetailApi(page);
    await page.goto(`/incidents/${INCIDENT_ID}`);

    const detailLink = page.getByRole("link", { name: "→ Service Detail" });
    await expect(detailLink).toBeVisible();
    const href = await detailLink.getAttribute("href");
    expect(href).toContain("/services/checkout");
    expect(href).toContain("2026-11-14T09%3A00%3A00Z");

    const logsLink = page.getByRole("link", { name: "→ Observed Errors" });
    const logsHref = await logsLink.getAttribute("href");
    expect(logsHref).toContain("/logs");
    expect(logsHref).toContain("2026-11-14T09%3A00%3A00Z");
  });

  test("OBS-23-04 & OBS-23-11: distinguishes observed facts, relationships, unverified hypotheses/suggestions, and confirmed conclusions", async ({
    page,
  }) => {
    await mockIncidentDetailApi(page, {
      findings: [
        {
          id: "f1",
          title: "HTTP 502 spike on checkout",
          kind: "observed_fact",
          summary: "Observed 8% 502 responses from payments",
        },
        {
          id: "f2",
          title: "checkout -> payments latency correlation",
          kind: "relationship",
        },
        {
          id: "f3",
          title: "Automated suggestion: DNS cache expiry",
          kind: "automated_suggestion",
          verified: true, // even if mistakenly flagged, automated_suggestion cannot get confirmed styling
          evidence_refs: [],
        },
        {
          id: "f4",
          title: "Connection pool exhaustion in v2.1.0",
          kind: "confirmed_cause",
          verified: true,
          evidence_refs: ["trace-abc-123"],
        },
      ],
    });
    await page.goto(`/incidents/${INCIDENT_ID}`);

    const findings = page.getByTestId("incident-findings");
    await expect(findings.getByText("Observed evidence")).toBeVisible();
    await expect(findings.getByText("Relationship")).toBeVisible();
    await expect(findings.getByText("Unverified hypothesis")).toBeVisible();
    await expect(findings.getByText("Confirmed cause")).toBeVisible();

    const suggestionRow = findings.locator('[data-finding-kind="automated_suggestion"]');
    await expect(suggestionRow).toHaveAttribute("data-finding-verified", "false");
  });

  test("OBS-23-05: responder can assign, escalate, and update incident state with actor attribution", async ({
    page,
  }) => {
    await mockIncidentDetailApi(page);
    await page.goto(`/incidents/${INCIDENT_ID}`);

    await page.getByLabel("Assign owner").fill("alice");
    await page.getByRole("button", { name: "Assign" }).click();
    await expect(page.getByText("Assigned incident to alice")).toBeVisible();

    await page.getByRole("button", { name: "Escalate" }).click();
    await expect(page.getByText("Escalated to secondary-oncall").first()).toBeVisible();

    await page.getByRole("button", { name: "Investigate" }).click();
    await expect(page.getByText("Status changed to investigating")).toBeVisible();
  });
});
