# W6 operational assistance: proposed specification and verification

**FEATURE.md Phase 1 draft. Owner decisions and a revised machine-checkable specification are pending.** Baseline `481133f4`. The proposed scope covers 39 requirements and 68 criteria in the v2.7.3 business baseline. Areas 18 and 38 target `assist-operations` in query. OBS-19-07 targets the platform. Priorities remain P1 (31) and P2 (8). The six tests in `services/query-api/src/ops_assistant.rs` supply historical unit evidence. The main query router does not mount that module.

## Decisions for the requester

| Decision | Proposal for approval | Input still needed |
| --- | --- | --- |
| D1. Ownership | Query owns a dedicated PostgreSQL schema and login for `assist-investigation` and `assist-action-audit`. Existing telemetry reads continue during a W6 database outage. Update ADR-035, the component specification and `docs/component-decomposition.md` together. | Approve query ownership or name an alternative provider. |
| Eligible operations | Authorize automatic bounded evidence reads through a versioned, tenant-approved policy. Require per-proposal human approval for every remediation under ADR-014. Bind approval to tenant, environment, action, parameters, exact target and proposal revision. Dispatch only a named provider operation. | Name the initial ordinary operational action, its provider, designated approver and choice of suspension authority. If that action is below the approved high-impact classification, select a separate, controlled high-impact case for OBS-18-04 and OBS-38-05/21. |
| Investigation policy | Use a versioned tenant policy for eligible incidents, authorized scope, elapsed-time and cost limits, retry limit and overlapping-event correlation. Missing applicable policy blocks automatic evidence gathering. Missing per-proposal approval blocks that remediation. | Identify the policy owner and approve the exercise profile, cost unit, eligibility rule and configured limits. Test values have no production authority. |
| Recovery and review | Judge recovery from independent, fresh observations after dispatch. Record recovered, failed and unknown. Join decisions to adjudication by a named incident authority. | Name the signal and source, observation window, freshness, required sample coverage and pass rule, and confirm who adjudicates incidents. |
| Platform onboarding | Use the existing onboarding wizard as a candidate entry point. It currently detects a tenant’s first signal. Specify and verify service-specific prerequisite diagnosis, correction and completion for the selected service. | Identify the approved scenario, its per-service prerequisite sources and provider APIs, and its trained-owner verifier. |

## Contracts and handoffs

1. **Specification and domain allocation.** Investigation owns the worker, HTTP surface and W6 data. Identity owns credential validation and purpose-based decisions. Incident response owns incident facts, corrections and adjudication. Service assessment and telemetry storage supply their authoritative observations where selected. Operational experience owns affected browser interactions and the platform onboarding exercise. Assign an accountable owner per affected domain before implementation. Review service-model or control dependencies if the approved scenario needs their coverage or policy records. Query currently declares 11 dependencies at a budget of 11. Declare and justify each additional dependency, including direct calls to `decide-access`. Name the owning provider, operation, caller identity, grants and compatibility range for incident reads, trigger consumption, adjudication, recovery and purge in the approval baseline. The current `evaluate-alerts` dependency alone conveys no incident-read permission. Preserve the direction from investigation to incident response under ADR-042.
2. **Ownership and worker.** Propose a numbered migration, schema-scoped grants, deployment configuration, tenant purge coordination and additive rollback in the contract. Use the query login for W6 writes. A tenant-scoped worker reads authoritative eligible incident changes with a durable checkpoint and reconciles gaps. Alerting’s `alerting.state.v1` is best effort. Verify a missed signal, duplicate delivery, overlapping events, concurrent workers and restart against actual provider state. Commit the incident link, trigger references and budget/scope snapshot durably. Historical evidence belongs to its tenant.
3. **HTTP and access.** Proposed under `/v1/assist`: investigation list/detail and event-backed creation, pause/resume/cancel/takeover, evidence summary and hypotheses, proposal submission and approve/execute, audit and review. Define operations for recording an accountable decision, disabling an approved action path and querying onboarding prerequisites in the provider contract that owns them. Record the exact operation names, payloads and failure mapping in `contracts/openapi/query-v1.yaml` or the owning provider contract before implementation. Derive tenant and principal from the validated credential. Use a separately governed, tenant-scoped worker or delegated identity with an explicit purpose and approved read set. Check its effective grant at each provider, including withdrawal or expiry. Human authorization for an interactive request conveys no background worker permission. Check purpose, action, service and record scope on detail, lists, aggregations and worker dispatch. A denial is indistinguishable from absence for restricted reads. Reject malformed requests, stale revisions and invalid transitions before side effects. Unavailable auth, audit persistence and execution policy fail closed.
4. **Execution and recovery.** Persist execution intent and audit before dispatch, use a stable idempotency key, then record provider receipt and independently observed outcome. Reconcile a crash between dispatch and receipt without repeating an effect. Check cancellation, suspension, approval revision and authorization at dispatch. Keep in-flight work identifiable and reconcile its actual outcome after cancellation. Verify with the approved provider’s operation and its recovery observation, including stale, unavailable and degraded signals.
5. **User verification.** Exercise UX-11 investigation review and UX-12 remediation approval and verification. Check affected UX-14 governed agent-access interactions for OBS-18-07/10. Preserve context, expose pending/error/unauthorized/inconclusive states and test keyboard access. Run the approved onboarding scenario through the trained owner’s normal entry point.

