# Observable Roadmap (2.0 line)

> **Status:** Active and authoritative on the `2.0` branch. The 0.x roadmap stays on `main`.
>
> Outcome-based sequence for turning Observable into the observability business specification
> (`spec/business/`, merged into [spec/system.yaml](spec/system.yaml)). Detail, decisions and risks:
> [docs/plans/business-spec-adoption.md](docs/plans/business-spec-adoption.md). Per-requirement
> status: [docs/plans/business-spec-gap.yaml](docs/plans/business-spec-gap.yaml).

## Direction

- Target: observability-spec **v2.6** (commit `7ce467c`): 299 `OBS-*` IDs (293 active, six superseded), 15 business risks and 10 critical workflows comprising 45 interactions and 91 scenarios. Preserve the established 99 P0 + three gating refinements for 2.0.0.
- First milestone (`2.0.0`): every active P0 requirement verified: 99 `OBS-*` (88 platform by test or
  analysis; 10 organization and 1 process by accountable inspection) plus 3 phase-gating authorization
  refinements (`req-decide-access-fail-closed`, `req-catalog-access-control`, `req-health-access-control`).
  P1 and P2 follow in `2.x`.
- Refactor the existing Rust/React platform. Breaking changes are allowed: no migrations, data
  preservation or compatibility with 0.x.
- Every change, including spec- and docs-only changes, passes `bash scripts/spec-gate.sh`
  (verification stage, baseline integrity, UX contracts, strict readiness evidence policy).
  Track progress with `uv run --project models python scripts/spec-readiness.py --priority P0`.
- System Specification **0.6.0** supports `system.release_scope: [P0]`. The 2.0.0 release requires a clean **P0-scoped** `--stage release`, including `EXP-07` and risks mitigated by P0; P1/P2 are reported as `deferred`, never passing. Expand the scope deliberately for 2.x.

## Preconditions and cross-cutting obligations

- **Phase 0 verification baseline:** Use `system-specification` 0.6.0 (`5eb061e`) with `REQ-10`, `EV-07/08`, `EXP-09..12`, `--evidence-report`, and P0 release scope. Keep the immutable v2.6 business snapshot in `spec/business/baseline/v2.6/`. Run `bash scripts/spec-gate.sh`; structural validity is not runtime UX proof.
- **Authorization prerequisite:** Phases 1–3 and 6–7 must define tenant, role, purpose, evidence visibility, existence-hiding and elevation behavior before exposing new catalog, health, investigation or transaction capabilities. Phase 1 and 2 carry this as P0 exit requirements (`req-decide-access-fail-closed`, `req-catalog-access-control`, `req-health-access-control`) and negative scenarios in UX-01 and UX-07; phases 3, 6 and 7 carry it through OBS-25-02/25-03, OBS-23-05 and OBS-31-01..03. Phase 8 completes platform-wide governance; it must not defer security for earlier functionality.
- **Component isolation prerequisite:** Every new provider (`observable-catalog`, `observable-health`, `observable-response`) publishes injectable capability contracts, programmable mocks for success/failure/delay and state transitions, and real/mock contract tests. Independent execution is separate from the integrated release gate.
- **Experience completeness:** UX-01–UX-10 are multi-step contracts; their declared interactions and negative paths are maintained in `spec/business/experience/ux-workflows.yaml`. Structural validation alone does not prove usability.
- **Business intent preservation:** The upstream v2.6 baseline is vendored byte-identically with a SHA-256 manifest; the historic v2.5 snapshot is retained. Derived requirements preserve accepted target, version and status, but adopt upstream holder and stable criterion IDs. Any future semantic deviation requires a recorded refinement, source impact and accountable decision.

Detailed acceptance and handoff rules: [phase-0 assurance plan](docs/plans/phase0-assurance-hardening.md). Domains: [ADR-042](spec/adr/ADR-042-operational-domain-boundaries.md).

## Specification alignment (2026-11-11)

