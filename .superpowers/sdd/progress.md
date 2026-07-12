# SDD Progress - Field View Column Toggle

Plan: docs/superpowers/plans/2026-07-12-field-view-column-toggle.md
Branch: fix/issue-536-field-add-column
Started: 2026-07-12

## Tasks

- [x] Task 1: Capture regression at component and browser levels
- [x] Task 2: Implement shared add/remove semantics
- [x] Task 3: Unify log field columns and toggling
- [x] Task 4: Add trace field resolution and toggling
- [x] Task 5: Verify and archive plan
- [ ] Task 6: Publish for human review

## Log

Task 1: complete (commits 8c3701a..c574b14, review clean; minor: report retains superseded pre-amend SHA and RED wording in its opening section)
Task 2: complete (commits c574b14..96e09ba, review clean; minor: stop-propagation and blur semantics are implemented but not directly tested)
Task 3: complete (commits 96e09ba..0b21329, review clean; minors: existing row keyboard semantics, timestamp nowrap visual check, stale numeric-severity sentence in scratch report)
Task 4: complete (commits 0b21329..616d691, review clean; process note: scratch report opening retains superseded pre-correction statements)
Task 5: complete (commits 616d691..HEAD plus new fix; visual-suite inspection of panel-log-open.png caught a real regression the plan step 3 warned about — the context-panel value grid column collapsed to ~55px because `minmax(88px,auto)` let long attribute-key labels (e.g. `resource.k8s.namespace.name`) expand the label track, wrapping every value one character per line; fixed by capping the label track to `minmax(88px,45%)` in both LogSearch.tsx and TraceSearch.tsx; `bash scripts/local-ci.sh` passes with `SKIP_MODELABLE=1` — the modelable diff-check has a pre-existing Windows-only false positive (Python writes CRLF, committed artifacts are LF; content is byte-identical after normalizing), unrelated to this branch's changes)
