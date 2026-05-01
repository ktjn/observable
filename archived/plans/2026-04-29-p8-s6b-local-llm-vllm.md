# P8-S6b: Local LLM Backend (vLLM + Phi-3 Mini / Llama-3 8B)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the NLQ pipeline to support a local vLLM backend as an opt-in alternative to
the OpenAI API. The Setup page gains a backend mode selector. Phi-3 Mini is the default local
model; Llama-3 8B Instruct is a selectable alternative.

**Architecture:** vLLM exposes an OpenAI-compatible HTTP API, so `VllmCaller` reuses the
existing `async-openai` crate with no auth key and a configurable base URL. No new HTTP client
dependency is introduced. Config is stored in the existing `platform_config` key-value table
(no schema migration). Backend selection follows the existing env-var-or-DB precedence pattern.
See [ADR-027](../../spec/adr/ADR-027-local-llm-backend.md) and
[ADR-021](../../spec/adr/ADR-021-nl-query-layer.md).

**Prerequisite:** P8-S6 complete (all ✅). Observable must already run NLQ via OpenAI API
before this slice is started.

**Tech Stack:** Rust, `async-openai`, PostgreSQL (`sqlx`), React 19, TypeScript, Vitest,
React Testing Library, Testcontainers (for config endpoint integration test).

---

## Scope

In scope:
- `VllmCaller` implementing the `LlmCaller` trait (no auth, configurable base URL + model)
- New `PUT /v1/config/llm-backend` endpoint + extended `GET /v1/config` response
- Setup page backend selector: OpenAI API | vLLM (local) with model dropdown + URL field
- Testcontainers integration test for the new config endpoint
- Unit tests for `VllmCaller` factory logic and config helpers

Out of scope:
- Running or bundling a vLLM server (user provides the endpoint)
- Adding vLLM to `docker-compose.yml` (resource requirements are environment-specific)
- Arbitrary custom model strings in the Setup UI (env var escape hatch suffices)
- Ollama or other local inference backends (follow-on slice)
- PromQL façade changes (P8-S7 is independent)

---

## File Structure

### Backend
- Modify: `services/query-api/src/llm_adapter.rs` — add `VllmCaller`, `LlmBackendConfig`; update `handle_nlq_query` factory
- Modify: `services/query-api/src/config.rs` — extend `ConfigStatus`; add `put_llm_backend` handler and `fetch_llm_backend_config` helper
- Modify: `services/query-api/src/main.rs` — register `PUT /v1/config/llm-backend` route
- Modify: `services/query-api/tests/config_integration.rs` (or create) — Testcontainers integration test for new endpoint

### Frontend
- Modify: `apps/frontend/src/api/setup.ts` — extend `PlatformConfig`; add `saveLlmBackend()`
- Modify: `apps/frontend/src/pages/SetupPage.tsx` — replace `LlmKeyPanel` with `LlmConfigPanel`
- Modify: `apps/frontend/src/pages/SetupPage.test.tsx` — update mocks; add vLLM tests

---

## Closure Steps (strictly ordered)

### Step 1: Add VllmCaller and LlmBackendConfig to llm_adapter.rs

**Files:**
- Modify: `services/query-api/src/llm_adapter.rs`

- [ ] **1.1 Add `LlmBackend` enum and `LlmBackendConfig` struct**

  ```rust
  #[derive(Debug, Clone, PartialEq)]
  pub enum LlmBackend { OpenAi, Vllm }

  pub struct LlmBackendConfig {
      pub backend: LlmBackend,
      pub model: String,      // e.g. "microsoft/Phi-3-mini-4k-instruct"
      pub vllm_base_url: String, // e.g. "http://localhost:8000"
  }
  ```

  Model mapping (config value → vLLM model string):

  | Config value | vLLM model string |
  |---|---|
  | `phi3-mini` (default) | `microsoft/Phi-3-mini-4k-instruct` |
  | `llama3-8b` | `meta-llama/Meta-Llama-3-8B-Instruct` |

- [ ] **1.2 Add `VllmCaller` struct**

  Implement `LlmCaller` for `VllmCaller`. Uses `async-openai` with:
  - `api_base` = `vllm_base_url`
  - `api_key` = empty string (vLLM does not require auth by default)
  - `model` = resolved model string from `LlmBackendConfig.model`

  Prompt construction is identical to `OpenAiLlmCaller`; no new prompt logic.

