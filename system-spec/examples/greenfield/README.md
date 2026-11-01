# Greenfield Walkthrough: Commerce Checkout & Inventory

End-to-end walkthrough of [`spec.yaml`](spec.yaml) across the 12 phases, 4 gates, implementation handoff and change loop of [`GREENFIELD.md`](../../GREENFIELD.md) and [`CONSTITUTION.md`](../../CONSTITUTION.md).

> **Evidence boundary**: [`spec.yaml`](spec.yaml) and the gate decision records below are **illustrative specification and review artifacts**. Validate each gate with `metamodel.py examples/greenfield/spec.yaml --stage <gate>` (prototype `VALID` confirms stage metadata and invariant rules only; Gate 4 acceptance also requires reviewing actual test, benchmark and model-check artifacts per [`spec.md` §§ 8, 11](../../spec.md#8-verification)).

---

## Gate 1 — Discovery (`--stage discovery`)

- **Phases 1–2**: identify `place-order` (`preconditions: [cart_not_empty, customer_active]`, `postconditions: [order_created, inventory_reserved]`, `invariants: [order_paid_at_most_once]`, `failure_modes: [insufficient_inventory, customer_blocked]`) and `reserve-inventory` (`invariants: [stock_never_negative]`, `failure_modes: [insufficient_inventory, product_not_found]`).

| Record | Content |
|---|---|
| Scope | Gate 1 (`discovery`), capabilities `place-order@1.0` and `reserve-inventory@1.0` |
| Responsibility | Domain lead (author), checkout/inventory engineers (reviewers) |
| Findings | Success and failure outcomes cover blocked customers, missing products and insufficient stock; invariants `order_paid_at_most_once` and `stock_never_negative` stated without implementation coupling |
| Outcome | **Pass** — proceed to Gate 2 (`architecture`) |
| Follow-up | Derive component boundaries from the two consistency invariants; define request/response models and failure status codes |

---

## Gate 2 — Architecture (`--stage architecture`)

- **Phases 3–6, 9–10**:
  - Split `checkout` (`application`, `provides: [place-order]`, `requires: [reserve-inventory@^1.0]`, `owns_data: [order]`) from `inventory` (`domain`, `provides: [reserve-inventory]`, `owns_data: [stock-item, order-stock-projection]`).
  - Define `stock-item` (`internal`, from `erp-product-feed`), `order` (`confidential`), and `order-stock-projection` (`confidential`, `derived_from: [order, stock-item]`).
  - Bind `place-order` (`POST /v1/orders`) and `reserve-inventory` (`POST /v1/reservations`) via `interface`, mapping every failure mode to a status code.
  - Context budget: each component has ≤1 provided capability, ≤1 required capability and ≤2 owned data models (within `5 / 8 / 4`).

| Record | Content |
|---|---|
| Scope | Gate 2 (`architecture`), components `checkout@1.0`, `inventory@1.0` and data models `order@1.0`, `stock-item@1.0`, `order-stock-projection@1.0` |
| Responsibility | Architect (author), domain & platform reviewers |
| Findings | Layering (`application → domain`) and single-owner data rules hold; `order-stock-projection` preserves `confidential` classification from `order`; both components fit the context budget |
| Outcome | **Pass** — proceed to Gate 3 (`verification`) |
| Follow-up | Specify verifiable criteria across all 10 categories and assess formal verification for concurrency and projection integrity |

---

## Gate 3 — Verification (`--stage verification`)

- **Phases 7–8**:
  - Define requirements with `category` covering all 10 verification categories (`req-order-func`, `req-order-sem`, `req-order-contract`, `req-order-serial`, `req-reserve-conc`, `req-latency`, `req-scale`, `req-resilience`, `req-security`, `req-observability`, `req-projection`).
  - Flag `req-reserve-conc` (`stock_never_negative`) for [`tla`](../../.github/skills/tla/SKILL.md) (`tla+::Inventory.tla`, `method: model-check`) and `req-projection` (`order-stock-projection` referential integrity) for [`alloy`](../../.github/skills/alloy/SKILL.md) (`alloy::Inventory.als`, `method: model-check`).

| Record | Content |
|---|---|
| Scope | Gate 3 (`verification`), 11 requirements and 3 risks in [`spec.yaml`](spec.yaml) |
| Responsibility | QA/verification lead (author), domain lead (reviewer) |
| Findings | All 10 verification categories are covered with testable or model-checkable criteria; `risk-oversell` is mitigated by `req-reserve-conc` |
| Outcome | **Pass** — issue bounded implementation handoffs |
| Follow-up | Export each component slice with `--context <component-id>`, run TLA+/Alloy checks and execute failing tests before coding |

---

## Implementation Handoff (`checkout` Component)

Prepared per [`CONSTITUTION.md` § Implementation Handoff](../../CONSTITUTION.md#implementation-handoff) using `metamodel.py examples/greenfield/spec.yaml --context checkout`:

- **Target capability & invariants**: `place-order@1.0` (`POST /v1/orders`, sync, idempotent); preserve `order_paid_at_most_once`.
- **Component boundary**: `checkout@1.0` (`layer: application`, `consistency: strong`); owns `order@1.0` (`confidential`); consumes `reserve-inventory@^1.0` (`POST /v1/reservations`).
- **Verification obligations**: `req-order-func`, `req-order-sem`, `req-order-contract`, `req-order-serial`, `req-resilience`, `req-security`, plus system obligations `req-latency`, `req-scale`, `req-observability`.
- **Allowed scope & expected return**: implement `checkout` and derive `.mdl` data artifacts ([`modelable`](../../.github/skills/modelable/SKILL.md)) without accessing `stock-item` storage directly; return requirement-linked test results and affected revisions.

---

## Gate 4 — Release & Change Loop (`--stage release`)

- **Phases 11–12**: [`spec.yaml`](spec.yaml) passes `metamodel.py examples/greenfield/spec.yaml --stage release`; `--impact reserve-inventory` traces affected components, models, requirements and invalidated evidence.
- **Change & reverification**: on any contract or model change, run `--impact` and `--compat`, reopen the earliest affected gate, and rerun invalidated evidence before accepting release.

| Record | Content |
|---|---|
| Scope | Gate 4 (`release`), [`spec.yaml`](spec.yaml) |
| Responsibility | Release owner |
| Findings | `--stage release` passes all 60 metamodel invariants (incl. `EV-04` `subject_revision`, `timestamp` and `verifier`); provenance entries (`pytest::*`, `k6::*`) in [`spec.yaml`](spec.yaml) remain illustrative placeholders, while formal models [`Inventory.tla`](Inventory.tla) and [`Inventory.als`](Inventory.als) are bundled |
| Outcome | **Illustrative Pass (Hold in production until real execution artifacts are attached)** |
| Follow-up | Attach immutable `subject_revision`, `timestamp`, `verifier` and artifact logs from actual test/TLC/Alloy runs before production release |
