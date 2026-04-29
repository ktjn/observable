# Collectable — Development Plans

This folder contains development and iteration plans for **Collectable**.

Drop a new plan file here before starting any significant chunk of work.
Keep Observable's planning folders clean — Collectable plans belong here.

## Naming convention

```
YYYY-MM-DD-short-kebab-description.md
```

Examples:

```
2026-05-10-transport-expansion.md
2026-06-01-ui-ux-improvements.md
```

## Index

| Plan | Summary |
|---|---|
| [2026-04-28-testing-transport-buffering.md](2026-04-28-testing-transport-buffering.md) | Playwright UI tests, transport config completeness, output buffering, end-to-end pipeline tests |

## What to put in a plan file

A plan file is lightweight — it is not a specification. A good plan covers:

- **Goal:** One sentence describing what this work achieves.
- **Scope:** What is in and what is explicitly out.
- **Approach:** The key steps or decisions needed.
- **Done criteria:** How you know the work is complete.

Avoid putting design decisions in plan files. If a decision needs to outlive the
plan, write an ADR in `docs/adr/` and link to it from the plan.