## Approved acceptance cases and risks

Run the eight applicable cases from [v2.7 acceptance tests](../../spec/business/verification/acceptance-tests.yaml) and their linked [UX scenarios](../../spec/business/experience/ux-workflows.yaml). Record provider and browser results under the original case and scenario IDs.

| Case | Linked UX scenarios | Required observation |
| --- | --- | --- |
| V27-T01 | ux-11-open-investigation-success / failure | One bounded case retains both related event references. A replay of the same identifier adds neither a second investigation nor a duplicate reference. |
| V27-T02 | ux-11-inspect-hypotheses-success / failure | Competing explanations retain counterevidence and remain unconfirmed when unsupported. |
| V27-T03 | ux-11-pause-resume-success / failure | Cancel while an action is pending. Case evidence survives and no later privileged dispatch occurs. |
| V27-T04 | ux-12-execute-success / failure | Exhaust budget, deny a tool permission, attempt the same operation through another tool and inspect both recorded limits. |
| V27-T05 | ux-12-approve-success / failure | Approve for service A and attempt service B. No effect occurs on B. |
| V27-T06 | ux-12-verify-success / failure and ux-12-inspect-outcome-failure | A successful command with independently degraded service health is reported failed. |
| V27-T07 | No UX scenario is declared. | The reviewer finds an accepted, adjudicated-incorrect recommendation counted as inaccurate. |
| V27-T18 | ux-14-request-elevation-success / failure | Calling another tool after denial blocks the same operation and records the denial. |

Trace risk coverage in the [business risk register](../../spec/business/risks.yaml).

| Risk | W6 verification |
| --- | --- |
| obs-risk-unauthorized-disclosure and obs-risk-sensitive-context-disclosure | Permission and purpose checks at assistant input, citations, actions and aggregate reads, including the governed-agent integration where used. |
| obs-risk-unapproved-automated-remediation | Designated approval, target binding, delegated-identity grants and the alternate-tool denials in V27-T04/T05/T18. |
| obs-risk-false-recovery-assertion | Independent degraded, absent and stale evidence in V27-T06 and OBS-38-23/24. |
| obs-risk-unsupported-ai-diagnosis | Counterevidence and adjudicated outcomes in V27-T02/T07. |
| obs-risk-unbounded-investigation-resource-use | V27-T03/T04, persisted budgets and bounded retries after restart. |

## Criterion verification matrix

Each row refers to the stable `acceptance` criterion and, where present, the stable `negative` criterion of the named requirement. A dash denotes a requirement with only the acceptance criterion. Use mounted HTTP routes or a running worker with the real PostgreSQL schema and ClickHouse or a real provider at each relevant boundary. Browser checks supplement those cases for user-facing obligations.

