# Architecture Diagram Feedback

Date: 2026-11-01.
Source: Observable README [architecture diagram](https://github.com/ktjn/observable#architecture) (`metamodel.py --diagram`), Scalable README diagrams.
Metamodel: `ba973ff`.

## Findings

| ID | Observed fact | Gap | Proposed change |
|---|---|---|---|
| DIAG-FB-01 | Every `DataModel` renders as a cylinder `[( )]`. Readers take them for databases; they are logical owned record types. | Logical-only and persisted models look identical, although `namespace`/`storage_role` (OBS-FB-01) already distinguish them. | Solid cylinder when `namespace` or `storage_role` is set, dashed outline otherwise; show `namespace` in the label. |
| DIAG-FB-02 | Data models are emitted after the layer subgraphs, so they float outside every layer. | Ownership is only an `owns` edge; the model sits away from its owner's layer. | Place each model inside its owner's layer subgraph. |
| DIAG-FB-03 | Layer subgraph IDs reuse layer names; a component with the same ID as its layer (Scalable `access`, `execution`, `hosting`) produces invalid Mermaid. | Subgraph and node IDs share one namespace. | Prefix subgraph IDs (e.g. `layer_<name>`); test a spec whose component ID equals its layer. |

## Dispositions

- Open: DIAG-FB-01 to DIAG-FB-03 (`diagram()` in `metamodel.py` and a `test_metamodel.py` case; no schema change).
- Product-side: Observable `spec/system.yaml` data models now declare `namespace` (`auth`, `control`, `alerting`, ClickHouse `observable`) and Postgres `storage_role`; Scalable README diagrams were hand-patched for DIAG-FB-03.
