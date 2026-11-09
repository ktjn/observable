/**
 * Programmable read mock for assess-service-health 0.1.0
 * (`GET /v1/health/services/{id}?environment_class=`).
 * The harness supplies assessments, access decisions and failure conditions. Like the
 * provider it checks the request-level decision before any lookup, answers missing, hidden
 * and non-live services with one 404, and withholds the whole assessment when a contributor
 * is restricted.
 */
const CLASSES = ["production", "non_production"];

export function createHealthReadMock({
  services, canReadService = () => true, canReadEvidence = () => true,
  unavailable = () => false, delayMs = 0,
}) {
  return async (url, caller = "member") => {
    const match = /^\/v1\/health\/services\/([^/]+)$/.exec(url.pathname);
    if (!match) return undefined;
    if (delayMs > 0) await new Promise((resolve) => setTimeout(resolve, delayMs));
    const environment_class = url.searchParams.get("environment_class");
    if (!CLASSES.includes(environment_class)) {
      return {
        status: 400,
        body: { error: "invalid_request", message: "environment_class must be production or non_production" },
      };
    }
    if (unavailable()) return { status: 503, body: { error: "access_decision_unavailable" } };
    const service = services.find(({ id }) => id === match[1]);
    if (!service || !canReadService(service, caller) || !["live", "deprecated"].includes(service.lifecycle)) {
      return { status: 404, body: { error: "not_found" } };
    }
    const withheld = (evidence_status) => ({
      status: 200,
      body: {
        service_id: service.id, environment_class, state: "unknown", evaluated_at: null,
        last_evidence_at: null, evidence_status, conditions: [],
      },
    });
    if (!canReadEvidence(service, caller)) return withheld("restricted");
    const assessment = service.assessments?.[environment_class];
    if (!assessment) return withheld("missing");
    return { status: 200, body: { service_id: service.id, environment_class, ...assessment } };
  };
}
