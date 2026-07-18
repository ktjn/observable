# ADR-035: WebLLM as a Client-Side NLQ Provider

**Date:** 2026-07-18
**Status:** Accepted
**Authors:** ktjn
**Deciders:** Project Stakeholders
**Review date:** 2027-01-18

## Context

ADR-027 established a single LLM connection model for NLQ (ADR-021): any
OpenAI-compatible HTTP endpoint (OpenAI, Azure, vLLM, Ollama), configured via three
fields (API key, endpoint URL, model) and unified behind one `OpenAiLlmCaller`. That
ADR explicitly rejected a provider-selector concept, reasoning that every supported
backend speaks the same HTTP protocol, so a mode selector would add UI complexity
without adding capability.

WebLLM (`@mlc-ai/web-llm`) runs LLM inference entirely inside the browser via WebGPU.
It is not an HTTP endpoint reachable from the server at all — there is no URL to
point at, no API key, and the model executes on the user's own GPU. This breaks the
assumption ADR-027's rejection rested on: the "any provider is just a URL" argument
only holds for providers that are, in fact, reachable over HTTP from the query-api
process. WebLLM is a different kind of provider along a different axis (server-side
network call vs. client-side local computation), not a variant of the same kind
ADR-027 already covers.

The motivation for adding it is privacy: some users want to run NLQ without the
question text or the schema context embedded in the system prompt (tenant metric and
label names) ever leaving their browser to any third-party or remote LLM. WebLLM
satisfies that by construction — inference happens locally, on-device.

## Decision

Observable now supports two NLQ providers, selectable per tenant, coexisting side by
side (WebLLM does not replace the remote path):

| Setting | `platform_config` key | Env var | Default |
|---|---|---|---|
| Provider | `llm_provider` | `LLM_PROVIDER` | `"remote"` |
| WebLLM model | `webllm_model` | — | _(none)_ |

