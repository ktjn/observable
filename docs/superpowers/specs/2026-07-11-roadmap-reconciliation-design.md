# Roadmap Reconciliation Design

**Issue:** [#528](https://github.com/ktjn/observable/issues/528)

## Purpose

Reconcile the active roadmap with the repository as it exists on 2026-07-11. The result must stop
agents from selecting stale, already-shipped, oversized, or prerequisite-blocked work and must put
known authentication-bypass remediation ahead of discretionary feature development.

This iteration changes planning and governance documentation only. It does not implement any
product feature or security fix.

## Evidence Model

Roadmap status is derived in this order:

1. Current implementation and integration tests on `main`.
2. Merged pull requests and archived implementation plans.
3. Current specs and ADRs.
4. Existing roadmap prose.

When roadmap prose conflicts with shipped code, the roadmap is corrected. A partially shipped
parent item is split into explicit completed and remaining slices rather than left checked with an
open qualification in its description.

## Priority Model

The roadmap will replace its mechanical "first unchecked item in the highest tier" rule with a
two-stage selection rule:

1. Address ready P0 security or correctness risks before discretionary user-value work.
2. Otherwise select the highest-value ready slice that is independently reviewable and has its
   prerequisites and architecture decisions resolved.

Security precedence applies to the known public fallback session-signing secret. It does not
promote the entire stability or compliance backlog.

## Status Corrections

- Mark SLO burn-rate alerting complete, citing its evaluator, API/UI integration, and
  Testcontainers coverage. Retain only genuinely unfinished presentation improvements under
  Service Health.
- Split Saved Views into the completed logs slice and open traces and metrics slices.
- Replace "Admin Console RBAC and Quota Management Views" with a completed RBAC mutation slice and
  an open quota-management slice.
- Rewrite OIDC/session and tenant-scoping test gaps to acknowledge current coverage and name only
  the missing callback, provider-failure, cookie-issuance, and admin middleware paths.
- Rewrite self-metrics work to acknowledge existing OTLP self-observability initialization while
  retaining missing `/metrics` endpoints and service-specific operational instruments.

## Backlog Decomposition

- Export APIs becomes separate synchronous per-signal slices. CSV and JSON are product backlog;
  OTLP remains a spec requirement that must be explicitly resolved before the first export slice.
  Asynchronous exports require a separate architecture-backed job, retention, authorization, and
  download-storage design.
- Fleet Management becomes a sequenced program: inventory contract and data source, inventory UI,
  then remote-configuration protocol and UI. It is not "Ready Now" while its backend contract is
  absent.
- Quota management becomes separate backend mutation/audit and frontend control slices.

## Near-Term Sequence

The roadmap's recommended sequence becomes:

1. Fail closed on missing session-signing secret, with an explicit local-development exception and
   Helm installation strategy.
2. Add focused OIDC login/callback regression coverage.
3. Add focused admin-service authentication and tenant-scoping middleware coverage.
4. Complete Saved Views for traces.
5. Complete Saved Views for metrics.

After these slices, reassess synchronous exports against Error Tracking and Service Topology using
the same readiness and user-value criteria.

Each promoted near-term slice must have a GitHub issue before implementation begins. This
reconciliation iteration creates issues for the five named slices; later work is issue-backed when
it is promoted, avoiding speculative issue sprawl.

## Plan and Governance Cleanup

- Move the four shipped plans for OTel Demo integration, Prometheus Remote Write, Admin UI Cleanup,
  and Alert Inhibition Rules from `docs/superpowers/plans/` to root `archived/plans/`.
- Move completed plans from the nested `docs/superpowers/plans/archived/plans/` location to root
  `archived/plans/`, preserving filenames and fixing references.
- Update the UI-remediation roadmap link to its actual archived location.
- Update `AGENTS.md` to point to the unified roadmap instead of the superseded May plan.
- Keep `docs/agent-context.md` synchronized with the corrected selection rule and next slice.

## Verification

Because the iteration is documentation-only, `bash scripts/local-ci.sh` is exempt. Verification is:

- `git diff --check`
- placeholder and stale-path searches
- Markdown link/path validation for changed internal references
- four-phase documentation review
- planning-steward and spec-steward review

No ADR change is required: this iteration corrects backlog truth and sequencing without changing
architecture, technology, deployment, data, or security models. The future async-export and fleet
remote-configuration designs remain explicitly architecture-gated.

## Rollback

Revert the reconciliation commit. No runtime, schema, dependency, or generated artifact changes are
made.
