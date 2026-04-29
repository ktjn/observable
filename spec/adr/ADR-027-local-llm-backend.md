# ADR-027: Local LLM Backend (vLLM)

**Date:** 2026-04-29
**Status:** Proposed
**Authors:** ktjn
**Deciders:** Project Stakeholders
**Review date:** 2026-10-29

## Context

P8-S6 delivered the NLQ pipeline (ADR-021) with a single LLM backend: the OpenAI API (or any
OpenAI-compatible provider reachable via `OPENAI_BASE_URL`). To use natural language query, a
user must supply an `LLM_API_KEY`, which requires a cloud API account and incurs ongoing cost.

This creates a barrier for:
- Local development and evaluation without cloud credentials
- Air-gapped or security-sensitive environments
- Cost-sensitive operators who prefer a one-time GPU investment over per-token pricing

[vLLM](https://github.com/vllm-project/vllm) is an open-source LLM inference server that
exposes an **OpenAI-compatible HTTP API**. It can serve open-weight models locally (CPU or GPU).
Connecting Observable to a local vLLM instance requires no new HTTP client library — the
existing `async-openai` crate is reused with no auth key and a configurable base URL.

Two models are offered:
- **Phi-3 Mini 3.8B** (`microsoft/Phi-3-mini-4k-instruct`) — default; low resource requirement
  (~2.5 GB VRAM or fast CPU inference), strong instruction following for its size, MIT-licensed.
- **Llama-3 8B Instruct** (`meta-llama/Meta-Llama-3-8B-Instruct`) — selectable alternative;
  higher accuracy for complex NLQ IR generation, requires ~6 GB VRAM, Meta Llama 3 Community
  License.

Observable does not run vLLM itself. The user is responsible for starting a vLLM server and
pointing Observable at its endpoint. Observable stores only the connection configuration.

## Decision

Observable will support **vLLM as an opt-in local LLM backend** for the NLQ pipeline alongside
the existing OpenAI API backend. The OpenAI API remains the default.

### Backend selection

The active LLM backend is determined by the `llm_backend` configuration key, following the same
precedence pattern as the existing `LLM_API_KEY`:

1. Env var `LLM_BACKEND` (`openai` | `vllm`) — highest priority; set at container start.
2. `platform_config` database row (key = `llm_backend`) — runtime-configurable via Setup UI.
3. Default: `openai`.

Additional config keys for the vLLM path:

| Key | Env var override | Default | Description |
|---|---|---|---|
| `llm_backend` | `LLM_BACKEND` | `openai` | Active backend: `openai` or `vllm` |
| `llm_model` | `LLM_MODEL` | `phi3-mini` | Model selection: `phi3-mini` or `llama3-8b` |
| `vllm_base_url` | `VLLM_BASE_URL` | `http://localhost:8000` | vLLM server endpoint |

All keys live in the existing `platform_config` PostgreSQL table (key-value store). No schema
migration is required; new keys are upserted on first write.

### VllmCaller implementation

`VllmCaller` implements the `LlmCaller` trait using `async-openai` configured with:
- `api_base` → `vllm_base_url` (from env or DB)
- `api_key` → empty string (vLLM does not require authentication by default)
- `model` → resolved model string from `llm_model` config key

Model strings used with vLLM:

| Config value | Model string passed to vLLM |
|---|---|
| `phi3-mini` | `microsoft/Phi-3-mini-4k-instruct` |
| `llama3-8b` | `meta-llama/Meta-Llama-3-8B-Instruct` |

### Setup UI changes (P8-S6b)

The Setup page `LlmKeyPanel` is replaced with a `LlmConfigPanel` offering a backend mode
selector:

- **OpenAI API** — existing API key input, unchanged.
- **vLLM (local)** — model dropdown (Phi-3 Mini selected by default, Llama-3 8B as
  alternative) + endpoint URL input pre-filled with `http://localhost:8000`.

The badge on the panel reflects the currently configured backend.

### Invariants (unchanged from ADR-021 and ADR-014)

- All advisory-only, provenance-required, read-only constraints remain in force for both backends.
- `VllmCaller` is injected via the `LlmCaller` trait; no direct call sites outside `llm_adapter`.
- The `GET /v1/config` endpoint returns `llm_backend` and `llm_model` alongside
  `llm_key_configured` so the UI can reflect the active configuration.
- A new `PUT /v1/config/llm-backend` endpoint accepts `{ backend, model, vllm_base_url? }`.

## Consequences

**Easier:**
- Local evaluation and development of NLQ without any cloud API account.
- Air-gapped deployments can use NLQ with self-hosted models.
- No new HTTP client dependency — `async-openai` already handles OpenAI-compat endpoints.
- Configuration follows the existing env-var-or-DB precedence pattern; no new patterns introduced.

**Harder:**
- NLQ output quality depends on the chosen model. Phi-3 Mini is smaller and may produce less
  accurate NLQ IR for complex multi-signal questions. Users must be informed of this trade-off.
- vLLM server availability and health are outside Observable's control. The NLQ endpoint must
  surface a clear error when vLLM is unreachable (503 with actionable message).
- The operator must pre-download model weights to the vLLM server. Observable does not manage this.

**Constrained:**
- Observable does not bundle, start, or manage the vLLM process. This is intentional — vLLM
  resource requirements (CPU/GPU, RAM) are highly environment-specific.
- Only the two enumerated models are selectable via the UI. Arbitrary model strings require env
  var override (`LLM_MODEL`) for advanced use — this prevents typo-driven misconfiguration.
- vLLM authentication (API key / bearer token) is not supported in this slice. If a vLLM server
  requires auth, the operator must use `OPENAI_BASE_URL` + `LLM_API_KEY` env vars with the
  OpenAI backend (vLLM accepts this combination).

## Alternatives Considered

### Option A: Ollama backend

Ollama is popular for local LLM serving. Its HTTP API differs from the OpenAI format, which would
require a new HTTP client or a thin adapter. vLLM's native OpenAI-compat API requires no such
adapter. Rejected in favour of vLLM for this slice; Ollama support can be added as a follow-on
using the same `LlmCaller` trait extension point.

### Option B: OPENAI_BASE_URL env var only (no UI)

Users can already point `OPENAI_BASE_URL` at any OpenAI-compatible server, including vLLM. This
works but requires a container restart and offers no Setup UI feedback. Rejected because the
stated requirement is a Setup UI toggle — env-var-only is an advanced escape hatch, not an
operator-friendly UX.

### Option C: Embed a local inference runtime

Embedding llama.cpp or similar via FFI would remove the external vLLM dependency. Rejected
because it adds significant build complexity, OS/GPU-specific binary dependencies, and increased
binary size. The vLLM HTTP bridge adds negligible overhead relative to LLM inference latency.

## Related

- [ADR-021](ADR-021-nl-query-layer.md) — NL query layer architecture; `LlmCaller` trait
- [ADR-014](ADR-014-ai-feature-boundaries.md) — AI feature boundaries (advisory-only, provenance required)
- [spec/08-ai-ml.md §13.1](../08-ai-ml.md) — NLQ spec, LLM backend configuration
- [P8-S6b iteration slice](../../docs/superpowers/plans/2026-04-29-p8-s6b-local-llm-vllm.md)
