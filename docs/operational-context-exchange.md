# Operational context exchange and incident communications

Contract of `exchange-operational-context` (OBS-16-01, 16-02, 16-04, 16-10) and `communicate-incident-status` (OBS-30-01..04), provided by observable-response in `services/admin-service/src/response/`. Schema `response`, login `observable_response` (migration 085, `RESPONSE_DATABASE_URL`).

## Rules

- Every route sits behind `require_tenant`. The tenant and the acting principal come from the caller's credential. Tenant or user ids in a request body are ignored.
- Data leaves the platform only to an exchange target: a registered, active destination of the caller's tenant. Registering a target needs a tenant admin session; registration is the approval.
- Target URLs are checked at registration and again on every send. `https` only, no credentials in the URL, every resolved address must be public (no loopback, private, link-local, CGNAT, ULA or metadata addresses). The connection is pinned to the checked addresses and redirects are not followed. `RESPONSE_TARGETS_ALLOW_HTTP=true` and `RESPONSE_TARGETS_ALLOW_PRIVATE=true` relax this for local stacks only.
- Incident content is read from observable-alerting (`GET /v1/incidents/{id}`) with the caller's own credential. An incident the caller cannot read answers `404`, and nothing is sent.
- Access to evidence and records is decided by decide-access. An unavailable decision answers `503`; nothing is exported or sent.

## Routes

| Route | Requirement | Behaviour |
| --- | --- | --- |
| `GET /v1/response/targets` | | targets of the caller's tenant |
| `POST /v1/response/targets` | | tenant admin session; `kind` is `incident_handoff`, `notification`, `status_page` or `external_consumer` (needs `consumer_principal_id`) |
| `POST /v1/response/targets/{id}/deactivate` | | tenant admin; a deactivated target is never used |
| `POST /v1/response/handoffs` | OBS-16-01 | `{incident_id, target_id, max_retries, context}`; sends service, severity, status, owner, impact and context; every outcome is recorded in `handoff_outbox`; failure is `502 failed_pending_retry` with the preserved payload |
| `POST /v1/response/exports` | OBS-16-02 | session only; `{purpose, records}`; the package holds the records the user may read, and `withheld_count` |
| `POST /v1/response/notifications` | OBS-16-04 | `{incident_id?, target_ids, message}`; only active `notification` targets receive it; other ids are listed in `rejected_target_ids`; each delivery is recorded |
| `POST /v1/response/investigation-handoffs` | OBS-16-08, OBS-16-10 | `{incident_id, target_id, scope, purpose, evidence}` to an `external_consumer`; evidence is kept only when both the caller and the consumer's principal may read it. A security operations team registered as a consumer receives a suspected-attack incident this way (OBS-16-08) |
| `GET /v1/response/incidents/{id}/communications` | OBS-30-03 | review trail: drafted, approved and released by whom and when, delivery status |
| `POST /v1/response/incidents/{id}/communications` | OBS-30-01, 30-02 | session; draft to a `status_page` audience with `suspected_scope`, `confirmed_services` (each must be named by the incident), `summary`, `evidence_refs` |
| `POST .../{cid}/approve` | OBS-30-01, 30-04 | tenant admin who is not the drafter; refused (`422 non_public_evidence`) while any evidence reference is not `public` (missing classification counts as restricted) |
| `POST .../{cid}/release` | OBS-30-01, 30-04 | tenant admin; approved only; sends summary, suspected scope and confirmed services, never evidence; a failed delivery stays approved with `delivery_status = failed` |

## Not covered

- Retrying `failed_pending_retry` hand-offs in the background. The record is kept for an operator or a later slice.
- Owner-resolved classification. Export records and evidence references carry the classification the caller sends; decide-access decides on it. Resolving classification from each record's owner is W2c.
- Tenant purge deletes every `response` table (`response::PURGE_STEPS`) as part of `DELETE /v1/tenants/{id}`.
