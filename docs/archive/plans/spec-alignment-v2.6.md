# Specification alignment — observability v2.6 / system-specification 0.6.0

**Date:** 2026-11-11  
**Scope:** Specification and verification semantics only; no new runtime behavior asserted.

## Pinned upstream inputs

| Source | Revision | Local path |
|---|---|---|
| [observability-spec](https://github.com/ktjn/observability-spec/tree/7ce467cb13c0c19a0b9f50934b08bff0c6e3278b) | v2.6, `7ce467c` | `spec/business/baseline/v2.6/` (SHA-256-checked) |
| [system-specification](https://github.com/ktjn/system-specification/tree/5eb061e92096a02e90bf9c57f2d38412b4239752) | v0.6.0, `5eb061e` | `system-spec/` reference validator, schema and methodology |

The v2.5 snapshot is retained for comparison. The 293 active OBS obligations and six superseded identifiers remain stable. Derived requirements retain the previously accepted Observable `target`, `version` and `status`; the upstream `holder`, criterion ID/text and other non-runtime fields are authoritative. `spec/business/refinements.yaml` records permitted differences; `scripts/check-business-baseline.py` verifies the new manifest and rejects undeclared deviations.

## New model requirements

- `REQ-10`: criteria are individually identified and stable (`acceptance`, `negative` as applicable).
- `EV-07`: evidence methods permitted by obligation holder: platform `test, analysis, inspection`; process `test, inspection`; organization `inspection`.
- `EV-08`: every active evidence record must identify a criterion; blanket evidence is not accepted. `--evidence-report` is the canonical coverage report.
- `EXP-09..12`: exhaustive experience actions have interactions, view states and failure modes have appropriately scoped scenarios. Model validation is not a browser execution result.
- `system.release_scope: [P0]`: the 2.0.0 release gate is scoped to P0; deferred P1/P2 requirements and risks must remain reported as deferred. Release is not authorized until the scope's evidence and UX verification pass.

## Preserved historical evidence and debt

The imported OBS requirements' existing lifecycle status and evidence links are preserved. Five old technical evidence records lacked individual criterion bindings. They remain in the specification with exact criteria rather than being promoted into new OBS evidence.

| Legacy requirement | Decision | Required follow-up |
|---|---|---|
| `req-bounded-worker-concurrency` | Existing bounded TLC check names its one criterion; retains previous status | Rerun model check in a reproducible job |
| `req-ingest-delivery` | Reclassified as `specified`; TLC checks backpressure but not every accepted item's terminal outcome | Add independent terminal-delivery evidence |
| `req-component-compat` | Reclassified as `specified`; TLC covers SemVer compatibility but not the source-import boundary criterion | Run the component boundary gate and bind both criteria |
| `req-query-latency` | Reclassified as `specified`; historical smoke does not establish p99 ≤ 500 ms | Benchmark at the defined percentile/load/scope |
| `req-ingest-throughput` | Reclassified as `specified`; historical smoke does not establish 10,000 spans/sec per replica | Reproduce controlled sustained-throughput measurements |

The old smoke evidence remains visible, marked as skipped for the missing acceptance demonstration; it is **not passing verification**. No tests are claimed to have been rerun during this specification-only update.

## Roadmap consequences

1. **Phase 0 specification hardening:** v2.6 import and 0.6.0 model gate, manifest validation, explicit evidence-report and UX completeness.
2. **Phase 1 closeout:** six remaining organizational inspections of the service catalog. Do not claim completion until accountable owners accept current evidence.
3. **Phase 2 health/evidence trust:** five-state health and unknown-on-missing-evidence; preserve authorization and service ownership. Verify UX-07.
4. **Phase 3 experience shell:** use all 45 declared interactions and 91 scenarios as the target contract; independently execute UX-01/02/09/10 with browser/a11y checks.
5. **Phases 4–9:** retain existing dependencies and technology choices; prioritize P0 and owner-specific evidence for release.
6. **2.0.0 release:** strict 102 P0 requirement readiness plus P0-scoped `--stage release`, risk mitigation and applicable critical UX evidence. P1/P2 explicitly deferred.
7. **2.x:** only widen release scope after criteria-level evidence is ready and owner-approved.

## Optional modernization and independent verification

- Select `observable-catalog` or `observable-health` for a bounded implementation replacement experiment. Use a provider-owned programmable mock, fresh independent tests, and no predecessor source; compare outcomes with accepted contract requirements. Follow [VERIFIED-DEVELOPMENT.md](../../system-spec/VERIFIED-DEVELOPMENT.md) as a proposal, not an additional mandatory schema.
- Apply [MODERNIZATION.md](../../system-spec/MODERNIZATION.md) to evaluate VictoriaMetrics and VictoriaLogs versus ClickHouse; preserve functional and security obligations across technology changes.
- Consider native `SpecImport.mode: baseline` and distributed `--workspace` only after they provide verified advantages over the existing pinned manifest and refinement checks.

## Reproducible checks

```bash
bash scripts/spec-gate.sh
uv run --project models python system-spec/reference-implementation/metamodel.py spec/system.yaml --evidence-report --json
uv run --project models python system-spec/reference-implementation/metamodel.py spec/system.yaml --stage release
```

The release command is expected to fail until every P0 obligation is met; it is the release decision gate, **not** a routine development precondition.
