# Python uv Migration — Design

## Problem

Five independent Python projects in the repo still use `pip install -r requirements.txt`:
`models/`, `scripts/seed/`, `testbench/api/`, `testbench/worker/`, `testbench/loadgen/`.

`AGENTS.md` already states the target policy ("Python packages: prefer `uv`... do not introduce new pip/requirements.txt... workflows without an ADR-backed exception") but the existing files predate that policy and have not been migrated.

## Goal

Replace `requirements.txt` with `pyproject.toml` + `uv.lock` in all five projects, in a single PR, and switch the Dockerfiles/CI script that install from them over to `uv`.

## Scope

| Project | Has Dockerfile | Nature |
|---|---|---|
| `models/` | no | installs only the `modelable` CLI tool; no project source |
| `scripts/seed/` | yes (`Dockerfile.seed`, repo root) | real package, `python -m seed`, has `tests/` |
| `testbench/api/` | yes | standalone FastAPI service |
| `testbench/worker/` | yes | standalone worker service |
| `testbench/loadgen/` | yes | standalone load generator |

## Approach

Each `pyproject.toml` is a dependency manifest only — `[tool.uv]\npackage = false` — since none of these are published packages; they're apps/scripts with pinned dependencies. This avoids build-system boilerplate or `src/` layout changes.

- **Pinning style preserved per project:** `testbench/*` keep exact `==` pins; `scripts/seed` and `models` keep `>=` floors. Achieved via `uv add <pkg><existing-specifier>` per line, then `uv lock`.
- **Python version preserved per project:** `requires-python` matches each project's current target — `>=3.12` for `scripts/seed` (matches `Dockerfile.seed`'s `python:3.12-slim`), `>=3.14` for the three `testbench/*` services (matches their `python:3.14-slim` base images), and `>=3.12` for `models/` (no Docker image of its own; pinned to the repo's lowest existing floor for consistency).
- **Dockerfiles** (`Dockerfile.seed`, `testbench/api/Dockerfile`, `testbench/worker/Dockerfile`, `testbench/loadgen/Dockerfile`): copy the static `uv` binary from `ghcr.io/astral-sh/uv`, `COPY pyproject.toml uv.lock`, run `uv sync --locked --no-dev`, then invoke the app via `uv run <cmd>` (or put `.venv/bin` on `PATH`) instead of `pip install --no-cache-dir -r requirements.txt`.
- **`scripts/local-ci.sh`:** the modelable step currently shells out to a globally-installed `modelable` binary and tells the user to `pip install -r models/requirements.txt` if missing. Replace with `uv run --project models modelable validate models/`, updating the skip/fallback message accordingly.
- **AGENTS.md:** no edits required — the existing policy text already describes this end state.
- Delete each project's `requirements.txt` once its `pyproject.toml`/`uv.lock` are verified working.

## Out of scope

- Bumping Python versions (e.g. moving `scripts/seed` from 3.12 to 3.14) — this migration changes package manager only, not runtime versions.
- Converting `scripts/seed` into a proper installable/publishable package (`package = false` is sufficient; the Dockerfile already `COPY`s source separately rather than installing it).
- Any change to dependency versions themselves beyond what `uv lock` resolves within the existing specifiers.

## Verification

- `docker build` each of the four Dockerfiles (`Dockerfile.seed`, `testbench/api`, `testbench/worker`, `testbench/loadgen`) and confirm they build and the resulting image starts.
- `uv run pytest` inside `scripts/seed/` to confirm its existing test suite still passes against the uv-managed environment.
- `bash scripts/local-ci.sh` (modelable step) to confirm the new `uv run --project models modelable validate models/` invocation works end-to-end.
- If `testbench/*` services are normally exercised via `docker compose`, bring the stack up and confirm the services boot.
