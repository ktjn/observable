# Repo Analysis — 2026-05-01

**Date:** 2026-05-01
**Status:** Initial review
**Scope:** Whole-repo survey for anomalies, drift, and small improvement opportunities.
**Baseline:** Phase 2 closed; Phase 3 substantially delivered through P3-S7 (faceting); P8-S6 (NLQ pipeline) and P8-S6b (local-LLM unification under ADR-027) implemented; multi-model NLQ eval added today (PR #204).

This report complements `2026-04-19-gaps-analysis.md` (still the canonical strategic gap analysis) and focuses on **drift, hygiene, and small concrete fixes** rather than re-litigating the Phase 4–8 backlog.

---

## 1. High-priority anomalies

### 1.1 Migration filename collision: two `011_*.sql` files

`migrations/postgres/` contains two migrations sharing the same `011` prefix:

```
011_create_dashboards.sql
011_create_schema_registry.sql
012_add_worker_metrics.sql
013_platform_config.sql
```

`docker-compose.yml`'s `postgres-setup` step applies them with a shell glob `for f in /migrations/postgres/*.sql`. Apply order is therefore alphabetical (`d` before `s`), which happens to be deterministic today, but:

- ADR-013 ("Schema Governance") relies on monotonically increasing sequence numbers as the contract; two `011` files break that contract semantically.
- Anyone reading the directory cannot tell which `011` ran first without inspecting the script.
- Future authors may add a third `011_*.sql` (or `010b_*`) and hit a real ordering bug.

**Recommendation:** Renumber one of them. The simplest fix is `014_create_schema_registry.sql` (it's the newer feature and has no foreign-key dependency on dashboards). Add a one-line note to ADR-013 if it doesn't already forbid duplicate sequence numbers explicitly.

### 1.2 `docs/superpowers/plans/2026-04-29-p8-s6b-local-llm-vllm.md` is stale vs ADR-027

The P8-S6b plan still describes the **superseded** "vLLM backend selector" approach:

- `PUT /v1/config/llm-backend` endpoint with `{ backend: "openai" | "vllm", model, vllm_base_url }`
- Backend mode selector in the Setup UI
- DB keys `llm_backend`, `vllm_base_url`, model values `phi3-mini` / `llama3-8b`

ADR-027 (revised 2026-04-29) explicitly **rejected** that design in favour of a unified three-field setup (`llm_api_key`, `llm_url`, `llm_model`) with no backend selector, no `vllm_base_url`, and the canonical endpoint `PUT /v1/config/llm`. The implemented code matches ADR-027, not the plan document.

The same drift appears in the parent iteration plan: `2026-04-18-phases2-8-iteration-plan.md` line 791 still describes the vLLM-selector approach as the closure for ADR-027 + P8-S6b.

**Recommendation:** Either (a) mark the P8-S6b plan as superseded with a one-line pointer to ADR-027, or (b) rewrite it to match ADR-027. The iteration-plan footnote on line 791 should be rewritten to describe the unified-fields outcome that actually shipped.

### 1.3 Agent-instruction file fan-out has drifted (`AGENT.md` ≠ `AGENTS.md` ≠ `CLAUDE.md` ≠ `GEMINI.md`)

Four near-duplicate top-level files exist; their checksums are:

| File | md5 | Length |
|---|---|---|
| `AGENT.md` | `7fee984d…` | 60 lines |
| `GEMINI.md` | `7fee984d…` | 60 lines |
| `AGENTS.md` | `8929ae01…` | 67 lines |
| `CLAUDE.md` | `bee2fdb3…` | 84 lines |

Diffs show:

- `AGENTS.md` adds an "Agent Role Model" section pointing to `.github/agents/`.
- `CLAUDE.md` further adds "Before Starting Any Implementation Task" (ADR-reading discipline + dependency-version policy) and "CI and Scripts".
- `AGENT.md` and `GEMINI.md` are missing both blocks.

This means an agent that only loads `AGENT.md` or `GEMINI.md` will not see the dependency-version policy or the ADR-first reading rule, both of which are referenced from `spec/10-process.md` as mandatory.

**Recommendation:** Make one file canonical (suggest `AGENTS.md`, since that is the convention emerging across the ecosystem and already used by the GitHub Coordinator entry-point). Either symlink the others to it, or rewrite each to be a thin pointer (e.g., `AGENT.md` → "See [AGENTS.md](AGENTS.md)"). No information should diverge.

### 1.4 NLQ multi-model script is undocumented in the spec/process

PR #204 (today) added `scripts/nlq-multi-model.py`, but:

- `spec/08-ai-ml.md §13.4` and `spec/10-process.md §"NLQ Quality Gate"` only mention `scripts/nlq-eval.py`.
- `AGENTS.md` / `CLAUDE.md` quality-gate section likewise mentions only `nlq-eval.py`.
- `README.md` does not reference the multi-model harness.

The multi-model harness is more useful than the single-model one for catching regressions when changing the prompt, IR schema, or repair loop, because it demonstrates that a change does not regress _any_ model. Worth promoting it from "convenience script" to "named regression gate".

**Recommendation:** Add a short section to `spec/08-ai-ml.md §13.4` (and the matching NLQ Quality Gate text in `AGENTS.md` / `CLAUDE.md`) describing when to run `nlq-multi-model.py` vs `nlq-eval.py`. Suggested rule: "use `nlq-eval.py` for fast iteration against the configured model; use `nlq-multi-model.py` before merging any prompt/IR change to confirm no model regressed". Update `tests/nlq/results/.gitignore` cohabitation check (already done in PR #204).

---

## 2. Medium-priority hygiene

### 2.1 Docker image pinning below CLAUDE.md policy

`CLAUDE.md` policy: "Docker images (Compose/local): pin to `image:major.minor` at minimum. For production Dockerfiles and base images, use `image:major.minor.patch`; SHA digest is strongly preferred."

Current state in `docker-compose.yml` and `Dockerfile`:

| Image | Pin | Policy bucket | Verdict |
|---|---|---|---|
| `clickhouse/clickhouse-server:24.3` | major.minor | Compose | ✅ |
| `postgres:16` | major only | Compose | ⚠ below `major.minor` floor |
| `openfga/openfga:v1.5` | major.minor | Compose | ✅ |
| `redpandadata/redpanda:v23.3.1` | full | Compose | ✅ |
| `lukemathwalker/cargo-chef:0.1.77-rust-1.95.0-bookworm` | full | Builder | ✅ |
| `debian:bookworm-slim` | codename only | Production runtime | ⚠ no patch, no SHA |

**Recommendation:** Bump `postgres:16` → `postgres:16.6` (or whatever current stable is). For `debian:bookworm-slim` in the runtime stage, consider pinning to a SHA digest as the policy strongly prefers, and certainly to a patch version.

### 2.2 LLM env-var pass-through still uses legacy names in compose

`services/query-api/src/config.rs` makes `LLM_URL` / `LLM_MODEL` the canonical env vars, with `OPENAI_BASE_URL` / `OPENAI_MODEL` retained as **fallbacks**. But `docker-compose.yml` only forwards the legacy names:

```yaml
OPENAI_BASE_URL: ${OPENAI_BASE_URL:-}
OPENAI_MODEL:    ${OPENAI_MODEL:-}
```

It should forward the canonical names too (passing both is fine — the fallback chain still resolves correctly). Today, anyone setting `LLM_MODEL` in `.env.local` expecting it to take effect in compose-mode will be silently ignored.

**Recommendation:** Add `LLM_URL: ${LLM_URL:-}` and `LLM_MODEL: ${LLM_MODEL:-}` to the `query-api` service env block; keep the legacy ones for backwards compat.

### 2.3 Production unwraps and expects in services

`grep -rn "unwrap()|expect(" services/ --include="*.rs" | grep -v test` reveals **128 unwraps and 11 expects** in non-test code. Many are at startup (env-var parsing, schema-load) where panic is acceptable, but the Rust data-plane ADR-004 calls for "panic-free request paths". A targeted audit would distinguish:

- Acceptable unwraps in `main.rs` startup paths
- Risky unwraps in request handlers, DB-row decoding, or LLM-response parsing

A clippy lint (`#![warn(clippy::unwrap_used, clippy::expect_used)]` per crate) with explicit `#[allow]` at startup sites would convert this from an unstructured count into a reviewable contract.

**Recommendation:** As a small slice, enable `clippy::unwrap_used` warning (not deny) at the workspace level, then file an issue per crate to drive the count down. Don't block this PR or any current iteration on it.

### 2.4 `target/` is 32 GB

Local `target/` directory has grown to 32 GB. This is normal for incremental Rust builds with multiple feature sets but worth flagging for contributor onboarding docs.

**Recommendation:** Consider adding a `make clean-deep` or note in CONTRIBUTING about `cargo clean` cadence. Not a blocker.

---

## 3. Plan / spec / code reconciliation

### 3.1 P3-S7 Faceting was merged but not yet checked off

The latest `2026-04-18-phases2-8-iteration-plan.md` shows P3-S7 as ✅ in the completed list. ✅ — no action.

### 3.2 NLQ test cases.json now at 27 cases — versioning?

`tests/nlq/cases.json` has grown from 21 → 27 cases without explicit versioning. The eval harness writes `last-run.json` as a snapshot, but there is no schema version on `cases.json` itself. As models and the IR evolve, regression-comparison across older runs gets harder.

**Recommendation:** Add a top-level `{ "schema_version": 1, "cases": [...] }` wrapper or a `tests/nlq/CHANGELOG.md` recording case additions/removals. Low priority.

### 3.3 `.github/workflows/build.yml` is fully disabled

All triggers in `build.yml` are commented out except `workflow_dispatch`. This is consistent with the AGENTS.md statement "GitHub CI is disabled — do not push and rely on it to catch errors", but a fresh contributor reading `.github/workflows/` may still expect CI to run on PR. Consider adding a one-line comment at the top of the workflow file pointing to `scripts/local-ci.sh`.

---

## 4. Suggestions (no immediate action required)

1. **Promote `nlq-multi-model.py` to a regression gate** — once Ollama is reliably available in dev environments, run it as part of `local-ci.sh --with-llm` (opt-in) so prompt/IR changes are validated against all three reference models in a single command.
2. **Centralise the LLM connection settings docs** — there are now four places that describe the env-var/DB precedence (config.rs comments, ADR-027, P8-S6b plan, Setup UI placeholder). Pick one source of truth (ADR-027) and have the others cite it.
3. **Track per-model NLQ baseline** — store an expected pass count per model in `tests/nlq/cases.json` (e.g., `phi3:latest` is currently 27/27; phi3.5 21/27; llama3.1 22/27) so the multi-model script can fail loudly when a model that previously passed all cases regresses, without needing a code change.

---

## 5. Out of scope for this report

- Phase 4–8 strategic gaps (already covered by `2026-04-19-gaps-analysis.md`)
- Frontend UI quality / accessibility audit (UI-R1/R2/R3 lane has its own plans)
- Performance tuning (perf-smoke is the active gate)
- Test coverage gaps in individual services (a separate audit)

---

## 6. Suggested next slices (if any of the above are accepted)

| Anomaly | Slice size | Touches |
|---|---|---|
| 1.1 — rename `011_create_schema_registry.sql` | XS | `migrations/postgres/`, ADR-013 (one-line note) |
| 1.2 — reconcile P8-S6b plan with ADR-027 | XS | `docs/superpowers/plans/2026-04-29-p8-s6b-local-llm-vllm.md`, iteration plan §791 |
| 1.3 — collapse agent-instruction files | S | `AGENT.md`, `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, `README.md` |
| 1.4 — document `nlq-multi-model.py` | XS | `spec/08-ai-ml.md`, `AGENTS.md`, `CLAUDE.md`, `README.md` |
| 2.1 — bump image pins | XS | `docker-compose.yml`, `Dockerfile` |
| 2.2 — add canonical LLM env-vars to compose | XS | `docker-compose.yml` |

Each is independently shippable as a single-purpose PR per AGENTS.md "Branch and PR Every Iteration" rule.
