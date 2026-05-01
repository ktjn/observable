# Documentation Cleanup & Archival Plan

**Date:** 2026-05-01
**Status:** Draft / Pending Approval
**Author:** Gemini CLI

## 1. Objective
To reduce context clutter for AI agents and maintain a high signal-to-noise ratio in the active codebase by moving historical, completed, and superseded documentation to a top-level `archived/` directory that is ignored by default agent crawling.

## 2. Archival Strategy

### 2.1 Directory Structure
Create a top-level `archived/` directory with the following structure:
- `archived/plans/`: Historical implementation and iteration plans.
- `archived/analysis/`: Superseded repo reviews and gap analyses.
- `archived/specs/`: Obsolete specifications (if any).

### 2.2 Archival Candidates

**From `docs/superpowers/plans/` to `archived/plans/`:**
- `2026-04-17-phase1-internal-mvp.md` (Phase 1 closure)
- `2026-04-16-doc-spec-review-process.md` (Process established and merged into `spec/10-process.md`)
- `2026-04-16-metrics-storage-local-dev-spec-updates.md` (Integrated into `spec/03-storage.md`)
- `2026-04-21-dependency-maintenance-policy.md` (Integrated into `AGENTS.md`)
- `2026-04-23-infrastructure-inventory-plan.md` (P3-S9 complete)
- `2026-04-24-p3-s10-infra-correlation.md` (P3-S10 complete)
- `2026-04-26-p3-s11-deployment-markers.md` (P3-S11 complete)
- `2026-04-26-p3-s6e-accessibility.md` (P3-S6e complete)
- `2026-04-28-p3-s6d-threshold-alert-ui.md` (P3-S6d complete)
- `2026-04-29-ui-r3-legacy-cleanup.md` (UI-R3 complete)
- `2026-04-29-p8-s6b-local-llm-vllm.md` (Superseded by ADR-027)

**From `docs/analysis/` to `archived/analysis/`:**
- `2026-04-19-gaps-analysis.md` (Superseded by current review)

## 3. Agent Instruction Consolidation

### 3.1 Canonical File
Promote `AGENTS.md` to be the **canonical source of truth** for all agent instructions and repository mandates. It must include:
- All content from `GEMINI.md` (foundational mandates).
- All content from `CLAUDE.md` (ADR-first discipline, dependency pinning, CI scripts).
- The "Agent Role Model" from `AGENTS.md`.

### 3.2 Pointers
Replace the content of the following files with a single-line pointer:
- `AGENT.md`: "This file is a pointer to [AGENTS.md](AGENTS.md)."
- `CLAUDE.md`: "This file is a pointer to [AGENTS.md](AGENTS.md)."
- `GEMINI.md`: "This file is a pointer to [AGENTS.md](AGENTS.md)."

## 4. Reference Synchronization

- **`README.md`**: Update links in the "Documentation" section to reflect the new `archived/` directory.
- **`spec/README.md`**: Update any links to archived plans.
- **`AGENTS.md`**: Update internal links to the historical Phase 1 plan.

## 5. Visibility Control
To ensure agents do not "see" the archived files unless explicitly directed, the `archived/` folder should be added to `.geminiignore` (or equivalent). This prevents the files from being indexed or searched during broad discovery tasks, while still allowing an agent to read them if provided with the specific path.

## 6. Execution Steps
1. Create `archived/plans/` and `archived/analysis/`.
2. Move identified candidates to their respective archive folders.
3. Merge `CLAUDE.md` and `GEMINI.md` content into `AGENTS.md`.
4. Rewrite `AGENT.md`, `CLAUDE.md`, and `GEMINI.md` as pointers.
5. Update `README.md` and `spec/README.md` links.
6. Verify link integrity and agent visibility (ensure `glob` doesn't pick up archived files).
