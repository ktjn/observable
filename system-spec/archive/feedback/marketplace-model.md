# Marketplace Model Feedback

Date: 2026-11-01.
Source: Scalable `spec-model/marketplace.yaml` and README Marketplace diagram (`dae05296` plus the wiring fix), `--diagram`, `--check-drift`.
Metamodel: `af2bec5`.

## Findings

| ID | Observed fact | Gap | Proposed change |
|---|---|---|---|
| MKT-FB-01 | The app depends on the Scalable framework (`scalable-module-abi`, `-core`, `-node`, `-runtime`, `-hosting`, `-ops`); it was recorded as `external_sources: [scalable-framework]`, which is for data sources and is not rendered. The fix models a `platform`-layer stub component with 5 capabilities copied from `scalable.yaml`. | No external provider (platform, third-party API) concept and no cross-spec capability reference; the stub duplicates contracts, takes layer/budget/test obligations, and drift scans the platform's own manifests (`scalable-runtime` dev-deps the example). | Optional `Component.external: true`: provides only; exempt from layers, domains, budgets, `test_paths`/`fakes` and drift scanning of its own manifests, still a drift target via `source_paths`. `--diagram` renders it outside the system boundary as a platform node listing its capabilities (API). Optionally reference capabilities from another spec instead of copying. |
| MKT-FB-02 | `ordering` writes `catalog-inventory` lots and `ledger` credits via cross-module `TransactionPlan`; `catalog-inventory` reads `accounts` sellers. Modeled as plain `requires`. | Runtime-mediated dependencies look like compile-time ones; drift cannot confirm them (no Cargo edge) and does not report them as unchecked. | Optional `Component.via` (required capability → mediating platform capability, e.g. `ledger-module: distributed-transactions`); drift lists these edges as unchecked; `--diagram` dashes them. |
| MKT-FB-03 | Without `source_paths`, `--check-drift` printed VALID while checking nothing. With them, `store`/`topology` integration-test dev-dependencies on real components fail "use a fake". | Silent no-op drift; the tests-use-fakes rule has no notion of integration/assembly tests. | List components without `source_paths` as unchecked. Allow integration tests (e.g. `Component.test_scope: integration`) to dev-depend on components they already `require`. |

## Dispositions

- Implemented: MKT-FB-01 (`Component.external`, `COMP-11`, hexagon outside the system), MKT-FB-02 (`Component.via`, `REF-05`, dashed edges, drift `unchecked`), MKT-FB-03 (drift lists components without `source_paths`; `Component.test_scope: integration`). Cross-spec capability references: `system.imports` + `<import>:<capability>` + `IMP-01` + `--check-imports` (pins only; imported interfaces and data models stay in the other spec).
- Product-side: `marketplace.yaml` gains `source_paths`, the `aggregate-state` component, the platform stub, cross-module and hosting `requires`; 4 dev-dependency drift findings recorded in Scalable `spec-model/INTAKE.md`.
