# Brownfield Walkthrough: Legacy Billing Strangler Migration

Progression of a legacy billing monolith through the 5 reverse-discovery steps, 4 adoption levels, a failed gate recovery, and a strangler migration handoff of [`BROWNFIELD.md`](../../BROWNFIELD.md) and [`CONSTITUTION.md`](../../CONSTITUTION.md), ending in [`spec.yaml`](spec.yaml) and verified against [`old.yaml`](old.yaml) with `--compat`.

> **Evidence boundary**: [`level0.yaml`](level0.yaml), [`level1.yaml`](level1.yaml), [`old.yaml`](old.yaml) and [`spec.yaml`](spec.yaml) are **illustrative process snapshots**, and the gate records below are **illustrative review decisions**. Validate each level with `metamodel.py <spec> --stage <stage>` (prototype `VALID` confirms stage metadata and waiver rules only; release acceptance also requires reviewing actual execution artifacts per [`spec.md` §§ 7, 8, 11](../../spec.md#7-risks)).

---

## Reverse Discovery (Steps 1–5)

1. **Boundaries & entrypoints**: discover HTTP endpoint `issue-invoice` and batch job `charge-invoice`, plus external payment gateway `stripe-gateway`.
2. **Data & state**: extract `invoice` (`confidential`) and `ledger-entry` (`confidential`, `derived_from: [invoice]`) from SQL tables.
3. **Components & dependencies**: cluster into `invoicing` (`application`) and `payment-worker` (`domain`). Initially `payment-worker` wrote directly to `invoice` (shared ownership violation).
4. **Invariants & failures**: mine guards (`amount_positive`, `invoice_unpaid`), invariant `invoice_charged_at_most_once`, and failure modes (`card_declined`, `gateway_timeout`) from existing handlers.
5. **Gaps & risks**: record shared-table ownership and missing verification categories as `Risk` items with remediation requirements.

---

## Level 0 — Observed Inventory (`--stage inventory`, [`level0.yaml`](level0.yaml))

- Register `billing-system@1.0`, `stripe-gateway`, and component stubs `invoicing` and `payment-worker` (`status: draft`).

| Record | Content |
|---|---|
| Scope | Level 0 (`inventory`), [`level0.yaml`](level0.yaml) |
| Responsibility | Onboarding lead (author), billing maintainers (reviewers) |
| Findings | Both deployable units (`invoicing`, `payment-worker`) and external gateway (`stripe-gateway`) are registered with unique IDs; shared table writes flagged for Level 1 |
| Outcome | **Pass** — advance to Level 1 (`architecture`) |
| Follow-up | Extract SQL schemas and map observed cross-module dependencies without idealizing boundaries |

---

## Level 1 — Dependency Mapping & Discrepancy Review (`--stage architecture`, [`level1.yaml`](level1.yaml))

- Map `issue-invoice` and `charge-invoice`, data models `invoice` and `ledger-entry`, and record the observed shared-table discrepancy (`payment-worker` also claims `invoice` in `owns_data`) via a waiver on `risk-shared-invoice-table` (`waives: shared-data-ownership`, `mitigated_by: [req-split-ownership]`, `expires: verification`).

| Record | Content |
|---|---|
| Scope | Level 1 (`architecture`), [`level1.yaml`](level1.yaml) |
| Responsibility | Onboarding lead (author), tech lead (reviewer) |
| Findings | Observed shared-table write on `invoice` (`[DATA-03]`) is recorded as-is and bound to `risk-shared-invoice-table` with remediation `req-split-ownership` expiring at `verification` |
| Outcome | **Defer (Pass Level 1 with bounded waiver)** — remediate shared ownership before Level 2 (`verification`) |
| Follow-up | Sever direct `invoice` table writes from `payment-worker` and pin external contracts with characterization tests |

---

## Failed Gate & Recovery (Attempting Level 2 Before Remediation)

- **Failed check**: running `metamodel.py examples/brownfield/level1.yaml --stage verification` before remediating shared table ownership fails:
  - `[RISK-02]` rejects `risk-shared-invoice-table` (`waiver expired at stage 'verification'`).
  - `[DATA-03]` reports `DataModel 'invoice' is also claimed by 'payment-worker'`.
  - `[REQ-01]` reports missing verification categories.

| Record | Content |
|---|---|
| Scope | Gate review for Level 2 (`verification`) against unremediated [`level1.yaml`](level1.yaml) |
| Responsibility | Tech lead |
| Findings | Level 1 waiver `risk-shared-invoice-table` expired on entry to `verification`; `payment-worker` still writes to `invoice` directly |
| Outcome | **Rework** — keep Level 1/2 transition open; do not extend the waiver without completing the ownership split |
| Follow-up | Issue a bounded migration handoff to remove `invoice` writes from `payment-worker` and attach characterization tests |

---

## Migration Handoff & Level 2 Behavioral Pinning (`--stage verification`, [`old.yaml`](old.yaml))

- **Bounded migration handoff** ([`CONSTITUTION.md` § Implementation Handoff](../../CONSTITUTION.md#implementation-handoff)):
  - **Impact check**: `metamodel.py examples/brownfield/level1.yaml --impact invoice` identifies `invoicing`, `payment-worker`, `issue-invoice`, `charge-invoice`, `ledger-entry` and `req-split-ownership`.
  - **Scope**: remove `invoice` from `payment-worker.owns_data`, keep `ledger-entry` in `payment-worker`, and attach characterization tests (`ev-issue-char`, `ev-charge-char`) freezing `issue-invoice` and `charge-invoice` contracts in [`old.yaml`](old.yaml).
  - **Waiver scope**: remaining unpinned verification categories are deferred under `risk-unpinned-categories` (`waives: verification-category`, `mitigated_by: [req-full-coverage]`, `expires: release`).

| Record | Content |
|---|---|
| Scope | Level 2 (`verification`), [`old.yaml`](old.yaml) |
| Responsibility | Tech lead & QA reviewer |
| Findings | Shared ownership of `invoice` is resolved; characterization records freeze `issue-invoice@1.0` and `charge-invoice@1.0`; remaining category gaps are waived until `release` |
| Outcome | **Defer (Pass Level 2 with coverage waiver expiring at `release`)** — proceed to strangler refactoring and Level 3 conformance |
| Follow-up | Complete all 10 verification categories (including TLA+ for `invoice_charged_at_most_once` and Alloy for `ledger-entry` lineage), close `risk-unpinned-categories`, and verify `--compat` |

---

## Level 3 — Full Conformance & Release Review (`--stage release`, [`spec.yaml`](spec.yaml))

- Extract `charge-invoice@1.1` behind `payment-worker` owning `ledger-entry` while `invoicing` alone owns `invoice`.
- Verify contract preservation with `metamodel.py examples/brownfield/spec.yaml --compat examples/brownfield/old.yaml`.
- Satisfy all 10 verification categories (`req-issue-func`, `req-issue-sem`, `req-issue-contract`, `req-invoice-serial`, `req-charge-conc` via TLA+ and `req-ledger-rel` via Alloy (`method: model-check`), `req-latency`, `req-scale`, `req-resilience`, `req-security`, `req-observability`) with passing `Evidence` and close all waivers.

| Record | Content |
|---|---|
| Scope | Level 3 (`release`), [`spec.yaml`](spec.yaml) compared against [`old.yaml`](old.yaml) |
| Responsibility | Release owner |
| Findings | `--stage release` and `--compat examples/brownfield/old.yaml` pass with zero waivers; provenance entries in [`spec.yaml`](spec.yaml) are illustrative placeholders without bundled execution logs |
| Outcome | **Illustrative Pass (Hold in production until real execution artifacts are attached)** |
| Follow-up | Supply real test, benchmark and model-check logs with immutable subject revisions before closing the production migration |
