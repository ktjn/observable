# Observable Roadmap (2.0 line)

> **Status:** Active and authoritative on the `2.0` branch. The 0.x roadmap stays on `main`.
>
> Outcome-based sequence for turning Observable into the observability business specification
> (`spec/business/`, merged into [spec/system.yaml](spec/system.yaml)). Detail, decisions and risks:
> [docs/plans/business-spec-adoption.md](docs/plans/business-spec-adoption.md). Per-requirement
> status: [docs/plans/business-spec-gap.yaml](docs/plans/business-spec-gap.yaml).

## Direction

- Target: the 293 `OBS-*` requirements, 15 business risks and critical workflows UX-01..UX-10.
- First milestone (`2.0.0`): every active P0 requirement verified: 99 `OBS-*` (88 platform by test or
  analysis; 10 organization and 1 process by accountable inspection) plus 3 phase-gating authorization
  refinements (`req-decide-access-fail-closed`, `req-catalog-access-control`, `req-health-access-control`).
  P1 and P2 follow in `2.x`.
- Refactor the existing Rust/React platform. Breaking changes are allowed: no migrations, data
  preservation or compatibility with 0.x.
- Every change, including spec- and docs-only changes, passes `bash scripts/spec-gate.sh`
  (verification stage, baseline integrity, UX contracts, strict readiness evidence policy).
  Track progress with `uv run --project models python scripts/spec-readiness.py --priority P0`.
- `--stage release` cannot be scoped to P0: STAT-04, REQ-05 and RISK-04 fail for every active P1/P2
  requirement until it is verified. The `2.0.0` exit is therefore the project gate in the release row
  below; full `--stage release` is the exit for completing `2.x`.

## Preconditions and cross-cutting obligations

- **Phase 0 verification baseline:** Enforce criterion-level P0 evidence, complete UX interaction coverage and immutable imported business-requirement provenance before implementing new features. Run `bash scripts/spec-gate.sh`; neither structural verification nor a related existing test is passing OBS evidence.
- **Authorization prerequisite:** Phases 1–3 and 6–7 must define tenant, role, purpose, evidence visibility, existence-hiding and elevation behavior before exposing new catalog, health, investigation or transaction capabilities. Phase 1 and 2 carry this as P0 exit requirements (`req-decide-access-fail-closed`, `req-catalog-access-control`, `req-health-access-control`) and negative scenarios in UX-01 and UX-07; phases 3, 6 and 7 carry it through OBS-25-02/25-03, OBS-23-05 and OBS-31-01..03. Phase 8 completes platform-wide governance; it must not defer security for earlier functionality.
- **Component isolation prerequisite:** Every new provider (`observable-catalog`, `observable-health`, `observable-response`) publishes injectable capability contracts, programmable mocks for success/failure/delay and state transitions, and real/mock contract tests. Independent execution is separate from the integrated release gate.
- **Experience completeness:** UX-01–UX-10 are multi-step contracts; their declared interactions and negative paths are maintained in `spec/business/experience/ux-workflows.yaml`. Structural validation alone does not prove usability.
- **Business intent preservation:** The upstream baseline is vendored unchanged in `spec/business/baseline/v2.5/` with a SHA-256 manifest. Every deviation of the derived fragments is declared in `spec/business/refinements.yaml`; `scripts/check-business-baseline.py` rejects undeclared changes or removals.

Detailed acceptance and handoff rules: [phase-0 assurance plan](docs/plans/phase0-assurance-hardening.md). Domains: [ADR-042](spec/adr/ADR-042-operational-domain-boundaries.md).

## Technology decisions and measured alternatives

- **Default query path:** Rust semantic query facade → scoped ClickHouse queries. Apache Arrow optional; DataFusion deferred until federation/custom execution shows an independently verified benefit ([ADR-005](spec/adr/ADR-005-arrow-datafusion.md)).
- **Frontend:** React/TanStack/Base UI retained; Observable-owned visualization contracts and adapters replace the old mandatory Grafana library assumption. Grafana integration remains optional ([ADR-016](spec/adr/ADR-016-grafana-visualization-strategy.md)).
- **Authorization:** Zitadel/OIDC integration retained; purpose-bound, tenant-scoped and existence-hiding enforcement are required independent of whether OpenFGA is adopted. Prototype ReBAC requirements before committing to additional operations ([ADR-008](spec/adr/ADR-008-authorization-model.md)).
- **Storage experiment:** ClickHouse remains authoritative. Benchmark VictoriaMetrics for metrics first, VictoriaLogs for logs second; consider hybrid only after contract parity, security, correctness, storage/latency and recovery evidence ([ADR-045](spec/adr/ADR-045-victoria-storage-evaluation.md)).
- **Evaluation deployment:** Compose/single-node operation is a supported test profile. Kubernetes remains the production orchestration target, not a prerequisite for agent development or demos.

Do not declare technology replacements based only on vendor numbers or passing synthetic happy paths.

## Release model

