# Plan: Adopt the Observability Business Specification

**Status:** Draft 1 (planning the plan). Gap analysis is a surface scan; per-requirement verification is phase 0 work.
**Target spec:** `observability-spec` v2.5 (2026-11-06, commit `523e682`), local path `C:\git\observability-spec`
**Current system:** `spec/system.yaml` (`observable-platform` 0.1.0, 8 components, 17 requirements, 9 risks, 68 evidence records)
**Method:** [system-spec/FEATURE.md](../../system-spec/FEATURE.md) per slice, upstream intent from [GREENFIELD.md](../../system-spec/GREENFIELD.md); change flow in [AGENTS.md](../../AGENTS.md)

## 1. Scope and goal

- Goal: make Observable satisfy the business specification, verified requirement by requirement.
- This document is a plan for the detailed plan. It fixes structure, ordering, dependencies, risks, breaking changes and open questions. Each phase gets its own detailed plan in a later run.
- Out of scope for this document: implementation, edits to `spec/system.yaml`, fixes to the three known `main` failures (migration CRLF checksums 001-004, duplicate provenance key on `ev-dedup-token-deterministic`, `update-spec-evidence.py --check` `path::test` parsing).

## 2. The two specifications

| Aspect | Business spec (target) | `spec/system.yaml` (current) |
|---|---|---|
| Kind | Greenfield, vendor-neutral business requirements | Brownfield, recovered technical behavior |
| Metamodel | system-spec, `system-spec-ref 0.5.0`, stage `verification` | system-spec, in-repo `system-spec-ref 0.5.0`, stage `release` |
| Components | 37 logical areas (`health`, `alerting`, ...), no `requires` | 8 runtime components with contracts and data models |
| Requirements | 293 active (99 P0, 161 P1, 33 P2); 259 platform, 19 process, 15 organization | 17, all `verified` |
| Experience | 11 views, 10 critical exhaustive workflows UX-01..UX-10, baseline `state_sets` | none |
| Risks | 15, all `mitigation` | 9 (4 accepted) |
| Evidence | none (no requirement verified) | 68 records, `path::test_name` |
| Numbers | none; thresholds are business-owned | hard-coded (for example health thresholds 1% / 5%) |

`spec/observability_business_spec/` in this repo is a stale Markdown copy (2026-10-18, 19 areas). It is superseded by v2.5 and must be replaced or removed (Q1).

### Key semantic mismatches

1. **Service.** Spec: a business service with owners, criticality, lifecycle, outcome and coverage obligation. Observable: an OTel `service.name` string discovered from telemetry. No catalog entity exists.
2. **Health.** Spec: normal, degraded, unavailable, planned maintenance, unknown; unknown never shown as healthy. Observable: `healthy | watch | breach` from error rate only; no spans yields error rate 0 and therefore `healthy` (`services/query-api/src/discovery.rs:961`, `health_state`). This is a direct P0 violation (OBS-03-01 negative, `risk-false-healthy`).
3. **Incident.** Spec: owner, roles, impact, timeline, fact/hypothesis/verified cause. Observable: alert-derived record, statuses `triggered..post_mortem`, severities `critical|warning|info`, comments, deployment link.
4. **Roles.** Spec: 17 accountable organizational roles plus purpose-based access. Observable: `tenant_admin | member | viewer` (users), `viewer | member | admin` (API keys).
5. **Tenant.** Spec: organizational context, legal entity, business unit, jurisdiction segregation. Observable: tenant plus environment.

## 3. Gap analysis by area

Legend: **P** partial capability exists; **M** missing; **O** organization/process obligation (platform only records or supports it). Evidence is a surface scan of routes, migrations and UI; phase 0 confirms per requirement.