| Requirement | Acceptance test intent | Negative test intent |
| --- | --- | --- |
| OBS-18-01 | Request a plain-language incident summary as a responder and follow its citations to the seeded observations. | — |
| OBS-18-02 | Seed a material anomaly and negligible variation in the same window. Verify their relative prominence. | — |
| OBS-18-03 | Display a suggested action alongside a verified cause. Verify distinct labels, uncertainty and evidence. | — |
| OBS-18-04 | Submit a high-impact proposal without designated approval. Observe a blocked execution and zero provider calls. | — |
| OBS-18-05 | Review accepted, rejected and adjudicated-inaccurate recommendations as the owner. | — |
| OBS-18-06 | Disable an automation path as its authorized operator during an exercise. Verify subsequent dispatch is blocked. | — |
| OBS-18-07 | Seed restricted evidence. As an ungranted user and as a principal with a grant for a different purpose, inspect actual assistant inputs before summarization and the returned text, citations and counts. None may contain the restricted observation. | — |
| OBS-18-08 | Accept a recommendation. Read the incident review and verify the original output and authenticated decision maker. | — |
| OBS-18-09 | Attempt a controlled action on a service outside the declared scope. Verify the denial and zero dispatches. | — |
| OBS-18-10 | Inspect the plan lifecycle and dispatch history. Verify reads stayed within the approved policy and only individually approved remediation steps ran. | Request the same denied operation through another registered tool and verify denial and zero effects. |
| OBS-19-07 | A trained owner completes the approved monitoring onboarding scenario, identifies its seeded missing prerequisite and verifies completed coverage after correction. | — |
| OBS-38-01 | Deliver two related, distinct event IDs for one qualifying incident through its provider. Observe one bounded worker-created investigation retaining both references (V27-T01). | Replay an identical event ID concurrently and after restart. Verify one stored reference per distinct event and one investigation. |
| OBS-38-02 | Compare deployment and database explanations against cited seeded observations. | Seed counterevidence and read the conclusion. Verify it is present. |
| OBS-38-03 | Pause and resume as responder. Compare evidence IDs and scope across transitions. | Cancel then attempt the next privileged step. Observe zero dispatches. |
| OBS-38-04 | Exhaust the approved budget. Read incomplete status and the stop reason. | Request a denied operation using a different tool identity. Observe denial. |
| OBS-38-05 | Use the approved high-impact case. Block dispatch before approval and after a different actor approves. Dispatch only after the designated approver authorizes the exact proposal. | Reuse the approval for another target. Observe zero effects on that target. |
| OBS-38-06 | Return success from the approved operation while independent service data remains degraded. Read failed outcome. | Withhold post-action telemetry and verify unknown outcome. |
| OBS-38-07 | Compare decisions with adjudicated incidents in the owner’s review. | Accept an incorrect recommendation and verify it contributes no successful result. |
| OBS-38-08 | Match a configured eligibility rule and inspect its recorded identity in the new investigation. | Submit a nonmatching event and verify no investigation is created. |
| OBS-38-09 | Deliver two events for one incident and inspect both links on a single investigation. | Replay either event and verify the investigation count remains one. |
| OBS-38-10 | Read the incident reference and each contributing trigger reference after worker processing. | Attempt creation without a trigger reference and verify no record is stored. |
| OBS-38-11 | Seed deployment and database evidence. Read both evaluated hypotheses for one incident. | Leave a competing explanation unrefuted. Verify the result does not present a sole cause. |
| OBS-38-12 | Read a hypothesis and resolve supporting and contradicting citations. | Retrieve its summary and verify that the contradicting observation remains visible. |
| OBS-38-13 | Inspect confidence and an identified missing observation. | Add unresolved counterevidence and verify the hypothesis cannot be confirmed. |
| OBS-38-14 | Resume a paused investigation and compare persisted evidence and scope. | Interrupt and recreate the worker with the same database. Verify collected evidence survives. |
| OBS-38-15 | Cancel and read the retained investigation evidence and timeline. | Race a queued step with cancellation. Verify no new privileged dispatch after cancellation takes effect. |
| OBS-38-16 | Take over as an authorized responder and inspect scope and evidence. | Attempt control as an unauthorized caller. Verify unchanged state. |
| OBS-38-17 | Reach configured elapsed-time and cost limits separately. Verify incomplete status and the stopping limit. | Submit the next step after either limit and verify zero provider calls. |
| OBS-38-18 | Request an out-of-grant read and verify that the denials ledger records the decision. | Request the same underlying permission through a second tool and verify continued denial. |
| OBS-38-19 | Fail a bounded provider read repeatedly and count exactly the permitted attempts. | Restart or redeliver failed work. Verify no further retries after the limit. |
| OBS-38-20 | Read a proposal and verify its expected effect, blast radius and prerequisites. | Omit each mandatory field in turn and verify submission for approval fails. |
| OBS-38-21 | Use the approved high-impact case. Attempt execution without approval, with another actor’s approval, and with the designated approver’s approval. Only the last dispatches. | Change target on the approved action. Verify the provider sees no call. |
| OBS-38-22 | Inspect the recorded reversal or mitigation path before dispatch. | Remove the recovery route and verify execution is blocked. |
| OBS-38-23 | Observe the approved independent recovery criteria passing within their window. Verify recovered status. | Return a successful provider receipt alone. Verify recovery remains unconfirmed. |
| OBS-38-24 | Pair a successful operation with degraded post-action evidence. Verify failed status. | Withhold post-action observations. Verify unknown status. |
| OBS-38-25 | Compare the persisted approving principal to the authenticated designated approver who actually approved. Match action, target and outcome to the provider dispatch and independent observation. | Fail the durable audit write before dispatch. Verify zero side effects. |
| OBS-38-26 | Join investigation conclusions to adjudicated incident outcomes. Verify the matching cases in review. | Include an unadjudicated case. Verify it is excluded from the correct tally. |
| OBS-38-27 | Seed false positives and denied or unsafe attempts. Verify counts, denominators and rates. | Mark an accepted recommendation incorrect in adjudication. Verify it contributes no success. |
| OBS-38-28 | Read accuracy, cost and duration for multiple periods as an owner. | Change the evaluation method between periods and verify the comparison flags the change. |