- [ ] **1.3 Add `build_caller_from_config` factory function**

  Signature: `pub fn build_caller_from_config(config: &LlmBackendConfig, api_key: Option<&str>) -> Box<dyn LlmCaller>`

  - `LlmBackend::OpenAi` → `OpenAiLlmCaller::from_key(api_key.unwrap())`
  - `LlmBackend::Vllm` → `VllmCaller::new(config)`

- [ ] **1.4 Update `handle_nlq_query` to use the factory**

  Replace the current inline `OpenAiLlmCaller::from_key(key)` construction with a call to
  `build_caller_from_config`. The backend config is resolved from env vars (priority 1) or
  fetched from the DB (priority 2). `AppState` is extended with
  `llm_backend_config: Option<LlmBackendConfig>` loaded at startup from env vars.

  Env var precedence:
  - `LLM_BACKEND` (`openai` | `vllm`) — overrides DB
  - `LLM_MODEL` — overrides DB `llm_model` key
  - `VLLM_BASE_URL` — overrides DB `vllm_base_url` key

- [ ] **1.5 Add unit tests**

  - `build_caller_from_config` returns correct variant for each `LlmBackend` value
  - `VllmCaller` sends no `Authorization` header (verify via mock HTTP)
  - `handle_nlq_query` returns 503 with clear message when `LlmBackend::Vllm` is selected but
    vLLM endpoint is unreachable

---

### Step 2: Extend config.rs with llm-backend endpoint

**Files:**
- Modify: `services/query-api/src/config.rs`
- Modify: `services/query-api/src/main.rs`

- [ ] **2.1 Extend `ConfigStatus`**

  Add fields:
  ```rust
  pub llm_backend: Option<String>,  // "openai" | "vllm"
  pub llm_model: Option<String>,    // "phi3-mini" | "llama3-8b"
  ```

  `get_config` reads `llm_backend` and `llm_model` from env vars (if set) or DB and
  includes them in the response.

- [ ] **2.2 Add `fetch_llm_backend_config` helper**

  Reads `llm_backend`, `llm_model`, `vllm_base_url` from `platform_config` table.
  Returns a `LlmBackendConfig` with defaults applied:
  - `llm_backend`: `openai`
  - `llm_model`: `phi3-mini`
  - `vllm_base_url`: `http://localhost:8000`

- [ ] **2.3 Add `SetLlmBackendRequest` DTO and `put_llm_backend` handler**

  ```rust
  #[derive(Deserialize)]
  pub struct SetLlmBackendRequest {
      pub backend: String,           // "openai" | "vllm"
      pub model: String,             // "phi3-mini" | "llama3-8b"
      pub vllm_base_url: Option<String>,
  }
  ```

  `PUT /v1/config/llm-backend` upserts `llm_backend`, `llm_model`, and `vllm_base_url`
  (defaulting to `http://localhost:8000` if absent) into `platform_config`.

  Returns `422 Unprocessable Entity` for unknown backend or model values.

- [ ] **2.4 Register route in main.rs**

  ```rust
  .route("/v1/config/llm-backend", axum::routing::put(config::put_llm_backend))
  ```

---

### Step 3: Testcontainers integration test for config endpoint

**Files:**
- Create or modify: `services/query-api/tests/config_integration.rs`

- [ ] **3.1 Write Testcontainers test**

  ```rust
  #[tokio::test]
  async fn put_llm_backend_stores_vllm_config_and_get_config_reflects_it() {
      // Start PostgreSQL container, apply migrations.
      // Start the query-api in-process test server with the container pool.
      // PUT /v1/config/llm-backend with { backend: "vllm", model: "phi3-mini" }.
      // GET /v1/config → assert llm_backend == "vllm" && llm_model == "phi3-mini".
  }
  ```

  Uses the same Testcontainers helper pattern established in P8-S6.

- [ ] **3.2 Run test**

  ```bash
  cargo test -p query-api --test config_integration -- --nocapture
  ```

---

### Step 4: Extend setup.ts API client

**Files:**
- Modify: `apps/frontend/src/api/setup.ts`

- [ ] **4.1 Extend `PlatformConfig`**

  ```typescript
  export interface PlatformConfig {
    llm_key_configured: boolean;
    llm_backend: "openai" | "vllm" | null;
    llm_model: "phi3-mini" | "llama3-8b" | null;
  }
  ```

