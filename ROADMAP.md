# Observable Roadmap (2.0 line)

> **Status:** Active and authoritative on the `2.0` branch. The 0.x roadmap stays on `main`.
>
> Outcome-based sequence for turning Observable into the observability business specification
> (`spec/business/`, merged into [spec/system.yaml](spec/system.yaml)). Detail, decisions and risks:
> [docs/plans/business-spec-adoption.md](docs/plans/business-spec-adoption.md). Per-requirement
> status: [docs/plans/business-spec-gap.yaml](docs/plans/business-spec-gap.yaml).

## Direction

- Target: the 293 `OBS-*` requirements, 15 business risks and critical workflows UX-01..UX-10.
- First milestone (`2.0.0`): every active P0 requirement verified (99; 88 platform, 11 organization/process).
  P1 and P2 follow in `2.x`.
- Refactor the existing Rust/React platform. Breaking changes are allowed: no migrations, data
  preservation or compatibility with 0.x.
- Validation runs at `--stage verification`; `--stage release` is the `2.0.0` exit check.
  Track progress with `uv run --project models python scripts/spec-readiness.py --priority P0`.

## Preconditions and cross-cutting obligations

- **Phase 0 verification baseline:** Enforce criterion-level P0 evidence, complete UX interaction coverage and immutable imported business-requirement provenance before implementing new features. Run `python scripts/check-ux-contracts.py` and `uv run --project models python scripts/spec-readiness.py --priority P0`; neither structural verification nor a related existing test is passing OBS evidence.
- **Authorization prerequisite:** Phases 1–3 and 6–7 must define tenant, role, purpose, evidence visibility, existence-hiding and elevation behavior before exposing new catalog, health, investigation or transaction capabilities. Phase 8 completes platform-wide governance; it must not defer security for earlier functionality.
- **Component isolation prerequisite:** Every new provider (`observable-catalog`, `observable-health`, `observable-response`) publishes injectable capability contracts, programmable mocks for success/failure/delay and state transitions, and real/mock contract tests. Independent execution is separate from the integrated release gate.
- **Experience completeness:** UX-01–UX-10 are multi-step contracts; their declared interactions and negative paths are maintained in `spec/business/experience/ux-workflows.yaml`. Structural validation alone does not prove usability.
- **Business intent preservation:** Imported OBS requirements and their upstream baseline are versioned; implementation-specific refinements must not silently weaken or replace the business obligations.

Detailed acceptance and handoff rules: [phase-0 assurance plan](docs/plans/phase0-assurance-hardening.md). Domains: [ADR-042](spec/adr/ADR-042-operational-domain-boundaries.md).

## Release model

- Each phase has an outcome, dependencies, the P0 requirements it must verify, and exit evidence.
- A phase is complete only when its P0 requirements are `verified` with `path::test_name` evidence,
  the listed workflows pass, and the listed risks have verified mitigations.
- Each phase starts with its own detailed plan (one run), then FEATURE.md slices, one component per PR,
  into `2.0`.

## Phases

| Phase | Outcome | Depends on | P0 to verify | Exit evidence |
|---|---|---|---|---|
| 0 | Business spec merged; UX contract and P0 evidence gates established; domain/provenance baseline fixed | — | — | Verification-stage metamodel valid, UX completeness checker passes, criterion-level readiness report runs; imported OBS baseline recorded |
| 1 | Service model: business services, owners, criticality, coverage obligations (`observable-catalog`); authorize catalog reads before exposing them | 0 | OBS-01-01..04, 02-01..04, 27-01, 27-02, 27-04, 34-01, 36-01 | catalog maps OTel services; coverage status visible |
| 2 | Evidence trust: five-state health, last evidence, data quality, self-degradation (`observable-health`) | 1 | OBS-03-01..04, 13-03, 24-01, 24-02, 26-01, 26-02, 26-04, 29-03 | UX-07; `risk-false-healthy` mitigated |
| 3 | Experience shell: complete multi-step UX paths, scope bar, baseline states, navigation, overview, a11y and restricted-access tests | 2 | OBS-13-01, 13-02, 20-01..04, 20-08, 21-01, 21-03, 21-08, 22-01..03, 22-05, 22-11, 24-03..05, 24-09, 25-01..03 | UX-01, UX-02, UX-09, UX-10 |
| 4 | Reliability objectives and performance expectations | 1, 2 | OBS-04-01..03, 06-01..04 | SLIs/SLOs per business service with approval and history |
| 5 | Alerting and response routing | 1, 4 | OBS-07-01..04, 07-08, 33-01 | `risk-missed-escalation` mitigated |
| 6 | Incidents and investigation (`observable-response`) | 3, 5 | OBS-08-01..03, 09-01..03, 09-08, 11-01, 23-01..05, 23-11 | UX-04, UX-06; `risk-misattributed-cause`, `risk-inconsistent-incident-state` mitigated |
| 7 | Dependencies, impact, journeys, business processes, transactions, batch; purpose-bound authorized transaction lookup | 1, 6 | OBS-05-01, 05-02, 10-01, 10-02, 10-07, 12-01, 12-02, 31-01..03, 32-01, 32-02 | UX-03, UX-05, UX-08; impact, customer and cut-off risks mitigated |
| 8 | Governance: access, PII, retention, audit, interoperability, cost | 1 | OBS-14-01..04, 14-08, 15-01, 15-02, 16-01, 16-02, 17-01, 17-02 | disclosure and audit-evidence risks mitigated |
| 9 | Platform trust and resilience | 2, 5 | OBS-19-03, 29-01, 29-02 | monitoring-outage and overload risks mitigated |
| 2.0.0 | Release | 1-9 | all 99 P0 | `--stage release` passes for P0 scope |
| 2.x | P1/P2 expansion: providers, synthetics, status communication, on-call, AI services, assistance, remaining P1/P2 | per area | — | per-area detailed plan |

Phase 8 can run in parallel with 2-7 once phase 1 lands. Its cross-cutting access and audit primitives required in earlier phases are prerequisites, not deferred phase-8 deliverables.

## Cross-cutting tracks carried over from 0.x

These 0.x outcomes stay valid and are verified alongside the phases, not as separate releases:

- **Component independence** (ADR-035, [docs/component-decomposition.md](docs/component-decomposition.md)):
  new components (`observable-catalog`, `observable-health`, `observable-response`) follow the same
  isolation, contract and build rules.
- **Self-hosted distribution:** Compose evaluation, doctor, backup/restore drills and pinned install keep
  passing for the 2.0 topology (`scripts/release-candidate-suites.sh`).
- **Kubernetes operations:** Helm chart, runbooks, capacity guidance and failure drills are updated per
  phase; phase 9 owns their verification against OBS-29.
- **Governed adoption:** ingest limits, retention and purge, credential lifecycle and usage reporting
  feed phase 8 (OBS-14, OBS-17).

## Non-goals for 2.0.0

- P1/P2 requirements (scheduled for 2.x).
- Data or configuration migration from 0.x.
- Replacing SIEM, financial transaction processing or automated remediation internals (spec boundaries).
