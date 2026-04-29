# Collectable — Architectural Decision Records

This folder contains Architectural Decision Records (ADRs) for **Collectable**.

Collectable is an independent tool. Its ADRs live here and are not part of the
Observable `spec/adr/` hierarchy. New decisions about Collectable's design,
technology choices, or behaviour belong here.

## Index

| ADR | Title | Status |
|-----|-------|--------|
| [ADR-001](ADR-001-independent-compiled-mediator.md) | Independent Compiled-Mediator Architecture | Accepted |

## Conventions

- **Numbering:** Sequential integers starting at `001`. Use `ADR-000-template.md`
  as the starting point for every new record.
- **File naming:** `ADR-NNN-short-kebab-title.md`
- **Status lifecycle:** `Proposed` → `Accepted` → `Superseded` / `Deprecated`
- **One decision per ADR.** If a decision has multiple distinct sub-choices, split
  them into separate ADRs and cross-reference.
- **Immutability:** Once `Accepted`, the body of an ADR is not edited. Write a new
  ADR that supersedes it instead.