| # | Area | P0 | Status | What exists | Main gap |
|---|---|---|---|---|---|
| 01 | Business outcomes and scope | 4 | M/O | environments | business service, outcome, criticality, context classes |
| 02 | Service catalog and coverage | 4 | M | discovered service list, search | catalog entity, owners, support route, coverage status, unregistered detection |
| 03 | Health and availability | 4 | P | error-rate health pill | five-state model, unknown on missing evidence, last-evidence time, interruption history |
| 04 | Performance and capacity | 3 | P | latency, throughput, response-time history | expectations per interaction, degraded vs unavailable, limit warnings |
| 05 | Digital customer experience | 2 | M | ADR-018 accepted, not implemented | journeys, RUM, customer-impact view |
| 06 | Service level and reliability | 4 | P | availability SLO, burn-rate alerts, reliability report | SLI types, approval, commitment types (SLO/internal/SLA), definition history, journey SLOs |
| 07 | Detection and alerting | 5 | P | threshold/composite/burn-rate rules, webhook channel, silence, suppression, runbook URL | accountable owner, escalation path, delivery/ack tracking, alternate route, impact-based severity |
| 08 | Incident and problem response | 3 | P | incidents, ack, status, comments, deployment link | owner, roles, impact, problem records, milestone record completeness |
| 09 | Investigation and diagnosis | 4 | P | traces, logs, metrics, trace compare, workbench, NLQ | investigation context object, period comparison, hypothesis vs verified cause |
| 10 | Dependencies and impact | 3 | P | trace-derived topology | business dependency, observed vs estimated impact, source/confidence |
| 11 | Change and release assurance | 1 | P | deployment markers, change events, incident link | before/after comparison, change types (data-structure, third-party), freeze periods |
| 12 | Business process observability | 2 | M | none | business-defined process measures, rejection vs failure |
| 13 | Reporting and dashboards | 3 | P | dashboards, grants, import/export | consolidated accountable view, period/scope/freshness disclosure |
| 14 | Security, privacy, governance | 5 | P | OIDC, tenant isolation, audit logs, retention, purge | PII masking (`risk-pii-not-scrubbed`), classification/purpose, break-glass, segregation |
| 15 | Compliance and audit | 2 | P/O | credential and query audit logs | tamper evidence, incident reconstruction, regulatory classification and deadlines |
| 16 | Interoperability | 2 | P | REST API, MCP, dashboard import/export, OTLP | incident-management exchange, documented export of all data, SecOps exchange |
| 17 | Cost transparency | 2 | P | usage report, quotas | cost attribution per service, overrun warnings |
| 18 | Assistance and automation | 0 | P | NLQ, MCP tools | anomaly detection, uncertainty disclosure, action authority, attribution |
| 19 | Trust and service operation | 1 | P/O | self-observability, runbooks, perf smoke | platform owner, business-approved response times, accessibility |
| 20 | Information architecture | 5 | P | sidebar areas, global tenant/env context, search | scope bar in every view, context retention, entry points per spec |
| 21 | Role-based workspaces | 3 | P | home, service detail, incidents | org-wide overview of critical services, responder view, summary-to-evidence |
| 22 | Exploration and visualization | 5 | P | histograms, filters, workbench, saved views (logs only) | interval-to-evidence, units/aggregation disclosure, comparison, any-attribute breakdown |
| 23 | Incident triage experience | 6 | P | incident detail page | complete summary, fact/inferred/hypothesis separation, assign/escalate actions |
| 24 | Interaction states and a11y | 6 | P | loading/empty states ad hoc | baseline state set everywhere, stale/unknown/unauthorized, WCAG 2.2 AA evidence |
| 25 | Responsive access and governance | 3 | P | tenant context switch | mobile on-call, elevated-access indicator, existence-hiding authorization |
| 26 | Data quality and trust | 3 | M | none | completeness/freshness, absence of failures vs absence of data, sampling disclosure |
| 27 | Observability lifecycle | 3 | M/O | none | required vs actual coverage, gap owner and acceptance |
| 28 | Proactive assurance | 0 | M | none | synthetics, expiry warnings |
| 29 | Operational resilience | 3 | P | runbooks, drills, platform alerting | self-degradation shown in UI, failure-domain separation of notification |
| 30 | External communication | 0 | M | none | status communication with approval |
| 31 | Transaction tracing | 3 | P | trace lookup by trace ID | lookup by business reference, purpose-bound and audited lookup |
| 32 | Batch and async | 2 | M | none | registered jobs, deadlines, late/missed detection |
| 33 | Runbooks | 1 | P | `runbook_url` on alert rules | owned, reviewed guidance; known errors |
| 34 | Asset governance | 1 | P/O | dashboard owner, grants | owner on every alert/SLO/check, versioning, approval |
| 35 | AI-enabled services | 0 | M | none (GenAI semconv not modeled) | AI interaction observability |
| 36 | Third-party providers | 1 | M/O | none | provider registry, independent provider health |
| 37 | Responder load and on-call | 0 | M | none | schedules, coverage gaps, load review |