`llm_provider` accepts `"remote"` or `"webllm"` (case-insensitive, trimmed); any
other value is treated as unset and falls back to `"remote"` rather than erroring —
this is a deliberate fail-safe so a malformed config value can never silently break
NLQ for existing deployments. The existing `llm_url`/`llm_model`/`llm_api_key` fields
keep their exact ADR-027 semantics unchanged for the Remote provider. `webllm_model`
is a separate key, not a reuse of `llm_model` — WebLLM model identifiers (drawn from
`@mlc-ai/web-llm`'s `prebuiltAppConfig.model_list`) are a disjoint namespace from
OpenAI-compatible model strings, and overloading the field would risk a stale WebLLM
model id leaking into a remote call if a tenant switches providers back.

### Two-phase request architecture

`POST /v1/nlq` remains the single-call path for the Remote provider, byte-identical
to ADR-021/ADR-027 behavior. It is not reachable for WebLLM tenants — since there is
no server-side model to call, `POST /v1/nlq` returns `503 Service Unavailable` for a
tenant configured as `Webllm`, directing the (correctly written) client to the
two-phase endpoints instead. This 503 is a server-side guard against a stale or
misrouted client, not a path any correctly written frontend should hit in normal
operation.

WebLLM tenants instead use:
- `POST /v1/nlq/prepare` — runs the deny gate, schema-context fetch, and system
  prompt construction server-side (identical logic to the Remote path, factored out
  into `prepare_nlq_pipeline`), then returns the system prompt and question to the
  browser **without ever calling an LLM**. The browser runs WebLLM locally against
  this exact prompt.
- `POST /v1/nlq/complete` — takes the raw completion the browser produced and
  resumes the pipeline (parsing, the repair sub-loop, service-scope enforcement,
  metric resolution, query execution) using the same `resume_nlq_pipeline` logic the
  Remote path's internal loop already used.

Both endpoints reuse the pre-LLM shortcuts (empty-question page-load execution,
raw-IR paste, `/`-shorthand bypass) and the deny gate identically to `POST /v1/nlq`,
via a shared helper — behavior does not diverge between providers for anything that
isn't the LLM call itself.

### Repair-loop trust boundary

The repair sub-loop (`MAX_REPAIR_ATTEMPTS = 1`, ADR-021) must stay server-enforced
even though "the LLM" is now client code the server cannot trust. `/prepare` issues
an opaque `session_token` backed by a server-side, in-memory (not persisted to
Postgres — see Implementation Note below), TTL-bounded session that stores the
original question, request, and a repair-attempt counter the server alone
increments. `/complete` validates the token's tenant ownership and TTL on every
call; an unknown, expired, or wrong-tenant token is rejected outright. A client
cannot extend its own repair budget, replay a stale prompt, or spoof another
tenant's session.

### WebGPU availability is not guaranteed

The server cannot know ahead of time whether a client's browser/GPU supports
WebGPU. The capability check happens client-side, before WebLLM is offered as usable.
If a tenant is configured for WebLLM but the current browser lacks WebGPU support,
the frontend fails closed: a clear error is shown, and there is **no silent fallback**
to the remote provider (which would defeat the privacy guarantee this feature exists
to provide) and **no silent fallback** to the ADR-034 shorthand parser without
explaining why. The user must either switch browsers/devices or reconfigure the
provider themselves.

### Model weights are a distinct kind of network egress

WebLLM's first use of a model downloads its weights (hundreds of MB to several GB)
from the model's CDN. This is a genuine network request the privacy-conscious user
should be aware of, but it is categorically different from the privacy guarantee this
ADR makes: the guarantee is that **query data** (the question text and the system
prompt's embedded schema/metric names) never leaves the browser once WebLLM is
selected. Model weight downloads are static assets, not tenant data, and are cached
by the browser after the first fetch. The Setup page's WebLLM copy calls this
distinction out explicitly so it isn't mistaken for a gap in the privacy claim.

## Implementation note: in-memory session store, not a new table

The plan that preceded this ADR considered a Postgres-backed session table. The
actual implementation uses an in-memory, per-process store instead
(`Arc<Mutex<HashMap<Uuid, NlqPipelineSession>>>` in query-api's `AppState`). Reasons:
this repo's backend CI test job runs without a live Postgres instance, so a
DB-backed session store would have been difficult to unit-test without new
integration-test infrastructure; and the session is inherently short-lived,
single-process-scoped correlation data (a 10-minute-TTL token pairing a
`/prepare` call with its `/complete` call), not data that needs to survive a
process restart or be shared across replicas — consistent with this whole LLM
configuration surface already being framed by ADR-027 as local-development-targeted
rather than high-availability production infrastructure. If Observable's query-api
is ever deployed with multiple replicas behind a load balancer without session
affinity, a `/prepare` and its matching `/complete` could land on different
processes and the token would not resolve — this is an accepted limitation for the
same reason ADR-027 accepted XOR "obfuscation" instead of real secret encryption:
proportionate to the feature's current scope, not a production-hardening guarantee.

## Non-goals

- No server-side model hosting or GPU infrastructure is added by this ADR.
- No change to the deny-gate, service-scope enforcement, or advisory-only/
  provenance-required invariants (ADR-014, ADR-021) — these are provider-agnostic
  and apply identically to both providers.
- No change to the Remote provider's configuration semantics, connectivity probe
  (`POST /v1/config/llm/models`), or `POST /v1/nlq` behavior for Remote tenants.

## Consequences

**Easier:**
- Users who want NLQ without any tenant data reaching a remote LLM (including
  Observable's own configured OpenAI-compatible endpoint) now have a path that
  requires no API key, no external account, and no network egress of query content.
- The Remote provider's existing configuration, behavior, and tests are completely
  unaffected — this is purely additive.

**Harder:**
- Two providers now means two things to reason about when debugging NLQ issues;
  the shared pre-LLM-shortcut/deny-gate/prompt-build code path keeps this bounded,
  but the two-phase HTTP contract for WebLLM is a second flow to understand.
- First-use model downloads (multi-GB) are a real UX cost the Remote provider
  doesn't have — mitigated with explicit Setup-page copy, not solved.

**Constrained:**
- The in-memory session store does not survive a query-api restart or work across
  multiple replicas without session affinity — acceptable for this feature's current
  local-development-targeted scope (see Implementation Note), but would need
  revisiting (e.g. a shared session store) before this could be considered
  production-hardened for a multi-replica deployment.
- WebGPU support varies by browser/device/OS; this ADR does not attempt to work
  around browsers that don't support it — it fails closed instead.

## Related

- [ADR-027](ADR-027-local-llm-backend.md) — Remote provider configuration; superseded
  in part (a provider concept now exists), amended rather than replaced (its
  three-field remote configuration model is unchanged).
- [ADR-021](ADR-021-nl-query-layer.md) — NL query layer architecture; `LlmCaller`
  trait, the pipeline this ADR splits into `prepare`/`resume` phases.
- [ADR-014](ADR-014-ai-feature-boundaries.md) — AI feature boundaries (advisory-only,
  provenance required) — unchanged, provider-agnostic.
- [ADR-034](ADR-034-simple-ir-shorthand.md) — deterministic shorthand fallback
  when no LLM is configured (code comments in `llm_adapter.rs` still cite the old
  "ADR-029" number from before a 0.1-release renumbering — see that file's own
  header note); precedent for graceful degradation, but explicitly not used as a
  fallback when WebLLM is configured but unavailable (see WebGPU availability
  section above).
