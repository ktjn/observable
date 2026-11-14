# Compliance, Regulatory Reporting, and Supplier-Exit Governance

Covers `OBS-15-04` (process), `OBS-15-07` (process), and `OBS-16-05` (organization).

## 1. Evidence Compilation Procedure (`OBS-15-04`)

For any representative audit or compliance event, the compliance reviewer compiles an evidence bundle from immutable platform logs containing:
- Triggering signal/control record, timestamps, and scoped service identifiers.
- Access-control and authorization decision records (`auth-service`).
- Reviewer sign-off and retention digest reference.

## 2. Major Incident Regulatory Classification & Reporting (`OBS-15-07`)

Major incidents subject to operational resilience / financial regulation record:
- **Regulatory classification:** incident category and materiality threshold decision.
- **Reporting deadlines:** initial notification deadline, intermediate update deadline, and final post-incident report deadline.
- **Submitted reports:** immutable timestamps and submission references for each filed report.

## 3. Supplier-Exit Interpretability & Accessibility Review (`OBS-16-05`)

Quarterly and pre-contract supplier-exit reviews verify that exported telemetry, SLO/objective definitions (`openslo/v1`), and incident/audit archives remain readable, self-describing, and accessible in standard formats without vendor-proprietary tooling.
