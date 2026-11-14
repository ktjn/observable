# Coverage & Control Exception Register

This register governs approved coverage, telemetry, and security control gaps (`OBS-14-08`).

## Approved Exception Record Schema

Every approved gap or exception MUST record:

- **Exception ID** (`exception_id`): unique identifier (e.g., `EXC-2026-001`)
- **Scope / Requirement** (`requirement`): the affected requirement, service, or control
- **Rationale** (`rationale`): technical or operational justification for the temporary gap
- **Review / Expiry Date** (`review_date`): ISO-8601 date (`YYYY-MM-DD`) by which the exception must be re-reviewed or remediated
- **Accountable Owner** (`accountable_owner`): named role or team accountable for risk acceptance and remediation
- **Current Status** (`status`): one of `approved`, `expired`, `revoked`, `closed`

## Expired Exception Flagging Policy

Any exception whose `review_date` is earlier than the current evaluation date MUST be flagged as `expired` and MUST NOT be treated as `approved`. Expired exceptions block release/compliance sign-off until either remediated (`closed`) or formally re-approved with a new rationale, owner sign-off, and future `review_date`.

## Register Entries

| Exception ID | Scope / Requirement | Rationale | Review / Expiry Date | Accountable Owner | Current Status |
| --- | --- | --- | --- | --- | --- |
| `EXC-2026-001` | Legacy batch-ingest TLS 1.2 fallback for partner collector | Partner collector upgrade scheduled; compensating network ACL in place | `2027-06-30` | Platform Security Lead | `approved` |
| `EXC-2025-004` | Staging synthetic log anonymization exemption | Replaced by `observable-process` scrubbing pipeline | `2026-01-15` | Telemetry Platform Owner | `expired` (flagged — NOT treated as approved) |