## Exit criteria

- **Approval.** Resolve the decisions above. Publish the capability operations, failure modes, data ownership, domain handoffs, risk dispositions and verification cases against one revision. Run `--impact assist-operations`, inspect affected query/web evidence and obtain a valid compatibility comparison to main. Requester approves the complete baseline before implementation.
- **Implementation.** Follow TDD with the above positive and negative cases. A focused suite under `services/query-api/tests/it/`, registered from `main.rs`, uses the mounted HTTP paths and PostgreSQL/ClickHouse Testcontainers. At each new or changed internal provider boundary, supply a versioned programmable mock and run the same contract suite against the real provider and its mock. Exercise the provider independently, verify the consumer with the mock, then join the real provider and consumer and run the end-to-end scenarios. Record the independent provider, consumer and integration results against their respective obligations. Verify tenant-role grants, purge, interrupted/retried work, auth failure and isolation. Independently isolate the W6 database connection while other services remain live. Before, during and after the outage, query a known ClickHouse record through the existing API and require successful status, the expected result and healthy core query readiness. During the outage, exercise a W6 write and dispatch, require the documented unavailable result, zero provider calls and no cross-owner SQL fallback, and verify recovery of persisted state on reconnection. Inspect runtime grants/access traces for zero cross-owner SQL. Execute affected browser scenarios and record the trained-owner result for OBS-19-07.
- **Evidence.** For every passing criterion, record the exact criterion, requirement version, test path and independent verifier in `spec/system.yaml` and `docs/plans/business-spec-gap.yaml`. Retain historical provenance when superseding old evidence. Remove unreachable helpers only after checking callers. Retain verified status only when every criterion has replacement integration evidence. Otherwise mark the affected requirement specified, record its open criteria and update its gap-map entry under the wiring plan. Verify component scope, boundary checks, spec gate, affected tests and full local CI before code push. The acceptance record links all eight applicable V27 reports and their declared UX scenario results, independent verification and the requester’s final decision.