Summary: no area is complete. Areas 18, 28, 30, 35 and 37 have no P0; of these only 18 has any capability. Areas 05, 12, 26, 27, 32 have P0 requirements and nothing built. The P0 core concentrates in areas 01-03, 06-09, 13-14, 20-27, 29, 31.

## 4. Architecture decisions the plan needs

1. **Spec integration model (Q1, decided 2026-11-07: merge).** The OBS requirements move into `spec/system.yaml`; the upstream repo stays the authoring source until the merge, then `spec/system.yaml` is authoritative. Consequences and sub-decisions:
   - **Release gate.** STAT-04 fails at `--stage release` for any active requirement that is not `verified` (`draft` and `specified` both fail), and REQ-05 fails for unverified mandatory P0s. Merging 293 unverified requirements turns the AGENTS.md gate red until they are verified (Q7).
   - **Targets.** OBS requirements target the spec's 37 logical areas (`health.platform`, ...). Observable components are runtime units. Recommended: retarget each OBS requirement to an Observable capability (new capabilities per phase), keep the `OBS-NN-NN` ID, and record area and holder in the requirement description or tags. Alternative: import the 37 areas as logical components alongside the runtime ones (Q8).
   - **Layers and domains.** Spec layers `experience/capability/governance` and its 10 domains differ from Observable's `experience/application/domain/infrastructure`. Retargeting (recommended above) avoids importing them.
   - **Experience.** `views`, `experiences` and `state_sets.baseline` merge into `spec/system.yaml` (root-level `state_sets`, per OBSSPEC-EXP-09).
   - **Ownership.** OBS `owner` roles (17) and the `roles` catalog need a home; `spec/catalog.yaml` is a rendering sidecar upstream.
   - **Stale copy.** `spec/observability_business_spec/` is removed in the merge PR.
   - **Upstream.** After the merge, upstream changes are applied through FEATURE.md; the upstream repo is archived or kept read-only (Q9).
2. **Service catalog as a new owned state.** New data model (`business-service`, `service-component` mapping to OTel `service.name`, owners, criticality, lifecycle, coverage obligation). Owner component: `observable-control` or a new `observable-catalog` (Q5).
3. **Health evaluation as a domain capability.** Move health from a query-time error-rate helper into an evaluated, rule-based state with evidence age, maintenance windows and unknown. Feeds overview, alerting, SLO and data-quality.
4. **Incident management: build or integrate (Q4).** Spec scope says incident capabilities "may be satisfied by integrated incident-management tooling". Same question for on-call schedules (37) and status communication (30).
5. **Authorization model.** Purpose-based and existence-hiding access (14, 25-02, 31-03) needs fine-grained checks. OpenFGA runs in Compose but no service code calls it; ADR-008 must be revisited.
6. **Experience layer.** Adopt the spec's views, workflows and baseline state set as the UI contract; add `views`/`experiences` to `spec/system.yaml` or keep them in the upstream spec with Playwright evidence binding.

## 5. Phases

Ordered by dependency. Each phase becomes one detailed-plan run and several FEATURE.md slices (one component per PR).

| Phase | Name | Areas | Depends on | Exit (spec terms) |
|---|---|---|---|---|
| 0 | Adoption baseline | all | — | Q2-Q9 decided; OBS requirements, views and experiences merged into `spec/system.yaml`; per-requirement gap status for all 293; stale copy removed |
| 1 | Service model | 01, 02, 27, 34-01, 36-01 | 0 | P0 of 01, 02, 27 verified; catalog maps OTel services |
| 2 | Evidence trust and health | 03, 26, 29-03, 13-03, 24-01/02 | 1 | `risk-false-healthy` mitigated; UX-07 passes |
| 3 | Experience shell | 20, 21, 22, 24, 25 | 2 | scope bar, state set, navigation; UX-01, UX-02, UX-09, UX-10 pass |
| 4 | Reliability objectives | 06, 04 | 1, 2 | P0 of 06, 04 verified |
| 5 | Alerting and response | 07, 33-01 | 1, 4 | `risk-missed-escalation` mitigated |
| 6 | Incidents and investigation | 08, 09, 11-01, 23 | 3, 5 | UX-04, UX-06 pass; `risk-misattributed-cause`, `risk-inconsistent-incident-state` mitigated |
| 7 | Dependencies, impact and business processes | 10, 05, 12, 31, 32 | 1, 6 | UX-03, UX-05, UX-08 pass; `risk-missed-cut-off` mitigated |
| 8 | Governance and compliance | 14, 15, 16, 17 | 1 | `risk-unauthorized-disclosure`, `risk-corrupted-audit-evidence` mitigated; PII scrubbing done |
| 9 | Resilience | 19, 29 | 2, 5 | `risk-monitoring-outage`, `risk-platform-overload-during-incident` mitigated |
| 10 | P1/P2 expansion | 18, 28, 30, 35, 36, 37, remaining P1/P2 | per area | per-area detailed plan |