- [ ] **4.2 Add `saveLlmBackend` function**

  ```typescript
  export async function saveLlmBackend(
    backend: "openai" | "vllm",
    model: "phi3-mini" | "llama3-8b",
    vllm_base_url?: string,
  ): Promise<void>
  ```

  `PUT /v1/config/llm-backend` with JSON body. Throws on non-2xx response.

---

### Step 5: Replace LlmKeyPanel with LlmConfigPanel in SetupPage.tsx

**Files:**
- Modify: `apps/frontend/src/pages/SetupPage.tsx`

- [ ] **5.1 Add backend selector**

  Two-option radio group or segmented control: "OpenAI API" | "vLLM (local)".
  Controlled by local state `backend: "openai" | "vllm"`, initialised from `config.llm_backend`
  (defaulting to `"openai"`).

- [ ] **5.2 OpenAI mode panel**

  Identical to the existing `LlmKeyPanel` — API key input + Save button. No behaviour change.

- [ ] **5.3 vLLM mode panel**

  - Model dropdown (`<select>`) with options:
    - `phi3-mini` → "Phi-3 Mini 3.8B (default)" (selected by default)
    - `llama3-8b` → "Llama-3 8B Instruct"
  - Endpoint URL text input pre-filled with `http://localhost:8000`
    (overridable; stored as `vllm_base_url`).
  - Save button calls `saveLlmBackend(backend, model, vllm_base_url)`.
  - Informational note: "vLLM must be running separately at the configured endpoint."

- [ ] **5.4 Badge update**

  Badge on the panel header reflects the currently configured backend:
  - `"openai"` → "OpenAI API" (tone: good if key configured, warn if not)
  - `"vllm"` → "vLLM (local)" (tone: good)
  - `null` / default → "Not configured" (tone: warn)

---

### Step 6: Update SetupPage.test.tsx

**Files:**
- Modify: `apps/frontend/src/pages/SetupPage.test.tsx`

- [ ] **6.1 Update `getConfig` mock type**

  Add `llm_backend` and `llm_model` fields to all mock return values.

- [ ] **6.2 New test: backend selector defaults to OpenAI when config is null**

  `getConfig` returns `{ llm_key_configured: false, llm_backend: null, llm_model: null }`.
  Assert "OpenAI API" selector is checked.

- [ ] **6.3 New test: backend selector reflects existing vLLM config**

  `getConfig` returns `{ llm_key_configured: false, llm_backend: "vllm", llm_model: "phi3-mini" }`.
  Assert "vLLM (local)" selector is checked and model dropdown shows "Phi-3 Mini 3.8B".

- [ ] **6.4 New test: switching to vLLM shows model dropdown**

  Click "vLLM (local)" selector → assert model dropdown and endpoint URL input are visible.

- [ ] **6.5 New test: saving vLLM config calls saveLlmBackend**

  Select vLLM mode, choose Llama-3 8B, set URL to `http://gpu-box:8000`, submit.
  Assert `mockSaveLlmBackend` called with `("vllm", "llama3-8b", "http://gpu-box:8000")`.

---

## Checkpoint / Exit Criteria

- [ ] `cargo test -p query-api` passes (unit + integration)
- [ ] `npm test` in `apps/frontend` passes (all SetupPage tests including new vLLM tests)
- [ ] `bash scripts/local-ci.sh --skip-docker` passes
- [ ] `GET /v1/config` response includes `llm_backend` and `llm_model` fields
- [ ] `PUT /v1/config/llm-backend` with `{ backend: "vllm", model: "phi3-mini" }` returns 204
- [ ] Setup page shows backend selector; vLLM mode shows model dropdown + endpoint URL
- [ ] NLQ panel on ServiceDetailPage works against a locally running vLLM instance (manual smoke)
- [ ] Provenance payload still present on all NLQ responses (both backends)
- [ ] `PUT /v1/config/llm-backend` with unknown backend/model returns 422

## Not Addressed in This Slice

- vLLM authentication (bearer token) — use env-var `LLM_BACKEND=openai` + `OPENAI_BASE_URL` for
  auth-gated vLLM; native auth support is a follow-on.
- Ollama or other OpenAI-compat servers — same `LlmCaller` trait extension point applies.
- Streaming NLQ responses — out of scope for NLQ in general (follow-on).