- Each phase has an outcome, dependencies, the P0 requirements it must verify, and exit evidence.
- A phase is complete only when its P0 requirements are `verified` with `path::test_name` evidence,
  the listed workflows pass, and the listed risks have verified mitigations.
- Each phase starts with its own detailed plan (one run), then FEATURE.md slices, one component per PR,
  into `2.0`.

## Phases

| Phase | Outcome | Depends on | P0 to verify | Exit evidence |
|---|---|---|---|---|
| 0 | Business spec merged; UX contract and P0 evidence gates established; domain/provenance baseline fixed | — | — | `scripts/spec-gate.sh` passes: verification stage, baseline manifest and declared refinements, UX contracts UXC-01..09, strict criterion-level readiness policy |
| 1 | Service model: business services, owners, criticality, coverage obligations (`observable-catalog`, domain `service-model`); policy decision point; authorize catalog reads before exposing them | 0 | OBS-01-01..04, 02-01..04, 27-01, 27-02, 27-04, 34-01, 36-01; `req-decide-access-fail-closed`, `req-catalog-access-control` | catalog maps OTel services; coverage status visible |
| 2 | Evidence trust: five-state health, last evidence, data quality, self-degradation (`observable-health`, domain `service-assessment`); authorized health reads | 1 | OBS-03-01..04, 13-03, 24-01, 24-02, 26-01, 26-02, 26-04, 29-03; `req-health-access-control` | UX-07; `risk-false-healthy` mitigated |
| 3 | Experience shell: complete multi-step UX paths, scope bar, baseline states, navigation, overview, a11y and restricted-access tests | 2 | OBS-13-01, 13-02, 20-01..04, 20-08, 21-01, 21-03, 21-08, 22-01..03, 22-05, 22-11, 24-03..05, 24-09, 25-01..03 | UX-01, UX-02, UX-09, UX-10 |
| 4 | Reliability objectives and performance expectations | 1, 2 | OBS-04-01..03, 06-01..04 | SLIs/SLOs per business service with approval and history |
| 5 | Alerting and response routing | 1, 4 | OBS-07-01..04, 07-08, 33-01 | `risk-missed-escalation` mitigated |
| 6 | Incidents and investigation (`observable-response`) | 3, 5 | OBS-08-01..03, 09-01..03, 09-08, 11-01, 23-01..05, 23-11 | UX-04, UX-06; `risk-misattributed-cause`, `risk-inconsistent-incident-state` mitigated |
| 7 | Dependencies, impact, journeys, business processes, transactions, batch; purpose-bound authorized transaction lookup | 1, 6 | OBS-05-01, 05-02, 10-01, 10-02, 10-07, 12-01, 12-02, 31-01..03, 32-01, 32-02 | UX-03, UX-05, UX-08; impact, customer and cut-off risks mitigated |
| 8 | Governance: access, PII, retention, audit, interoperability, cost | 1 | OBS-14-01..04, 14-08, 15-01, 15-02, 16-01, 16-02, 17-01, 17-02 | disclosure and audit-evidence risks mitigated |
| 9 | Platform trust and resilience | 2, 5 | OBS-19-03, 29-01, 29-02 | monitoring-outage and overload risks mitigated |
| 2.0.0 | Release | 1-9 | all 102 P0 (99 OBS + 3 gating refinements) | `spec-gate.sh` passes; `spec-readiness.py --priority P0 --strict --fail-below 102`; every critical UX scenario whose requirements are all P0 has passing evidence; `--stage release` findings are limited to P1/P2 requirements and their risks and are listed in the release gate decision record |
| 2.x | P1/P2 expansion: providers, synthetics, status communication, on-call, AI services, assistance, remaining P1/P2 | per area | — | per-area detailed plan |

Detailed phase plans: [phase 1](docs/plans/phase1-service-catalog.md).

Phase 1 [S9 exit review](docs/plans/phase1-exit-review.md), 2026-11-08: S8 is merged,
but exit remains open at 6/15 verified. Six accountable inspections and three platform
requirements remain. The review records the health/objectives dependency conflict for
owner resolution; phase assignments and exit criteria remain unchanged.

Phase 8 can run in parallel with 2-7 once phase 1 lands. Its cross-cutting access and audit primitives required in earlier phases are prerequisites, not deferred phase-8 deliverables.

## Technology evaluation (non-blocking until an affected capability requires it)

- Define reproducible telemetry workloads, retention and cardinality distributions; record comparable ClickHouse and VictoriaMetrics ingestion, metric query and resource results.
- Separately exercise VictoriaLogs against real log UX-02/04 scenarios: faceting, search, tail, drill-down, and restricted-tenant security.
- Keep all experiments behind versioned storage capability adapters and real/mock contract tests; no backend swap without a separate ADR and rollback plan.
- Verify optional Grafana connectors only after the owned visualization model is functional.
- Benchmark single-node Compose and full Kubernetes topology separately; record the required operational burden.

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