Phase 8 can run in parallel with 2-7 after phase 1. Phase 3 UI shell work can start after phase 2 defines the state model.

### Phase 0 detailed-plan deliverables (next run)

1. Decide Q2-Q9.
2. Classify all 293 requirements (met/partial/missing/org) with evidence (file, route or test).
3. Map each OBS requirement to an owning component and capability, creating capabilities where none fits.
4. Merge PR: OBS requirements, risks, views, experiences and roles into `spec/system.yaml`; `--compat` against `main`; gate adjusted per Q7.
5. Reconcile with [ROADMAP.md](../../ROADMAP.md): place each phase relative to 0.2-1.0 (Q2).
6. Write ADR drafts for decisions 1-5 in section 4.

## 6. Breaking changes

**Policy (decided 2026-11-07):** this is a new start; breaking anything is allowed. Consequences:

- No data migrations, dual-emit, backfills or deprecation periods. Schemas, APIs, `.mdl` models and the UI are redesigned to the spec.
- `--compat` against `main` is not a gate for this work; capability versions restart (for example 2.0.0 or 0.x, Q10).
- Postgres and ClickHouse migrations may be squashed into a new baseline; existing tenants and data are not preserved.
- Existing `spec/system.yaml` requirements, risks and evidence are kept only where they still describe the target; the rest are deprecated or deleted.

Known areas that change shape (redesign, not migration): health states, service identity (catalog entity instead of `service_name`), incident model, SLO model, roles and permissions, notification channels, saved views.

## 7. Risks

- **Critical: scope.** 293 requirements against a 17-requirement system. Without a P0-first cut the plan does not converge. Mitigation: phases 1-9 target P0 only; P1/P2 deferred to phase 10.
- **Critical: false-healthy today.** The current health derivation violates OBS-03-01 now. Consider fixing it ahead of the plan (Q6).
- **Critical: release gate red after merge.** 293 unverified requirements fail STAT-04/REQ-05 at `--stage release`. Needs an explicit gate policy (Q7) before the merge PR.
- **Business-owned thresholds.** The spec defines no numbers. Every acceptance test needs configured thresholds; the platform must make thresholds configurable, not hard-coded.
- **Roadmap conflict.** ROADMAP.md 0.2 (component independence) is in progress. A new start may replace ROADMAP.md (Q2). Whatever survives must follow ADR-035 isolation rules.
- **Organization/process obligations** (34 requirements) cannot be verified by tests. The platform records them; verification is inspection by the business owner.
- **Upstream drift.** The business spec is versioned separately (v2.5 now). Pin a version; re-run the gap map on each upstream release.
- **Toolkit gap.** OBSSPEC-EXP-09 (`state_sets` in include fragments) is open upstream.

## 8. Open questions

- **Q1.** Decided 2026-11-07: merge into `spec/system.yaml`.
- **Q7.** Gate after merge: (a) validate at `--stage verification` until the target release, plus a readiness report of verified P0s; (b) merge requirements phase by phase so only the current phase is red; (c) keep `--stage release` red and accept it. Recommended: (a).
- **Q8.** Targets: retarget OBS requirements to Observable capabilities (recommended), or import the 37 areas as logical components?
- **Q10.** Reuse or rebuild: refactor the existing services into the target (keep Rust/React stack, ingest, storage), or rebuild from the spec per REPLACEMENT.md and reuse code only where it fits?
- **Q9.** Upstream repo after merge: archive, keep read-only, or keep authoring there and sync?
- **Q2.** Release placement: does this replace the ROADMAP 0.3-1.0 sequence, extend it after 1.0, or interleave (for example phase 1-2 inside 0.6)?
- **Q3.** Target cut: P0 platform requirements (88) as the first milestone, with P1/P2 later?
- **Q4.** Incidents, on-call schedules and status communication: build in Observable or integrate external tooling?
- **Q5.** Catalog ownership: extend `observable-control` or add a new component?
- **Q6.** Fix the false-healthy health derivation now as a standalone PR, ahead of the plan?
