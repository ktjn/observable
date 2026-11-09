import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { createHealthReadMock } from "./assess-service-health.v0.1.mjs";

const { fixtures, scenarios } = JSON.parse(readFileSync(new URL("./assess-service-health.v0.1.json", import.meta.url)));
const now = new Date().toISOString();
const old = new Date(Date.now() - 3600_000).toISOString();
const assessment = (kind) => ({
  fresh: { state: "normal", evidence_status: "current", evaluated_at: now, last_evidence_at: now,
    conditions: [{ signal: "any", last_evidence_at: now, evidence_status: "current" }] },
  stale: { state: "unknown", evidence_status: "stale", evaluated_at: now, last_evidence_at: old,
    conditions: [{ signal: "any", last_evidence_at: old, evidence_status: "stale" }] },
  absent: { state: "unknown", evidence_status: "missing", evaluated_at: now, last_evidence_at: null,
    conditions: [{ signal: "any", last_evidence_at: null, evidence_status: "missing" }] },
})[kind];

// Evidence ceilings mirror migration 062: viewer reads up to internal, member up to confidential.
const ceiling = { viewer: ["public", "internal"], member: ["public", "internal", "confidential"] };

test("assess-service-health 0.1.0 read contract", async () => {
  let unavailable = false;
  const services = fixtures.map((f) => ({
    id: f.name, lifecycle: f.lifecycle, classification: f.classification, hidden: f.hidden ?? false,
    assessments: Object.fromEntries(
      ["production", "non_production"].filter((c) => f[c]).map((c) => [c, assessment(f[c])]),
    ),
  }));
  const mock = createHealthReadMock({
    services,
    canReadService: (s) => !s.hidden,
    canReadEvidence: (s, caller) => ceiling[caller].includes(s.classification),
    unavailable: () => unavailable,
  });
  const outages = [];
  for (const s of scenarios) {
    unavailable = s.unavailable ?? false;
    const url = new URL(`http://localhost/v1/health/services/${s.service}?environment_class=${s.class}`);
    const { status, body } = await mock(url, s.caller ?? "member");
    assert.equal(status, s.status, s.name);
    if (status !== 200) {
      assert.equal(body.error, s.error, s.name);
      assert.equal(body.state, undefined, s.name);
      if (unavailable) outages.push(JSON.stringify(body));
      continue;
    }
    assert.equal(body.service_id, s.service, s.name);
    assert.equal(body.environment_class, s.class, s.name);
    assert.equal(body.state, s.state, s.name);
    assert.equal(body.evidence_status, s.evidence_status, s.name);
    assert.equal(body.last_evidence_at !== null, s.times, s.name);
    if (s.evidence_status === "restricted") assert.deepEqual(body.conditions, [], s.name);
  }
  assert.equal(outages.length, 3);
  assert.equal(new Set(outages).size, 1);
});
