# ADR-027: LLM Backend Configuration (Unified Three-Field Setup UI)

**Date:** 2026-04-29
**Revised:** 2026-04-29
**Status:** Accepted
**Authors:** ktjn
**Deciders:** Project Stakeholders
**Review date:** 2026-10-29

## Context

P8-S6 delivered the NLQ pipeline (ADR-021) with a single LLM backend: the OpenAI API,
configurable via `LLM_API_KEY` env var. To use NLQ, a user must supply an API key. The
Setup page only exposed a single API key input and pointed users at `OPENAI_BASE_URL` /
`OPENAI_MODEL` env vars that could not be changed without a container restart.

An initial draft of this ADR (2026-04-29, now superseded) proposed adding a separate "vLLM"
backend mode with a dedicated backend selector in the Setup UI. This was revised because:

1. vLLM exposes an **OpenAI-compatible HTTP API** — the same `async-openai` client and the
   same three config fields work for every OpenAI-compatible provider (OpenAI, Azure, vLLM,
   Ollama, etc.). A backend mode selector adds UI complexity without adding capability.
2. Users simply need to configure **API key, endpoint URL, and model** — the combination
   naturally covers all cases without mode distinction.
3. Connecting to vLLM on a remote machine requires only a custom endpoint URL; no API key
   is needed unless the vLLM server has auth enabled.

## Decision

Observable exposes **three configurable fields** for the LLM connection on the Setup page:

| Setting | `platform_config` key | Env var | Fallback env var | Default |
|---|---|---|---|---|
| API Key | `llm_api_key` | `LLM_API_KEY` | — | _(none)_ |
| Endpoint URL | `llm_url` | `LLM_URL` | `OPENAI_BASE_URL` | `https://api.openai.com/v1` |
| Model | `llm_model` | `LLM_MODEL` | `OPENAI_MODEL` | `gpt-4o-mini` |

Env vars take priority over database values. No backend selector or mode concept exists —
`OpenAiLlmCaller` handles all OpenAI-compatible endpoints via the configurable URL.

### API changes

- `GET /v1/config` response extended with `llm_url: string | null` and `llm_model: string | null`.
  API key is still write-only (`llm_key_configured: bool` only).
- `PUT /v1/config/llm` — new endpoint accepting `{ api_key?, url?, model? }`. Upserts
  whichever fields are present. Returns 204.
- `PUT /v1/config/llm-key` — retained as a backwards-compatible alias.

### XOR obfuscation for the API key

The API key is XOR-obfuscated (hex-encoded) before storage in the `platform_config` table.
This prevents the key from appearing in plaintext in database dumps or log output. The XOR
key is a fixed 32-byte constant embedded in source code — this is **obfuscation, not
encryption**. Real encryption (AES with operator-managed secret) is out of scope for this
local-development-targeted feature; the Setup page now states "API key is obfuscated in
PostgreSQL" rather than "stored in plaintext."

### Setup UI

The `LlmKeyPanel` component is replaced with `LlmConfigPanel` containing three labelled inputs:
- **API Key** (password) — placeholder notes blank is valid for no-auth endpoints
- **Endpoint URL** (url input) — placeholder shows OpenAI default
- **Model** (text input with `<datalist>`) — suggestions include OpenAI and local model names
- Single Save button submits all three fields at once
- Inputs for url and model pre-fill from the server-returned `llm_url` / `llm_model` values

### Invariants (unchanged from ADR-021 and ADR-014)

- All advisory-only, provenance-required, read-only constraints remain in force.
- `OpenAiLlmCaller` is injected via the `LlmCaller` trait; no direct call sites outside `llm_adapter`.

## Consequences

**Easier:**
- Any OpenAI-compatible endpoint (OpenAI, Azure, vLLM local or remote, Ollama) is
  configurable entirely from the Setup page without a container restart.
- No backend mode state to manage in UI or backend; single code path for all providers.
- Legacy `PUT /v1/config/llm-key` callers continue to work unchanged.

**Harder:**
- The operator must know the correct model identifier string for non-OpenAI providers
  (e.g., `microsoft/Phi-3-mini-4k-instruct` for vLLM serving Phi-3). The model `<datalist>`
  in the UI provides common suggestions.

**Constrained:**
- XOR obfuscation is not a substitute for proper secrets management. For production
  deployments, prefer env var injection (`LLM_API_KEY`) from a secrets manager.

## Related

- [ADR-021](ADR-021-nl-query-layer.md) — NL query layer architecture; `LlmCaller` trait
- [ADR-014](ADR-014-ai-feature-boundaries.md) — AI feature boundaries (advisory-only, provenance required)
- [spec/08-ai-ml.md §13.1](../08-ai-ml.md) — NLQ spec, LLM backend configuration
