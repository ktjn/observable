# SDD Progress - Column Key Toggle and Reorder

Plan: docs/superpowers/plans/2026-07-13-column-key-toggle-and-reorder.md
Branch: feat/column-key-toggle-and-reorder
Started: 2026-07-13

## Tasks

- [x] Task 1: Move the +/- toggle button into the <dt> label cell
- [x] Task 2: Add drag-and-drop reordering to ColumnPickerControl
- [x] Task 3: useColumnPreferences hook (localStorage-backed order + visibility)
- [x] Task 4: Wire useColumnPreferences into the log explorer
- [x] Task 5: Wire useColumnPreferences into the trace explorer
- [ ] Task 6: Full verification, roadmap entry, and final gate

## Log

Task 1: complete (commits ec76bde..acbb3cd, review clean)
Task 2: complete (commits acbb3cd..456c65c, review clean; minor: no onDragEnd to clear stale dragKey on aborted drag, self-correcting, not a blocker)
Task 3: complete (commits 456c65c..f6e5b8b, review clean after one fix for stale-hidden-column re-append bug; plus follow-up cleanup commit 35f27f3 removing stray __pycache__ files the fix subagent accidentally committed)
Task 4: complete (commits e7f67dd..c20695c, review clean; minor design note: shared localStorage key means all LogExplorer instances -- main page and per-service embeds -- share one persisted column set, inherent to the plan, not a defect)
Task 5: complete (commits e4bbc19..b0f5edd, review clean)
