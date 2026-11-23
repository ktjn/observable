# AI service observability

Read contract of `observe-ai-services` (OBS-35-01..08), provided by observable-query in `services/query-api/src/ai_observability.rs`. All routes need an API key or session and read only the caller's tenant.

## Telemetry contract

Observable reads OpenTelemetry GenAI attributes and adds `observable.ai.*` attributes. Emit them on spans, span events and logs.

| Where | Attribute | Used for |
| --- | --- | --- |
| span | `gen_ai.operation.name` (`chat`, `execute_tool`, `invoke_agent`, ...) | marks AI work; required |
| span | `gen_ai.request.model`, `gen_ai.tool.name`, `gen_ai.agent.name` | changes, reconstruction |
| span | `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens` | consumption |
| span | `gen_ai.input.messages`, `gen_ai.output.messages` | reconstruction (scrubbed at normalization) |
| span | `observable.ai.feature`, `observable.ai.feature.owner` | attribution and owner |
| span | `observable.ai.usage.cost` | usage-based cost, in the unit the service reports |
| span | `observable.ai.instruction.version` | instruction and configuration changes |
| span | `observable.ai.input.topic` | input drift |
| span | `observable.ai.rationale`, `observable.ai.context.sources` | why an agent acted |
| span event `gen_ai.evaluation.result` | `gen_ai.evaluation.name`, `gen_ai.evaluation.score.value`, `gen_ai.evaluation.score.label`, `gen_ai.evaluation.explanation`, `observable.ai.evaluator` | quality, goal outcome (`name = goal_outcome`, label `succeeded`/`achieved` or `failed`/`not_achieved`), evaluator provenance |
| span event `observable.ai.policy_violation` | `observable.ai.policy.kind`, `observable.ai.safety_probe.id` | policy violations; a probe id marks a controlled test |
| log | `observable.ai.feedback.sentiment`, `trace_id` | end-user feedback linked to its interaction |

## Routes

Windows are RFC 3339, half-open `[from, to)`. Comparisons take `baseline_from`, `baseline_to`, `from`, `to`. A reversed window is `400`.

| Route | Requirement | Answer |
| --- | --- | --- |
| `GET /v1/ai/services/{service}/quality?evaluation=&max_drop=` | OBS-35-01 | mean score per window, drop, `degraded`/`stable`/`unknown` |
| `GET /v1/ai/services/{service}/consumption?spike_multiplier=` | OBS-35-02 | per feature and owner: calls, failed calls, p95, tokens, baseline tokens, cost, `spike`/`normal`/`no_baseline` |
| `GET /v1/ai/services/{service}/changes?evaluation=&from=&to=` | OBS-35-03 | model and instruction revisions in first-seen order, each change with quality before, after and delta |
| `GET /v1/ai/interactions/{trace_id}` | OBS-35-04, OBS-35-08 | ordered steps with AI attributes only, tool-call counts, goal outcome and its evaluator, evaluations |
| `GET /v1/ai/services/{service}/policy-violations?from=&to=` | OBS-35-05 | violations with kind, probe id, owner and `flagged_to_owner`; `unowned` count |
| `GET /v1/ai/services/{service}/input-drift?threshold=` | OBS-35-06 | topic shares per window, shift, `review_required`; `unknown` without both windows |
| `GET /v1/ai/services/{service}/feedback?from=&to=&sentiment=` | OBS-35-07 | feedback with its interaction trace and whether that interaction is retained |

## Rules

- Missing evidence is `unknown` or `null`, never `0` or `stable`.
- The goal outcome comes only from a `goal_outcome` evaluation. Successful tool calls never imply success.
- An unknown trace, another tenant's trace and a trace without AI spans all answer `404`.
- Only `gen_ai.*` and `observable.ai.*` attributes are returned from interaction steps.

## Not covered

- Notifying an owner (paging or email) is alerting's job. These routes expose `flagged_to_owner` for an alert rule or UI to act on.
- Retention follows the span (14 days) and log (60 days) TTLs.