- **Current authority:** `spec/system.yaml` plus included `spec/business/`, derived from [observability-spec v2.6](https://github.com/ktjn/observability-spec/tree/7ce467cb13c0c19a0b9f50934b08bff0c6e3278b). The independent upstream snapshot is retained for provenance and controlled change review.
- **Validation authority:** the vendored [system-specification 0.6.0](system-spec/CHANGELOG.md) reference validator and schema from commit `5eb061e`. No assumption that the installed 0.5.0 package implements new invariants.
- **Evidence:** enforce criterion IDs, holder-specific methods and explicit criterion coverage (`REQ-10`, `EV-07`, `EV-08`); use the validator's `--evidence-report` rather than interpreting unrelated passing tests as verified business obligations.
- **UX:** 45 required interactions, 91 scenarios, 10 critical exhaustive experiences; `EXP-09..12` now validates action coverage, reachable states and one-interaction negative failure scenarios. Browser and accessibility execution evidence remains a separate obligation.
- **Historical proof debt:** `req-ingest-delivery`, `req-component-compat`, `req-query-latency` and `req-ingest-throughput` are `specified` pending complete criterion coverage or a representative benchmark. Original records and provenance remain available; no new result is manufactured.
- **Readiness:** align `scripts/spec-readiness.py` with holder-specific `evidence_methods`; check P0 release scope only after all P0 obligations have evidence. See [alignment and verification plan](docs/plans/spec-alignment-v2.6.md).

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
| 0 | Business spec v2.6 imported; 0.6.0 validator, UX and evidence policy aligned | — | — | `scripts/spec-gate.sh`: verification, reference/artifact integrity, UX contracts and explicit evidence report; immutable v2.6 provenance verified |
| 1 | Service model: business services, owners, criticality, coverage obligations (`observable-catalog`, domain `service-model`); policy decision point; authorize catalog reads before exposing them | 0 | OBS-01-01..04, 02-01..04, 27-01, 27-02, 27-04, 34-01, 36-01; `req-decide-access-fail-closed`, `req-catalog-access-control` | catalog maps OTel services; coverage status visible |
| 2 | Evidence trust: five-state health, last evidence, data quality, self-degradation (`observable-health`, domain `service-assessment`); authorized health reads | 1 | OBS-03-01..04, 13-03, 24-01, 24-02, 26-01, 26-02, 26-04, 29-03; `req-health-access-control` | UX-07; `risk-false-healthy` mitigated |
| 3 | Experience shell: complete multi-step UX paths, scope bar, baseline states, navigation, overview, a11y and restricted-access tests | 2 | OBS-13-01, 13-02, 20-01..04, 20-08, 21-01, 21-03, 21-08, 22-01..03, 22-05, 22-11, 24-03..05, 24-09, 25-01..03 | UX-01, UX-02, UX-09, UX-10 |
| 4 | Reliability objectives and performance expectations | 1, 2 | OBS-04-01..03, 06-01..04 | SLIs/SLOs per business service with approval and history |
| 5 | Alerting and response routing | 1, 4 | OBS-07-01..04, 07-08, 33-01 | `risk-missed-escalation` mitigated |
| 6 | Incidents and investigation (`observable-response`) | 3, 5 | OBS-08-01..03, 09-01..03, 09-08, 11-01, 23-01..05, 23-11 | UX-04, UX-06; `risk-misattributed-cause`, `risk-inconsistent-incident-state` mitigated |
| 7 | Dependencies, impact, journeys, business processes, transactions, batch; purpose-bound authorized transaction lookup | 1, 6 | OBS-05-01, 05-02, 10-01, 10-02, 10-07, 12-01, 12-02, 31-01..03, 32-01, 32-02 | UX-03, UX-05, UX-08; impact, customer and cut-off risks mitigated |
| 8 | Governance: access, PII, retention, audit, interoperability, cost | 1 | OBS-14-01..04, 14-08, 15-01, 15-02, 16-01, 16-02, 17-01, 17-02 | disclosure and audit-evidence risks mitigated |
| 9 | Platform trust and resilience | 2, 5 | OBS-19-03, 29-01, 29-02 | monitoring-outage and overload risks mitigated |
| 2.0.0 | P0 release | 1-9 | all 102 P0 (99 OBS + 3 gating refinements) | `spec-gate.sh` passes; strict readiness reaches 102; P0-scoped `--stage release` passes with no in-scope uncovered criteria or critical UX gaps; P1/P2 explicitly `deferred` |
| 2.x | Expand validated release scope to P1/P2: providers, synthetics, communication, on-call, AI services and further UX requirements | per area | per-phase | Scoped release gate widened only after criteria and risk evidence; no silent promotion from deferred to verified |

Detailed phase plans: [phase 1](docs/plans/phase1-service-catalog.md), [phase 2](docs/plans/phase2-evidence-trust.md) (draft).

Phase 1 remains open at 9/15 verified; the remaining six are organization inspections. Close it before starting the remaining Phase 2
and Phase 4 scope. The health, reporting and objectives work required by OBS-01-03,
OBS-01-04 and OBS-02-04 is part of Phase 1 closeout. See the
[closure slices](docs/plans/phase1-service-catalog.md#11-phase-1-closure-slices) and
[S9 inspection workbook](docs/plans/phase1-exit-review.md).

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

## Independent verification and future modernization (parallel, not implicit release blockers)

- Select a bounded but meaningful capability (catalog authorization or five-state health) for an **independent replacement experiment** using the accepted contract and provider mock without prior implementation source. Record divergent behavior as spec gaps, not auto-changes to tests.
- Reuse current `--context`, `--check-isolation` and `--evidence-report`; evaluate future change-set and contract-adapter features from [VERIFIED-DEVELOPMENT.md](system-spec/VERIFIED-DEVELOPMENT.md) only after the pilot demonstrates a real need.
- Apply [MODERNIZATION.md](system-spec/MODERNIZATION.md) when comparing ClickHouse, VictoriaMetrics, VictoriaLogs and authorization technologies: requirement/risk traceability, measurable alternatives, parity, and new evidence determine replacement, not technological preference.
- Consider a native `SpecImport.mode: baseline` migration after stable 0.6 tooling and evidence are available; keep the current SHA-256-checked baseline and refinement guard until equivalence is proven.
- Treat cross-repo `--workspace` as optional only if specifications become independently owned and deployed; avoid creating duplicate authoritative copies.

## Non-goals for 2.0.0

- P1/P2 requirements (scheduled for 2.x).
- Data or configuration migration from 0.x.
- Replacing SIEM, financial transaction processing or automated remediation internals (spec boundaries).
