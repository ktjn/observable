from __future__ import annotations
import argparse, ast, hashlib, json, os, re, subprocess, sys, tomllib, yaml
import xml.etree.ElementTree as ET
from collections import Counter
from datetime import date, datetime
from pathlib import Path, PurePosixPath, PureWindowsPath
from typing import Literal
from pydantic import BaseModel, ConfigDict, Field, ValidationError, create_model

RANK = {"public": 0, "internal": 1, "confidential": 2, "restricted": 3}
STATUS = ["draft", "specified", "verified", "deprecated"]
STAGES = ["inventory", "discovery", "architecture", "verification", "release"]

# Invariant ID: (stage, waiver kind, rule); mirrored in spec.md § 11. COMPAT rules are checked by --compat.
INVARIANTS = {
    "REF-01": ("inventory", None, "IDs are unique within each entity type."),
    "REF-02": ("inventory", None, "Requirement and risk targets resolve to exactly one system, component, capability or data model."),
    "REF-03": ("inventory", None, "Provided capabilities exist."),
    "IMP-01": ("inventory", None, "Imports have unique IDs; qualified capability references resolve to a pinned import and are provided only by external components."),
    "REF-04": ("inventory", None, "Owned data models exist."),
    "REF-05": ("inventory", None, "Bindings and via entries name required capabilities."),
    "REF-06": ("inventory", None, "Requirement relationships reference existing requirements."),
    "REF-07": ("inventory", None, "Evidence references an existing requirement."),
    "REF-08": ("inventory", None, "Evidence subjects exist."),
    "REF-09": ("inventory", None, "Risk mitigations reference existing requirements."),
    "REF-10": ("inventory", None, "Data model owners are existing components."),
    "REF-11": ("inventory", None, "Lineage references existing data models."),
    "REF-12": ("inventory", None, "Declassification references an existing requirement."),
    "REF-13": ("inventory", None, "Interfaces reference existing data models."),
    "REF-14": ("inventory", None, "Subject bindings name existing subjects and non-empty source revisions."),
    "DATA-01": ("inventory", None, "External data sources are registered."),
    "DATA-02": ("architecture", None, "Owners list their data models in owns_data."),
    "DATA-03": ("architecture", "shared-data-ownership", "Each data model has exactly one owning component."),
    "DATA-04": ("architecture", None, "Derived data keeps its sources' classification unless declassified by an active requirement."),
    "DATA-05": ("architecture", None, "Data classifications are known before architecture acceptance."),
    "DATA-06": ("inventory", None, "Unknown classifications name an active review requirement; declared reviews resolve."),
    "EV-01": ("inventory", None, "Model-check evidence states its bounds."),
    "EV-02": ("inventory", None, "Superseding evidence passes and supersedes failed evidence of the same requirement."),
    "EV-03": ("inventory", None, "Supersession states a reason."),
    "EV-04": ("release", None, "Release evidence matches the subject version and any declared source binding, with timestamp and verifier."),
    "EV-05": ("release", None, "Release evidence has no skipped checks."),
    "EV-06": ("inventory", None, "Evidence criteria name criteria of the verified requirement."),
    "EV-07": ("inventory", None, "Evidence methods are allowed for the requirement's holder when system.evidence_methods declares it."),
    "EV-08": ("verification", None, "With system.evidence_policy.require_criteria, every evidence record names the criteria it covers."),
    "RISK-01": ("inventory", None, "Waivers require mitigation or acceptance, mitigating requirements and an expiry."),
    "RISK-02": ("inventory", None, "Waivers have not expired."),
    "RISK-03": ("release", None, "Mitigated and avoided risks name mitigating requirements."),
    "RISK-04": ("release", None, "Mitigating requirements are verified."),
    "RISK-05": ("release", None, "Avoided risks carry no residual risk."),
    "RISK-06": ("release", None, "Transferred risks name the receiving party."),
    "RISK-07": ("release", None, "Accepted and transferred risks record residual acceptance."),
    "RISK-08": ("release", None, "No waivers remain."),
    "STAT-01": ("discovery", None, "Capabilities are at least specified."),
    "STAT-02": ("architecture", None, "Components are at least specified."),
    "STAT-03": ("verification", None, "Active requirements are at least specified."),
    "STAT-04": ("release", None, "System, capabilities, components and active requirements are verified or deprecated."),
    "CAP-01": ("discovery", None, "Capabilities define preconditions or postconditions, invariants and failure modes."),
    "CAP-02": ("architecture", None, "Capabilities required across components declare an interface."),
    "CAP-03": ("architecture", None, "Interfaces define operation, idempotent, auth and map errors to failure modes."),
    "CAP-04": ("verification", None, "Active capabilities have requirements."),
    "COMP-01": ("architecture", None, "Components provide at least one capability."),
    "COMP-02": ("architecture", None, "Component layers are declared."),
    "COMP-03": ("architecture", "context-budget", "Components fit the context budget."),
    "COMP-04": ("architecture", None, "Required capabilities resolve to exactly one compatible provider."),
    "COMP-05": ("architecture", "layer-violation", "Components depend only on the same or lower layers."),
    "COMP-06": ("architecture", "dependency-cycle", "Component dependencies are acyclic."),
    "COMP-07": ("architecture", None, "Active components do not require deprecated capabilities."),
    "COMP-08": ("verification", None, "Test evidence for a component that declares test_paths comes from files or selectors under them."),
    "COMP-09": ("verification", None, "A unit-scoped component that declares test_paths has a fake under them (file or selector) for each required capability."),
    "COMP-10": ("release", None, "A capability that a consumer fakes has a verified contract requirement."),
    "COMP-11": ("architecture", None, "External components declare no layer, domain, requires, test_paths or fakes."),
    "DOM-01": ("architecture", None, "Component domains and domain uses are declared."),
    "DOM-02": ("architecture", "domain-violation", "Cross-domain dependencies are declared in Domain.uses."),
    "DOM-03": ("architecture", "dependency-cycle", "Domain uses are acyclic."),
    "REQ-01": ("verification", "verification-category", "Active requirements cover all required verification categories (all ten unless `system.verification_categories` narrows them)."),
    "REQ-02": ("verification", None, "Requirement dependencies and refinements are acyclic."),
    "REQ-03": ("verification", None, "Requirement conflicts are resolved by an active requirement."),
    "REQ-04": ("release", None, "Obligated requirements have no unsuperseded failing evidence for their version."),
    "REQ-05": ("release", None, "Obligated (mandatory, declassifying, resolving) requirements have passing evidence for their version."),
    "REQ-06": ("release", None, "Dependencies of obligated and mitigating requirements are verified."),
    "REQ-07": ("verification", None, "Requirement kind matches its verification category."),
    "REQ-08": ("release", None, "Obligation passes to refining requirements, and obligated or mitigating requirements are refined only by verified requirements."),
    "REQ-09": ("inventory", None, "`superseded_by` is set only on deprecated requirements and names an active requirement."),
    "REQ-10": ("inventory", None, "Criterion IDs are unique within a requirement."),
    "EXP-01": ("inventory", None, "Experience, view, interaction and scenario IDs are unique."),
    "EXP-02": ("inventory", None, "Experience references to requirements, capabilities, views, entries, interactions, state sets and view states resolve, and each interaction has exactly one source."),
    "EXP-03": ("architecture", None, "Interactions have an observable outcome and a trigger offered by their source view."),
    "EXP-04": ("architecture", None, "View states state their handling, and interaction failure modes map to a state of the source or target view."),
    "EXP-05": ("verification", None, "Critical experiences exercise every interaction in a scenario, and every interaction with failure modes in a negative scenario."),
    "EXP-06": ("inventory", None, "Normative visual references declare a version and an acceptance authority."),
    "EXP-07": ("release", None, "Requirements verifying scenarios and constraints of critical experiences have passing evidence."),
    "EXP-08": ("verification", None, "Exhaustive critical experiences exercise every realized requirement in a scenario or give a reason in not_exercised."),
    "EXP-09": ("verification", None, "Every action of a view used by an exhaustive critical experience triggers an interaction."),
    "EXP-10": ("verification", None, "Every view-specific state of a view used by an exhaustive critical experience is reached by a scenario."),
    "EXP-11": ("verification", None, "In critical experiences, each failure mode's state is reached by a negative scenario of that interaction."),
    "EXP-12": ("verification", None, "In critical experiences, a negative scenario exercises exactly one interaction, and a scenario's states belong to the views its interactions connect."),
    "COMPAT-01": (None, None, "Preconditions are not strengthened."),
    "COMPAT-02": (None, None, "Postconditions are not removed."),
    "COMPAT-03": (None, None, "Invariants are not removed."),
    "COMPAT-04": (None, None, "Failure modes are not added."),
    "COMPAT-05": (None, None, "Data model fields are not removed."),
    "COMPAT-06": (None, None, "Data model field types are unchanged."),
    "COMPAT-07": (None, None, "Data model classification is unchanged."),
    "COMPAT-08": (None, None, "Breaking changes bump the major version."),
    "COMPAT-09": (None, None, "Capabilities are deprecated before removal."),
}
CATEGORIES = ["functional", "semantic", "contract", "serialization", "concurrency",
              "performance", "scalability", "resilience", "security", "observability"]
Status = Literal["draft", "specified", "verified", "deprecated"]

class M(BaseModel):
    model_config = ConfigDict(extra="forbid")

def F(d: str, default=...): return Field(default, description=d)
ID, VER, ST = F("Stable identity, unique per entity type (§ 11)."), F("Dot-separated integer version (§ 10)."), F("Lifecycle status (§ 10).", "draft")

class Budget(M):
    """Context budget limits per component (§ 3)."""
    max_provides: int = F("Max provided capabilities.", 5)
    max_requires: int = F("Max required capabilities.", 8)
    max_owns_data: int = F("Max owned data models.", 4)

class Domain(M):
    """Cohesive group of components (bounded context), orthogonal to layers (§ 2)."""
    id: str = ID
    description: str = F("Purpose and boundary.", "")
    uses: list[str] = F("Domains this domain's components may depend on (§ 2).", [])

class Refinement(M):
    """Declared deviation of this spec from a baseline import (§ 12)."""
    path: str = F("Changed or removed baseline element, e.g. 'requirements/R-1/criteria' or 'experiences/UX-1/scenarios/s1'; covers everything below it.")
    reason: str = Field(min_length=1, description="Why the baseline element changes.")
    decision: str = Field(min_length=1, description="Decision record, review or ADR approving the change.")

class SpecImport(M):
    """Pinned reference to another system's specification (§ 2): its capabilities are referenced as '<id>:<capability>' instead of copied, or (mode baseline) its requirements, risks and experiences are adopted with declared deviations."""
    id: str = ID
    path: str = F("Path to the imported spec, relative to this spec's directory.")
    version: str = F("Imported system.version this pin was taken from.")
    capabilities: dict[str, str] = F("Referenced capability ID -> pinned version.", {})
    sha256: str | None = F("Optional SHA-256 of the imported spec file bytes.", None)
    link: str | None = F("URL of the repository that owns the imported spec; no revision (the pin is version + sha256). Used for clone hints and --workspace link agreement (§ 12).", None)
    mode: Literal["capabilities", "baseline"] = F("capabilities: reference capabilities; baseline: this spec adopts the imported requirements, risks, views, experiences and state sets, and --check-imports reports undeclared changes or removals (§ 12).", "capabilities")
    artifacts: dict[str, str] = F("Baseline files relative to the imported spec's directory -> SHA-256 of their exact bytes (--check-imports).", {})
    allow_fields: dict[Literal["requirements", "risks", "views", "experiences"], list[str]] = F("Fields that may differ from the baseline per kind, e.g. requirements: [target, version] when retargeted.", {})
    id_prefix: dict[Literal["requirements", "risks"], str] = F("Prefix this spec adds to baseline IDs per kind, e.g. risks: 'up-'.", {})
    refinements: list[Refinement] = F("Declared changes or removals of baseline content; additions need none.", [])

class Member(M):
    """Spec checked out in a workspace (§ 12)."""
    id: str = ID
    path: str = F("Member spec file, relative to the workspace file.")
    link: str | None = F("URL of the repository that owns the member spec.", None)
    stage: Literal[tuple(STAGES)] | None = F("Optional per-member validation stage override for mixed-maturity workspaces (§ 12).", None)

class Workspace(M):
    """Super-repo manifest: independently owned specs checked out side by side (§ 12)."""
    id: str = ID
    members: list[Member] = F("Member specs.", [])

Method = Literal["test", "proof", "model-check", "analysis", "inspection"]

class EvidencePolicy(M):
    """Project evidence policy enforced by the validator (§ 8); evidence sufficiency remains project policy."""
    require_criteria: bool = F("Every evidence record names the criteria it covers; a record without criteria no longer covers the whole requirement (EV-08).", False)

class SystemDef(M):
    """System: composition of components (§ 2)."""
    id: str = ID
    version: str = VER
    status: Status = ST
    layers: list[str] = F("Layers, highest first; dependencies point downward (§ 2).", [])
    domains: list[Domain] = F("Domains; unordered, each may span layers (§ 2).", [])
    external_sources: list[str] = F("Registered external data sources (§ 5).", [])
    imports: list[SpecImport] = F("Pinned references to other systems' specs; their capabilities are used as '<id>:<capability>' (§ 2).", [])
    exports: list[str] | None = F("Capability IDs exposed to importing or parent specs; None exports all (§ 2).", None)
    quality_attributes: dict[str, str] = F("Measurable quality attributes (§ 9).", {})
    verification_categories: list[Literal[tuple(CATEGORIES)]] | None = Field(None, min_length=1, description="Verification categories REQ-01 requires; default all 10. Omitted categories are reported (§ 11).")
    context_budget: Budget = F("Component size limits (§ 3).", Budget())
    evidence_policy: EvidencePolicy = F("Optional evidence policy (EV-08) (§ 8).", EvidencePolicy())
    evidence_methods: dict[str, list[Method]] = F("Requirement holder -> allowed evidence methods (EV-07); holders not listed are unrestricted (§ 8).", {})
    release_scope: list[str] = F("Priorities in scope at release; other active requirements are deferred, not passed (§ 11).", [])

class Interface(M):
    """Interface contract of an exposed capability (§ 4)."""
    operation: str = F("Operation identifier or endpoint, e.g. 'POST /v1/orders'.", "")
    mode: Literal["sync", "async"] = F("Interaction mode.", "sync")
    request: str | list[str] | None = F("Request DataModel ID(s).", None)
    response: str | list[str] | None = F("Response DataModel ID(s).", None)
    idempotent: bool | None = F("Whether repeated calls have the same effect.", None)
    idempotency_strategy: Literal["token", "natural_key", "replacing_merge_tree", "outbox"] | None = F("Concrete idempotency mechanism for storage or async writes.", None)
    auth: str = F("Authentication/authorization requirement.", "")
    errors: dict[str, int | str] = F("Failure mode -> status code or event; keys equal failure_modes.", {})

class Capability(M):
    """Implementation-independent functionality with a contract (§ 4)."""
    id: str = ID
    version: str = VER
    status: Status = ST
    description: str = F("Human-readable purpose.", "")
    preconditions: list[str] = F("Contract preconditions.", [])
    postconditions: list[str] = F("Contract postconditions.", [])
    invariants: list[str] = F("Contract invariants.", [])
    failure_modes: list[str] = F("Named failure modes.", [])
    interface: Interface | None = F("Interface contract; required when another component requires this capability.", None)

class MockBinding(M):
    """Provider-owned, programmable substitute for one capability."""
    path: str = F("Repository-relative mock adapter path.")
    contract_suite: list[str] = F("Argv executed in real and mock modes against the same contract.")
    scenarios: list[str] = F("Scenarios supported by the programmable mock (reviewed by its suite).", [])

class Component(M):
    """Independent logical unit providing capabilities (§ 3)."""
    id: str = ID
    version: str = VER
    status: Status = ST
    description: str = F("Human-readable purpose.", "")
    owner: str | None = F("Accountable role or team (a reference; not validated) (§ 3).", None)
    layer: str | None = F("One of system.layers.", None)
    domain: str | None = F("One of system.domains.", None)
    consistency: Literal["strong", "eventual"] = F("Consistency guarantee.", "strong")
    provides: list[str] = F("Provided capability IDs.", [])
    requires: list[str] = F("Required capability IDs or 'id@^version' (§ 10).", [])
    test_requires: list[str] = F("Capabilities used only by this component's tests (integration scope); resolved like requires, exempt from layer, cycle and domain rules, context slices and fakes (§ 3).", [])
    owns_data: list[str] = F("Owned DataModel IDs.", [])
    bindings: dict[str, str] = F("Required capability ID -> provider component ID.", {})
    source_paths: list[str] = F("Implementation directories or modules (§ 3).", [])
    fakes: dict[str, str] = F("Required capability ID -> test double under test_paths that stands in for its provider (§ 3).", {})
    provided_mocks: dict[str, MockBinding] = F("Provided capability ID -> programmable mock adapter and shared real/mock contract suite (optional isolation methodology).", {})
    standalone_harness: list[str] = F("Command argv to run this component with dependency substitutes, without real internal providers (optional isolation methodology).", [])
    test_paths: list[str] = F("Directories or files holding this component's own tests, or `tool::selector` entries (e.g. `nextest::-p crate`) for package-level runners; test evidence for it must come from here (§ 3).", [])
    context_budget: dict[Literal["max_provides", "max_requires", "max_owns_data"], int] = F("Per-component override of system.context_budget limits (§ 3).", {})
    external: bool = F("Provider outside this system (platform, third-party API): provides only; exempt from layers, domains, budgets and tests; a drift target whose own sources are not scanned (§ 3).", False)
    realized_by: str | None = F("Import ID of the child subsystem specification realizing this component (§ 3).", None)
    via: dict[str, str] = F("Required capability ID -> required capability mediating it at runtime (e.g. a transaction platform); no direct code dependency expected (§ 3).", {})
    test_scope: Literal["unit", "integration"] = F("unit: tests use fakes only; integration: tests may also use providers of required capabilities (§ 3).", "unit")

class DataModel(M):
    """Owned data structure with lineage and classification (§ 5)."""
    id: str = ID
    version: str = VER
    owner: str = F("Owning component ID; must list this model in owns_data.")
    classification: Literal["unknown", "public", "internal", "confidential", "restricted"] = F("Classification; unknown requires review and is inventory-only.", "unknown")
    classification_review: str | None = F("Active requirement resolving an unknown/provisional classification.", None)
    namespace: str | None = F("Physical schema or database namespace prefix (e.g. 'auth', 'control').", None)
    storage_role: str | None = F("Dedicated database/storage role with access permissions (e.g. 'observable_auth_role').", None)
    fields: dict[str, str] = F("Field name -> type description.")
    derived_from: list[str] = F("Source DataModel IDs (lineage).", [])
    external_source: str | None = F("Entry in system.external_sources.", None)
    schema_path: str | None = F("Physical schema artifact (§ 5).", None)
    declassified_by: str | None = F("Verified requirement authorizing lower classification than sources.", None)

class Criterion(M):
    """Acceptance criterion with a stable ID; evidence may name it by ID or text (§ 6)."""
    id: str = F("Stable criterion ID, unique within the requirement (REQ-10).")
    text: str = F("The criterion.")

class Requirement(M):
    """Expected behavior or constraint with acceptance criteria (§ 6)."""
    id: str = ID
    version: str = VER
    target: str = F("System, component, capability or data model ID.")
    status: Status = F("Lifecycle status; 'deprecated' requirements are inactive (§ 6).", "draft")
    statement: str | None = F("The obligation itself; criteria then hold only its acceptance checks (§ 6).", None)
    priority: str | None = F("Project-defined phase or priority, e.g. P0; independent of mandatory and without effect on stage invariants (§ 6).", None)
    owner: str | None = F("Accountable role or approver (a reference; not validated) (§ 6).", None)
    holder: str | None = F("Who carries the obligation, e.g. platform, organization or process; system.evidence_methods may restrict evidence methods per holder (EV-07) (§ 6).", None)
    superseded_by: str | None = F("Active requirement that replaces this deprecated one (REQ-09) (§ 6).", None)
    kind: Literal["functional", "non-functional"] = F("Property kind.", "functional")
    category: Literal[tuple(CATEGORIES)] | None = F("Verification category; the project's required categories must be covered (§ 11).", None)
    mandatory: bool = F("Mandatory requirements need evidence at release.", True)
    criteria: list[str | Criterion] = F("Acceptance criteria: text, or {id, text} so evidence can bind to a stable ID (§ 6).")
    depends_on: list[str] = F("Prerequisite requirement IDs.", [])
    refines: list[str] = F("Requirement IDs whose obligations this elaborates; refinement is co-obligation (REQ-08), so link across priority or phase with depends_on (§ 6).", [])
    conflicts_with: list[str] = F("Incompatible requirement IDs.", [])
    resolved_by: str | None = F("Active requirement resolving this requirement's conflicts.", None)

    @property
    def crit(self) -> list[str]:
        """Canonical criterion keys: the ID where declared, else the text."""
        return [c.id if isinstance(c, Criterion) else c for c in self.criteria]

    def ckey(self, name: str) -> str | None:
        """Canonical key for a criterion named by ID or text; None when unknown."""
        for c in self.criteria:
            if name in ((c.id, c.text) if isinstance(c, Criterion) else (c,)): return c.id if isinstance(c, Criterion) else c
        return None

class Risk(M):
    """Risk or tracked architectural exception (§ 7)."""
    id: str = ID
    target: str = F("System, component, capability or data model ID.")
    description: str = F("Human-readable description of the risk.", "")
    owner: str | None = F("Accountable role or approver (a reference; not validated) (§ 7).", None)
    treatment: Literal["mitigation", "acceptance", "avoidance", "transfer"] = F("Risk treatment.")
    mitigated_by: list[str] = F("Mitigating or remediation requirement IDs; need evidence at release.", [])
    residual_accepted: bool = F("Residual risk explicitly accepted.", False)
    transferred_to: str | None = F("Counterparty for 'transfer'.", None)
    waives: Literal["layer-violation", "dependency-cycle", "shared-data-ownership", "context-budget", "verification-category",
                    "domain-violation",
                    "DATA-03", "COMP-03", "COMP-05", "COMP-06", "DOM-02", "DOM-03", "REQ-01"] | None = F("Waived violation kind or invariant ID (below release).", None)
    expires: date | Literal["architecture", "verification", "release"] | None = F("Waiver expiry: ISO date or stage.", None)

class Evidence(M):
    """Verification result traceable to a requirement version (§ 8)."""
    id: str = ID
    requirement_id: str = F("Verified requirement ID.")
    requirement_version: str = F("Verified requirement version.")
    method: Literal["test", "proof", "model-check", "analysis", "inspection"] = F("Verification method.")
    passed: bool = F("Result.")
    provenance: str = F("Artifact or tool reference, e.g. 'pytest::test_x'.")
    bounds: dict[str, int | str] | str | None = F("Bounds/scope; required for model-check.", None)
    subject_id: str | None = F("Verified subject ID; defaults to Requirement.target when omitted.", None)
    subject_revision: str | None = F("Subject version or immutable revision.", None)
    subject_version: str | None = F("Contract version when subject_revision is bound to an implementation revision.", None)
    artifacts: dict[str, str] = F("Repository-relative evidence file paths mapped to SHA-256 digests of exact bytes.", {})
    skipped: list[str] = F("Skipped check names/reasons; partial evidence cannot verify a requirement.", [])
    timestamp: datetime | date | None = F("Execution time.", None)
    verifier: str | None = F("Verifier or tool identity.", None)
    environment: str | None = F("Where the evidence was produced (host, OS or CI job), so local and remote runs can be told apart.", None)
    criteria: list[str] = F("Requirement criteria this record covers, by criterion ID or exact text; empty covers the whole requirement unless system.evidence_policy.require_criteria (EV-08). All criteria must be covered by passing records to verify a requirement.", [])
    supersedes: str | None = F("Failed evidence ID this record supersedes.", None)
    supersession_reason: str | None = F("Required with supersedes.", None)

class SubjectBinding(M):
    """Reviewed contract/source binding; optional file checks establish artifact integrity."""
    version: str = VER
    revision: str = F("Reviewed source revision; distinct from contract version.")
    artifacts: dict[str, str] = Field(description="Repository-relative source files mapped to exact-byte SHA-256 digests.", min_length=1)

class VisualReference(M):
    """Reference artifact for a view or experience; only a normative one constrains presentation (§ 13)."""
    path: str = F("Repository-relative artifact path.")
    authority: Literal["illustrative", "normative"] = F("illustrative: guidance for agents, never a pass/fail criterion; normative: presentation must conform, as accepted by accepted_by.", "illustrative")
    version: str | None = F("Reference version; required when normative.", None)
    accepted_by: str | None = F("Role that accepts conformance to the reference; required when normative.", None)
    sha256: str | None = F("Optional SHA-256 of the artifact bytes (--check-artifacts).", None)
    consistent_with: list[str] = F("Requirement IDs the reference must not contradict; the reference stays illustrative and a review may reject it where it does (EXP-02 resolves the IDs).", [])

class View(M):
    """Presentation context (screen, page, CLI output, conversational turn), independent of any UI technology (§ 13)."""
    id: str = ID
    purpose: str = F("What the view is for.", "")
    content: list[str] = F("Information hierarchy and content obligations, most important first.", [])
    actions: list[str] = F("Triggers the view offers; an interaction leaving the view names one.", [])
    states: dict[str, str] = F("State name -> required handling (e.g. empty: explain why and offer a next step); scoped to this view and overriding state_sets.", {})
    state_sets: list[str] = F("Names of Spec.state_sets whose states this view includes.", [])
    references: list[VisualReference] = F("Visual references for this view.", [])

class Interaction(M):
    """Transition between views of an experience (§ 13)."""
    id: str = ID
    from_view: str | None = F("Source view ID; exactly one of from_view and from_entry. Source and target may be the same view for an action inside it.", None)
    from_entry: str | None = F("Source entry point declared in Experience.entries (notification, deep link, command); exactly one of from_view and from_entry.", None)
    to_view: str = F("Target view ID.")
    trigger: str = F("Action of the source view, or the event at the source entry, that starts the interaction.")
    preconditions: list[str] = F("Conditions that must hold.", [])
    preserves: list[str] = F("Context carried to the target view (e.g. time-range, filters).", [])
    outcome: str = F("Observable result; required (EXP-03).", "")
    invariants: list[str] = F("Conditions that hold throughout.", [])
    failure_modes: dict[str, str] = F("Failure mode -> state of the source or target view that handles it (EXP-04).", {})

class Scenario(M):
    """Verification scenario that exercises interactions and states of its experience; verified through an existing requirement (§ 13)."""
    id: str = ID
    requirement: str | None = F("Requirement ID carrying the verification obligation; shorthand for a one-element requirements.", None)
    requirements: list[str] = F("Requirement IDs whose Evidence verifies the scenario; all must pass (EXP-07).", [])
    interactions: list[str] = F("Interaction IDs exercised.", [])
    states: list[str] = F("'view.state' entries exercised.", [])
    kind: Literal["positive", "negative"] = F("negative: exercises failure modes, denial or boundary behavior.", "positive")

class Constraint(M):
    """Design constraint with the requirement that verifies it (§ 13)."""
    value: str = F("Standard or value, e.g. WCAG-2.2-AA.")
    requirement: str | None = F("Requirement ID that verifies the constraint (EXP-02, EXP-07).", None)

class Experience(M):
    """Optional user-facing workflow or goal; orthogonal to components, layers and domains (§ 13)."""
    id: str = ID
    description: str = F("Goal of the workflow.", "")
    critical: bool = F("Critical workflows need verification scenarios (EXP-05, EXP-07).", False)
    requirements: list[str] = F("Requirement IDs the experience realizes.", [])
    capabilities: list[str] = F("Capability IDs the experience realizes.", [])
    views: list[str] = F("IDs of the Spec.views taking part.", [])
    entries: dict[str, str] = F("Entry points that are not views: name -> description (notification, deep link, command, conversational trigger).", {})
    interactions: list[Interaction] = F("Transitions between its views.", [])
    constraints: dict[str, str | Constraint] = F("Named design constraints: a value, or a Constraint naming the requirement that verifies it.", {})
    exhaustive: bool = F("With critical: every realized requirement is the target of a scenario or listed in not_exercised (EXP-08).", False)
    not_exercised: dict[str, str] = F("Realized requirement ID -> reason it has no scenario (EXP-08).", {})
    references: list[VisualReference] = F("Visual references for the whole experience.", [])
    scenarios: list[Scenario] = F("Verification scenarios.", [])

class Spec(M):
    """System specification document (spec.md)."""
    system: SystemDef
    capabilities: list[Capability]
    components: list[Component]
    data_models: list[DataModel] = []
    requirements: list[Requirement] = []
    risks: list[Risk] = []
    evidence: list[Evidence] = []
    subject_bindings: dict[str, SubjectBinding] = F("Subject ID -> reviewed contract version, source revision and artifact hashes.", {})
    state_sets: dict[str, dict[str, str]] = F("Reusable view states: set name -> state name -> required handling; views include them with state_sets (§ 13).", {})
    views: list[View] = F("Optional experience views (§ 13).", [])
    experiences: list[Experience] = F("Optional experience specification (§ 13).", [])
    include: list[str] = F("Fragment files merged on load (globs relative to this file, no '..'); consumed on load, so a resolved spec has none (spec.md § 12).", [])

# A spec fragment holds any Spec section except `system` and `include`, so teams or agents can own separate files.
SpecFragment = create_model("SpecFragment", __base__=M, __doc__="Spec fragment merged into the root spec by `include` (spec.md § 12).",
                            **{k: (f.annotation | None, Field(None, description=f.description)) for k, f in Spec.model_fields.items() if k not in ("system", "include")})

def _ver(v: str) -> tuple[int, ...] | None:
    try: return tuple(int(x) for x in v.split("."))
    except ValueError: return None

def _ver_ok(req: str | None, act: str) -> bool:
    if not req: return True
    r, a = _ver(req), _ver(act)
    return bool(r and a) and r[0] == a[0] and a[1:] >= r[1:]

def _import_cap(spec: Spec, ref: str) -> str | None:
    """Pinned version of an imported capability reference '<import>:<capability>', else None."""
    alias, _, cap = ref.partition(":")
    return next((i.capabilities.get(cap) for i in spec.system.imports if i.id == alias), None) if cap else None

def _resolve(spec: Spec, c: Component, ref: str) -> tuple[str | None, str | None]:
    k, _, v = ref.partition("@^")
    cap = next((x for x in spec.capabilities if x.id == k), None)
    ps = [p.id for p in spec.components if k in p.provides and p.id != c.id]
    if k in c.bindings: ps = [p for p in ps if p == c.bindings[k]]
    ver = cap.version if cap else _import_cap(spec, k)
    if ver is None or not _ver_ok(v, ver) or not ps: return None, f"unresolved capability '{ref}'"
    if len(ps) > 1: return None, f"ambiguous providers {ps} for '{ref}'; add a binding"
    return ps[0], None

def resolve_capabilities(spec: Spec) -> dict[str, dict[str, str]]:
    return {c.id: {r.partition("@^")[0]: p for r in c.requires if (p := _resolve(spec, c, r)[0])} for c in spec.components}

def _cycles(g: dict[str, list[str]]) -> list[list[str]]:
    seen, stack, out = set(), [], []
    def visit(n):
        if n in stack: out.append(stack[stack.index(n):] + [n]); return
        if n in seen: return
        seen.add(n); stack.append(n)
        for m in g.get(n, []): visit(m)
        stack.pop()
    for n in g: visit(n)
    return out

def _superseded(spec: Spec) -> set[str]:
    return {e.supersedes for e in spec.evidence if e.passed and e.supersedes and e.supersession_reason}

def _evidence_state(spec: Spec) -> dict[tuple[str, str], tuple[bool, list[str], list[str]]]:
    """(requirement, version) -> (verified, failing criteria, uncovered criteria) from non-superseded evidence; a record without criteria covers the whole requirement."""
    reqs, sup, recs = {(r.id, r.version): r for r in spec.requirements}, _superseded(spec), {}
    for e in spec.evidence:
        if e.id not in sup: recs.setdefault((e.requirement_id, e.requirement_version), []).append(e)
    out = {}
    for k, es in recs.items():
        r, good = reqs.get(k), lambda e: e.passed and not e.skipped
        crit, key = (r.crit, r.ckey) if r else ([], lambda c: c)
        failing = sorted({key(c) or c for e in es if not good(e) for c in (e.criteria or crit)})
        covered = {key(c) for e in es if good(e) for c in e.criteria}
        blanket = not spec.system.evidence_policy.require_criteria and any(good(e) and not e.criteria for e in es)
        uncovered = [] if blanket else [c for c in crit if c not in covered]
        out[k] = (all(good(e) for e in es) and not uncovered, failing, uncovered)
    return out

def verified_requirements(spec: Spec) -> set[tuple[str, str]]:
    return {k for k, (ok, _, _) in _evidence_state(spec).items() if ok}

def evidence_report(spec: Spec) -> list[dict]:
    """Per active requirement version: verification state from the validator's own evaluation, for project policy to build on."""
    state, sup = _evidence_state(spec), _superseded(spec)
    out = []
    for r in spec.requirements:
        if r.status == "deprecated": continue
        es = [e for e in spec.evidence if (e.requirement_id, e.requirement_version) == (r.id, r.version) and e.id not in sup]
        ok, failing, uncovered = state.get((r.id, r.version), (False, [], list(r.crit)))
        out.append({"requirement": r.id, "version": r.version, "status": r.status, "priority": r.priority, "holder": r.holder,
                    "verified": ok, "failing": failing, "uncovered": uncovered, "records": [e.id for e in es],
                    "blanket": [e.id for e in es if not e.criteria], "skipped": [e.id for e in es if e.skipped],
                    "methods": sorted({e.method for e in es})})
    return out

def deferred_scope(spec: Spec, scope: list[str] | None = None) -> dict[str, list[str]]:
    """Release scope by priority: active requirements with a priority outside scope, the entities they target and risks only they mitigate."""
    scope = spec.system.release_scope if scope is None else scope
    reqs = [r for r in spec.requirements if r.status != "deprecated"]
    deferred = sorted(r.id for r in reqs if scope and r.priority is not None and r.priority not in scope)
    if not deferred: return {"requirements": [], "entities": [], "risks": []}
    tgt = {r.target for r in reqs if r.id in deferred}
    ents = sorted(tgt | {c.id for c in spec.components if tgt & {*c.provides, *c.owns_data}} | {spec.system.id})
    risks = sorted(k.id for k in spec.risks if k.mitigated_by and set(k.mitigated_by) <= set(deferred))
    return {"requirements": deferred, "entities": ents, "risks": risks}

def _expired(k: Risk, stage: str, today: date | None) -> bool:
    return k.expires < (today or date.today()) if isinstance(k.expires, date) else STAGES.index(k.expires) < STAGES.index(stage)

def active_waivers(spec: Spec, stage: str = "release", today: date | None = None) -> list[Risk]:
    return [k for k in spec.risks if k.waives and k.treatment in ("mitigation", "acceptance") and k.mitigated_by and k.expires
            and stage != "release" and not _expired(k, stage, today)]

def _list(xs: str | list[str] | None) -> list[str]:
    return xs if isinstance(xs, list) else [xs] if xs else []

def validate_spec(spec: Spec, stage: str = "release", today: date | None = None, release_scope: list[str] | None = None) -> list[str]:
    E, resolved = [], resolve_capabilities(spec)
    def add(i, msg, *subj): E.append((STAGES.index(INVARIANTS[i][0]), f"[{i}] {msg}", {i, INVARIANTS[i][1]} - {None}, {*subj}))
    def need(i, lvl, groups):
        for kind, xs in groups:
            for x in xs:
                if STATUS.index(x.status) < STATUS.index(lvl): add(i, f"{kind} '{x.id}' status '{x.status}' below '{lvl}'")
    comps = {c.id: c for c in spec.components}
    caps = {c.id: c for c in spec.capabilities}
    reqs = {r.id: r for r in spec.requirements}
    active = {r.id: r for r in spec.requirements if r.status != "deprecated"}
    models = {d.id: d for d in spec.data_models}
    layers = {l: i for i, l in enumerate(spec.system.layers)}
    doms = {d.id: d for d in spec.system.domains}
    evs = {e.id: e for e in spec.evidence}
    sid, sup, verified = spec.system.id, _superseded(spec), verified_requirements(spec)
    ok = {r.id for r in active.values() if (r.id, r.version) in verified}
    state = _evidence_state(spec)
    dfr = deferred_scope(spec, release_scope)
    dreq, dent = set(dfr["requirements"]), set(dfr["entities"])
    failed = {(e.requirement_id, e.requirement_version) for e in spec.evidence if not e.passed and e.id not in sup}
    targets = {"system": {sid}, "component": set(comps), "capability": set(caps), "data model": set(models)}
    required = {r.partition("@^")[0] for c in spec.components for r in c.requires}
    vers = {x.id: x.version for x in [spec.system, *spec.capabilities, *spec.components, *spec.data_models]}

    # inventory: identity and references
    for kind, xs in [("capability", spec.capabilities), ("component", spec.components), ("data model", spec.data_models),
                     ("requirement", spec.requirements), ("risk", spec.risks), ("evidence", spec.evidence), ("domain", spec.system.domains)]:
        for i, n in Counter(x.id for x in xs).items():
            if n > 1: add("REF-01", f"Duplicate {kind} id '{i}'")
    for kind, xs in [("Requirement", spec.requirements), ("Risk", spec.risks)]:
        for x in xs:
            m = [t for t, ids in targets.items() if x.target in ids]
            if len(m) != 1: add("REF-02", f"{kind} '{x.id}' has {'ambiguous' if m else 'unknown'} target '{x.target}'")
    for p in spec.system.exports or ():
        if p not in caps: add("REF-03", f"System exports unknown capability '{p}'")
    for c in spec.components:
        for p in c.provides:
            if p not in caps and _import_cap(spec, p) is None: add("REF-03", f"Component '{c.id}' provides unknown capability '{p}'")
        for d in c.owns_data:
            if d not in models: add("REF-04", f"Component '{c.id}' owns unknown data model '{d}'")
        for k in c.bindings:
            if k not in {r.partition("@^")[0] for r in c.requires}: add("REF-05", f"Component '{c.id}' binds capability '{k}' it does not require")
        for k, v in c.via.items():
            if k == v or not {k, v} <= {r.partition("@^")[0] for r in c.requires}: add("REF-05", f"Component '{c.id}' via '{k}' -> '{v}' must map a required capability to another one")
    imps = {i.id for i in spec.system.imports}
    for i, n in Counter(i.id for i in spec.system.imports).items():
        if n > 1: add("IMP-01", f"Duplicate import id '{i}'")
    for c in spec.components:
        if c.realized_by and c.realized_by not in imps: add("IMP-01", f"Component '{c.id}' realized_by unknown import '{c.realized_by}'")
        for ref in [*c.provides, *(r.partition("@^")[0] for r in c.requires)]:
            if ":" in ref and ref not in caps and _import_cap(spec, ref) is None: add("IMP-01", f"Component '{c.id}' references '{ref}', which no import pins")
        for p in c.provides:
            if ":" in p and p not in caps and not c.external and _import_cap(spec, p) is not None: add("IMP-01", f"Component '{c.id}' provides imported capability '{p}' but is not external")
    for r in spec.requirements:
        for rel in ("depends_on", "refines", "conflicts_with", "resolved_by"):
            for x in _list(getattr(r, rel)):
                if ":" in x and rel in ("depends_on", "refines") and x.partition(":")[0] in imps and x.partition(":")[2]: continue
                if x not in reqs: add("REF-06", f"Requirement '{r.id}' {rel} unknown requirement '{x}'")
        if r.superseded_by and (r.status != "deprecated" or r.superseded_by not in active):
            add("REQ-09", f"Requirement '{r.id}' superseded_by '{r.superseded_by}' must name an active requirement and be set only on a deprecated one")
        if dup := sorted(i for i, n in Counter(c.id for c in r.criteria if isinstance(c, Criterion)).items() if n > 1):
            add("REQ-10", f"Requirement '{r.id}' has duplicate criterion IDs {dup}")
    for e in spec.evidence:
        if e.requirement_id not in reqs: add("REF-07", f"Evidence '{e.id}' references unknown requirement '{e.requirement_id}'")
        if e.criteria and e.requirement_id in reqs and (bad := [c for c in e.criteria if reqs[e.requirement_id].ckey(c) is None]):
            add("EV-06", f"Evidence '{e.id}' names criteria not in requirement '{e.requirement_id}': {bad}")
        if e.method == "model-check" and not e.bounds: add("EV-01", f"Evidence '{e.id}' model-check requires bounds")
        h = reqs[e.requirement_id].holder if e.requirement_id in reqs else None
        if h in spec.system.evidence_methods and e.method not in spec.system.evidence_methods[h]:
            add("EV-07", f"Evidence '{e.id}' method '{e.method}' is not allowed for holder '{h}' of '{e.requirement_id}'")
        if spec.system.evidence_policy.require_criteria and not e.criteria:
            add("EV-08", f"Evidence '{e.id}' names no criteria; the evidence policy requires them")
        if e.subject_id and not any(e.subject_id in ids for ids in targets.values()): add("REF-08", f"Evidence '{e.id}' has unknown subject '{e.subject_id}'")
        if e.supersedes:
            o = evs.get(e.supersedes)
            if not o or o.passed or o.requirement_id != e.requirement_id or not e.passed:
                add("EV-02", f"Evidence '{e.id}' must be passing and supersede a failed record of the same requirement")
            if not e.supersession_reason: add("EV-03", f"Evidence '{e.id}' supersedes without supersession_reason")
    for k in spec.risks:
        for m in k.mitigated_by:
            if m not in reqs: add("REF-09", f"Risk '{k.id}' references unknown requirement '{m}'")
        if k.waives and (k.treatment not in ("mitigation", "acceptance") or not k.mitigated_by or not k.expires):
            add("RISK-01", f"Risk '{k.id}' waiver requires mitigation or acceptance, mitigated_by and expires")
        elif k.waives and _expired(k, stage, today):
            add("RISK-02", f"Risk '{k.id}' waiver expired ({k.expires})")
    for dm in spec.data_models:
        if dm.owner not in comps: add("REF-10", f"DataModel '{dm.id}' has unknown owner '{dm.owner}'")
        if dm.external_source and dm.external_source not in spec.system.external_sources:
            add("DATA-01", f"DataModel '{dm.id}' has untracked external data source '{dm.external_source}'")
        for x in dm.derived_from:
            if x not in models: add("REF-11", f"DataModel '{dm.id}' derives from unknown data model '{x}'")
        if dm.declassified_by and dm.declassified_by not in reqs:
            add("REF-12", f"DataModel '{dm.id}' declassified_by unknown requirement '{dm.declassified_by}'")
    for cap in spec.capabilities:
        for xs in (cap.interface.request, cap.interface.response) if cap.interface else ():
            for x in _list(xs):
                if x not in models: add("REF-13", f"Capability '{cap.id}' interface references unknown data model '{x}'")
    for subj, binding in spec.subject_bindings.items():
        if sum(subj in ids for ids in targets.values()) != 1 or not binding.revision.strip():
            add("REF-14", f"Subject binding '{subj}' requires an existing unambiguous subject and source revision")
    for dm in spec.data_models:
        if (dm.classification == "unknown" or dm.classification_review) and dm.classification_review not in active:
            add("DATA-06", f"DataModel '{dm.id}' requires an active classification_review requirement")

    # discovery: capability contracts
    need("STAT-01", "specified", [("Capability", spec.capabilities)])
    for cap in spec.capabilities:
        if not ((cap.preconditions or cap.postconditions) and cap.invariants and cap.failure_modes):
            add("CAP-01", f"Capability '{cap.id}' has incomplete contract clauses")

    # architecture: composition, data, interfaces, budgets
    need("STAT-02", "specified", [("Component", spec.components)])
    b = spec.system.context_budget
    for c in spec.components:
        if not c.provides: add("COMP-01", f"Component '{c.id}' must provide at least one capability")
        if c.external:
            if bad := [f for f in ("layer", "domain", "requires", "test_requires", "test_paths", "fakes", "realized_by") if getattr(c, f)]: add("COMP-11", f"External component '{c.id}' declares {', '.join(bad)}")
            continue
        if layers and c.layer not in layers: add("COMP-02", f"Component '{c.id}' has undeclared layer '{c.layer}'")
        if (doms or c.domain) and c.domain not in doms: add("DOM-01", f"Component '{c.id}' has undeclared domain '{c.domain}'")
        for f in ("provides", "requires", "owns_data"):
            n = c.context_budget.get(f"max_{f}", getattr(b, f"max_{f}"))
            if len(getattr(c, f)) > n: add("COMP-03", f"Component '{c.id}' exceeds context budget: {f} > {n}", c.id)
        for ref in c.test_requires:
            if err := _resolve(spec, c, ref)[1]: add("COMP-04", f"Component '{c.id}' has test_requires {err}")
        for ref in c.requires:
            k = ref.partition("@^")[0]
            if c.status != "deprecated" and k in caps and caps[k].status == "deprecated":
                add("COMP-07", f"Component '{c.id}' requires deprecated capability '{k}'")
            p, err = _resolve(spec, c, ref)
            if err: add("COMP-04", f"Component '{c.id}' has {err}")
            elif c.layer in layers and comps[p].layer in layers and layers[c.layer] > layers[comps[p].layer]:
                add("COMP-05", f"Component '{c.id}' has layer violation depending on '{p}'", c.id, p)
            if not err and c.domain in doms and comps[p].domain in doms and comps[p].domain not in {c.domain, *doms[c.domain].uses}:
                add("DOM-02", f"Component '{c.id}' in domain '{c.domain}' depends on '{p}' in undeclared domain '{comps[p].domain}'", c.id, p)
    for cy in _cycles({k: list(v.values()) for k, v in resolved.items()}):
        add("COMP-06", f"Dependency cycle: {' -> '.join(cy)}", *cy)
    for d in spec.system.domains:
        for u in d.uses:
            if u not in doms: add("DOM-01", f"Domain '{d.id}' uses undeclared domain '{u}'")
    for cy in _cycles({d.id: [u for u in d.uses if u in doms] for d in spec.system.domains}):
        add("DOM-03", f"Domain cycle: {' -> '.join(cy)}", *(c.id for c in spec.components if c.domain in cy))
    for dm in spec.data_models:
        if dm.classification == "unknown": add("DATA-05", f"DataModel '{dm.id}' classification is unknown")
        if dm.owner in comps and dm.id not in comps[dm.owner].owns_data:
            add("DATA-02", f"DataModel '{dm.id}' owner '{dm.owner}' does not list it in owns_data")
        for c in spec.components:
            if dm.id in c.owns_data and c.id != dm.owner:
                add("DATA-03", f"DataModel '{dm.id}' is also claimed by '{c.id}'", dm.id, c.id, dm.owner)
        for x in dm.derived_from:
            if x in models and dm.classification in RANK and models[x].classification in RANK and dm.declassified_by not in active and RANK[dm.classification] < RANK[models[x].classification]:
                add("DATA-04", f"DataModel '{dm.id}' violates classification propagation from '{x}'")
    for cap in spec.capabilities:
        if cap.id in required and not cap.interface: add("CAP-02", f"Capability '{cap.id}' is required cross-component but has no interface")
        if cap.interface and (not cap.interface.operation.strip() or cap.interface.idempotent is None or not cap.interface.auth.strip() or set(cap.interface.errors) != set(cap.failure_modes)):
            add("CAP-03", f"Capability '{cap.id}' interface must define operation, idempotent, auth and map errors to failure_modes")

    # verification: requirements
    need("STAT-03", "specified", [("Requirement", active.values())])
    targeted = {r.target for r in active.values()}
    for cap in spec.capabilities:
        if cap.status != "deprecated" and cap.id not in targeted: add("CAP-04", f"Capability '{cap.id}' has no associated requirements")
    missing = [c for c in spec.system.verification_categories or CATEGORIES if c not in {r.category for r in active.values()}]
    if missing: add("REQ-01", f"Missing verification categories: {missing}", sid)
    for cy in _cycles({r.id: [*r.depends_on, *r.refines] for r in spec.requirements}):
        add("REQ-02", f"Requirement cycle: {' -> '.join(cy)}")
    for r in active.values():
        if r.category and (r.kind == "non-functional") != (r.category in CATEGORIES[5:]):
            add("REQ-07", f"Requirement '{r.id}' kind '{r.kind}' does not match category '{r.category}'")
        for x in r.conflicts_with:
            if x in active and r.resolved_by not in active and active[x].resolved_by not in active:
                add("REQ-03", f"Requirement '{r.id}' has unresolved conflict with '{x}'")

    for e in spec.evidence:
        if e.id in sup or e.method != "test": continue
        subj = e.subject_id or (reqs[e.requirement_id].target if e.requirement_id in reqs else None)
        owners = {c.id for c in spec.components if subj == c.id or subj in c.provides} | ({models[subj].owner} if subj in models else set())
        for c in spec.components:
            if c.id in owners and c.test_paths and not _under_tests(e.provenance, c.test_paths):
                add("COMP-08", f"Evidence '{e.id}' provenance '{e.provenance}' is outside test_paths of component '{c.id}'", c.id)

    for c in spec.components:
        if not c.test_paths: continue
        need_caps = {r.partition("@^")[0] for r in c.requires}
        if c.test_scope == "unit":
            for k in sorted(need_caps - set(c.fakes)): add("COMP-09", f"Component '{c.id}' has no fake for required capability '{k}'", c.id)
        for k, p in c.fakes.items():
            if k not in need_caps: add("COMP-09", f"Component '{c.id}' declares a fake for '{k}', which it does not require", c.id)
            elif not _under_tests(p, c.test_paths): add("COMP-09", f"Component '{c.id}' fake '{p}' for '{k}' is outside test_paths", c.id)

    # experiences (optional; vacuous when none are declared)
    views, sets = {v.id: v for v in spec.views}, spec.state_sets
    vstates = lambda v: {**{k: h for n in v.state_sets for k, h in sets.get(n, {}).items()}, **v.states}
    scr = lambda sc: [*([sc.requirement] if sc.requirement else []), *sc.requirements]
    for kind, xs in [("view", spec.views), ("experience", spec.experiences)]:
        for i, n in Counter(x.id for x in xs).items():
            if n > 1: add("EXP-01", f"Duplicate {kind} id '{i}'")
    refs = [(f"View '{v.id}'", v.references) for v in spec.views] + [(f"Experience '{x.id}'", x.references) for x in spec.experiences]
    for who, rs in refs:
        for r in rs:
            if r.authority == "normative" and not (r.version and r.accepted_by):
                add("EXP-06", f"{who} normative reference '{r.path}' requires version and accepted_by")
            for q in r.consistent_with:
                if q not in reqs: add("EXP-02", f"{who} reference '{r.path}' is consistent_with unknown requirement '{q}'")
    for v in spec.views:
        for n in v.state_sets:
            if n not in sets: add("EXP-02", f"View '{v.id}' uses unknown state set '{n}'")
        for name, handling in v.states.items():
            if not handling.strip(): add("EXP-04", f"View '{v.id}' state '{name}' states no handling")
    for n, st in sets.items():
        for name, handling in st.items():
            if not handling.strip(): add("EXP-04", f"State set '{n}' state '{name}' states no handling")
    for x in spec.experiences:
        for kind, xs in [("interaction", x.interactions), ("scenario", x.scenarios)]:
            for i, n in Counter(y.id for y in xs).items():
                if n > 1: add("EXP-01", f"Experience '{x.id}' has duplicate {kind} id '{i}'")
        for r in x.requirements:
            if r not in reqs: add("EXP-02", f"Experience '{x.id}' realizes unknown requirement '{r}'")
        for c in x.capabilities:
            if c not in caps and _import_cap(spec, c) is None: add("EXP-02", f"Experience '{x.id}' realizes unknown capability '{c}'")
        for v in x.views:
            if v not in views: add("EXP-02", f"Experience '{x.id}' uses unknown view '{v}'")
        for name, c in x.constraints.items():
            if isinstance(c, Constraint) and c.requirement and c.requirement not in reqs:
                add("EXP-02", f"Experience '{x.id}' constraint '{name}' names unknown requirement '{c.requirement}'")
        own, acts = {v for v in x.views if v in views}, {i.id: i for i in x.interactions}
        for i in x.interactions:
            who = f"Interaction '{x.id}.{i.id}'"
            if (i.from_view is None) == (i.from_entry is None): add("EXP-02", f"{who} needs exactly one of from_view and from_entry")
            if i.from_view is not None and i.from_view not in own: add("EXP-02", f"{who} from_view '{i.from_view}' is not a view of the experience")
            if i.from_entry is not None and i.from_entry not in x.entries: add("EXP-02", f"{who} from_entry '{i.from_entry}' is not an entry of the experience")
            if i.to_view not in own: add("EXP-02", f"{who} to_view '{i.to_view}' is not a view of the experience")
            ends = [views[v] for v in (i.from_view, i.to_view) if v in own]
            if not i.outcome.strip() or (i.from_view in own and i.trigger not in views[i.from_view].actions):
                add("EXP-03", f"{who} needs an observable outcome and, from a view, a trigger that view offers")
            for fm, st in i.failure_modes.items():
                if not any(st in vstates(v) for v in ends): add("EXP-04", f"{who} failure mode '{fm}' maps to '{st}', which is not a state of its views")
        for sc in x.scenarios:
            if not scr(sc): add("EXP-02", f"Scenario '{x.id}.{sc.id}' needs at least one requirement")
            for r in scr(sc):
                if r not in reqs: add("EXP-02", f"Scenario '{x.id}.{sc.id}' verifies through unknown requirement '{r}'")
            for a in sc.interactions:
                if a not in acts: add("EXP-02", f"Scenario '{x.id}.{sc.id}' exercises unknown interaction '{a}'")
            for st in sc.states:
                v, _, name = st.partition(".")
                if v not in own or name not in vstates(views[v]): add("EXP-02", f"Scenario '{x.id}.{sc.id}' exercises unknown view state '{st}'")
        if x.critical:
            done = {a for sc in x.scenarios for a in sc.interactions}
            neg = {a for sc in x.scenarios if sc.kind == "negative" for a in sc.interactions}
            gaps = [i.id for i in x.interactions if i.id not in done or (i.failure_modes and i.id not in neg)]
            if gaps or not x.scenarios: add("EXP-05", f"Critical experience '{x.id}' lacks positive/negative scenarios for interactions {gaps or 'at all'}")
            for sc in x.scenarios:
                for r in scr(sc):
                    if r in reqs and r not in ok and r not in dreq: add("EXP-07", f"Scenario '{x.id}.{sc.id}' requirement '{r}' is not verified")
            for name, c in x.constraints.items():
                if isinstance(c, Constraint) and c.requirement in reqs and c.requirement not in ok and c.requirement not in dreq:
                    add("EXP-07", f"Experience '{x.id}' constraint '{name}' requirement '{c.requirement}' is not verified")
            if x.exhaustive:
                exercised = {r for sc in x.scenarios for r in scr(sc)}
                for r in x.requirements:
                    if r not in exercised and not x.not_exercised.get(r, "").strip():
                        add("EXP-08", f"Exhaustive critical experience '{x.id}' requirement '{r}' is exercised by no scenario and has no not_exercised reason")
                for r in x.not_exercised:
                    if r not in x.requirements: add("EXP-08", f"Experience '{x.id}' not_exercised names '{r}', which it does not realize")
            for i in x.interactions:
                ends = {v for v in (i.from_view, i.to_view) if v}
                negs = [sc for sc in x.scenarios if sc.kind == "negative" and i.id in sc.interactions]
                hit = {st for sc in negs for st in sc.states}
                for fm, st in i.failure_modes.items():
                    if negs and not {f"{v}.{st}" for v in ends} & hit:
                        add("EXP-11", f"Interaction '{x.id}.{i.id}' failure mode '{fm}' ({st}) is reached by no negative scenario of that interaction")
            for sc in x.scenarios:
                if sc.kind == "negative" and len(sc.interactions) != 1:
                    add("EXP-12", f"Negative scenario '{x.id}.{sc.id}' must exercise exactly one interaction")
                ends = {v for a in sc.interactions if a in acts for v in (acts[a].from_view, acts[a].to_view) if v}
                for st in sc.states:
                    if ends and st.partition(".")[0] not in ends:
                        add("EXP-12", f"Scenario '{x.id}.{sc.id}' state '{st}' is not on a view its interactions connect")
    full = {v for x in spec.experiences if x.critical and x.exhaustive for v in x.views if v in views}
    triggered = {(i.from_view, i.trigger) for x in spec.experiences for i in x.interactions if i.from_view}
    reached = {st for x in spec.experiences for sc in x.scenarios for st in sc.states}
    for v in sorted(full):
        for t in views[v].actions:
            if (v, t) not in triggered: add("EXP-09", f"View '{v}' action '{t}' triggers no interaction")
        for name in views[v].states:
            if f"{v}.{name}" not in reached: add("EXP-10", f"View '{v}' state '{name}' is reached by no scenario")

    # release: status, evidence, risk disposition, no waivers
    need("STAT-04", "verified", [("System", [spec.system] if spec.system.id not in dent else []), ("Capability", [x for x in spec.capabilities if x.id not in dent]),
                                 ("Component", [x for x in spec.components if x.id not in dent])])
    need("STAT-04", "verified", [("Requirement", [r for r in active.values() if r.id not in dreq])])
    for e in spec.evidence:
        if e.id not in sup:
            subj = e.subject_id or (reqs[e.requirement_id].target if e.requirement_id in reqs else None)
            binding = spec.subject_bindings.get(subj)
            matched = (binding and e.subject_version == binding.version == vers.get(subj) and e.subject_revision == binding.revision
                       if binding or e.subject_version is not None else e.subject_revision == vers.get(subj))
            if not (e.subject_revision and e.timestamp and e.verifier) or (subj in vers and not matched):
                add("EV-04", f"Evidence '{e.id}' requires timestamp, verifier and matching subject version/source binding for '{subj}'")
            if e.skipped: add("EV-05", f"Evidence '{e.id}' has skipped checks: {e.skipped}")
    mitig = {m for k in spec.risks for m in k.mitigated_by}
    oblig = {r.id for r in active.values() if r.mandatory} | {d.declassified_by for d in spec.data_models} | {r.resolved_by for r in active.values()}
    kids: dict[str, list[str]] = {}
    for r in active.values():
        for p in r.refines: kids.setdefault(p, []).append(r.id)
    while new := {k for p in oblig for k in kids.get(p, [])} - oblig: oblig |= new
    for r in active.values():
        _, failing, uncovered = state.get((r.id, r.version), (False, [], []))
        part = lambda cs: f" for criteria {cs}" if cs and set(cs) != set(r.crit) else ""
        if r.id in dreq: continue
        if r.id in oblig and (r.id, r.version) in failed: add("REQ-04", f"Requirement '{r.id}@{r.version}' has unresolved failing evidence{part(failing)}")
        elif r.id in oblig and r.id not in ok: add("REQ-05", f"Requirement '{r.id}@{r.version}' missing valid verification evidence{part(uncovered)}")
        if r.id in oblig | mitig:
            for k in kids.get(r.id, []):
                if k not in ok: add("REQ-08", f"Requirement '{r.id}' is refined by unverified requirement '{k}'")
            for d in r.depends_on:
                if d in reqs and d not in ok: add("REQ-06", f"Requirement '{r.id}' depends on unverified requirement '{d}'")
    for k in sorted({k for c in spec.components for k in c.fakes}):
        if not any(r.category == "contract" and r.target == k and r.id in ok for r in active.values()):
            add("COMP-10", f"Capability '{k}' is faked by {sorted(c.id for c in spec.components if k in c.fakes)} but has no verified contract requirement", k)
    for k in spec.risks:
        if k.treatment in ("mitigation", "avoidance") and not k.mitigated_by: add("RISK-03", f"Risk '{k.id}' has no mitigating requirements")
        for m in k.mitigated_by:
            if m in reqs and m not in ok and m not in dreq: add("RISK-04", f"Risk '{k.id}' has unverified mitigation '{m}'")
        if k.treatment == "avoidance" and k.residual_accepted: add("RISK-05", f"Risk '{k.id}' is avoided and must not carry residual risk")
        if k.treatment == "transfer" and not k.transferred_to: add("RISK-06", f"Risk '{k.id}' requires transferred_to")
        if k.treatment in ("acceptance", "transfer") and not k.residual_accepted: add("RISK-07", f"Risk '{k.id}' requires residual_accepted")
        if k.waives: add("RISK-08", f"Risk '{k.id}' waives '{k.waives}'; no waivers permitted at release")

    s, ws = STAGES.index(stage), active_waivers(spec, stage, today)
    return [m for st, m, w, sub in E if st <= s and not (len(w) > 1 and any(k.waives in w and k.target in sub | {sid} for k in ws))]

def _breaking(errs: list[str], old: str, new: str) -> list[str]:
    o, n = _ver(old), _ver(new)
    return errs + ([f"[COMPAT-08] Breaking change requires major version bump ({old} -> {new})"] if errs and (not o or not n or n[0] <= o[0]) else [])

def check_compatibility(old: Capability, new: Capability) -> list[str]:
    return _breaking([f"[{i}] {msg}: {sorted(diff)}" for diff, i, msg in [
        (set(new.preconditions) - set(old.preconditions), "COMPAT-01", "Strengthened preconditions"),
        (set(old.postconditions) - set(new.postconditions), "COMPAT-02", "Removed postconditions"),
        (set(old.invariants) - set(new.invariants), "COMPAT-03", "Removed invariants"),
        (set(new.failure_modes) - set(old.failure_modes), "COMPAT-04", "Added failure modes"),
    ] if diff], old.version, new.version)

def check_data_compatibility(old: DataModel, new: DataModel) -> list[str]:
    errs = [f"[{i}] {msg}: {sorted(diff)}" for diff, i, msg in [
        (set(old.fields) - set(new.fields), "COMPAT-05", "Removed fields"),
        ({f for f, t in old.fields.items() if new.fields.get(f, t) != t}, "COMPAT-06", "Changed field types"),
    ] if diff]
    if old.classification != new.classification: errs.append(f"[COMPAT-07] Changed classification ({old.classification} -> {new.classification})")
    return _breaking(errs, old.version, new.version)

def compare_specs(old: Spec, new: Spec) -> list[str]:
    caps, dms = {c.id: c for c in new.capabilities}, {d.id: d for d in new.data_models}
    at = lambda e, x: e.replace("] ", f"] {x}: ", 1)
    return [at(e, f"Capability '{c.id}'") for c in old.capabilities for e in (
        check_compatibility(c, caps[c.id]) if c.id in caps else [] if c.status == "deprecated" else ["[COMPAT-09] removed without deprecation"])] + [
        at(e, f"DataModel '{d.id}'") for d in old.data_models for e in (
            check_data_compatibility(d, dms[d.id]) if d.id in dms else ["[COMPAT-05] removed data model"])]

def context_slice(spec: Spec, cid: str) -> dict:
    c = next(x for x in spec.components if x.id == cid)
    capids = set(c.provides) | {r.partition("@^")[0] for r in c.requires}
    caps = [x for x in spec.capabilities if x.id in capids]
    dms = set(c.owns_data) | {x for cap in caps if cap.interface for xs in (cap.interface.request, cap.interface.response) for x in _list(xs)}
    ids = {spec.system.id, cid, *capids, *dms}
    return Spec(system=spec.system, components=[c], capabilities=caps, data_models=[d for d in spec.data_models if d.id in dms],
                requirements=[r for r in spec.requirements if r.target in ids], risks=[k for k in spec.risks if k.target in ids],
                subject_bindings={k: v for k, v in spec.subject_bindings.items() if k in ids}
                ).model_dump(mode="json", exclude_defaults=True)

def diagram(spec: Spec) -> str:
    """Mermaid flowchart: components and their data models grouped by layer (dashed boxes), then domain (solid boxes), resolved capability dependencies, data ownership/lineage, status styling.
    Persisted data models (namespace or storage_role) are cylinders; logical-only ones are dashed boxes. External components are hexagons outside the system listing their API;
    runtime-mediated (via) dependencies are dashed. Node IDs are prefixed per kind so equal IDs never collide."""
    mid = lambda k, s: k + "_" + "".join(c if c.isalnum() else "_" for c in s)
    node = lambda x: f'{mid("c", x.id)}[["{x.id} v{x.version}<br>subsystem: {x.realized_by}<br>{x.status}"]]' if x.realized_by else f'{mid("c", x.id)}["{x.id} v{x.version}<br>{x.status}"]'
    stored = lambda d: bool(d.namespace or d.storage_role)
    data = lambda d: f'{mid("d", d.id)}[("{d.id}<br>{d.namespace or d.storage_role}<br>{d.classification}")]' if stored(d) else f'{mid("d", d.id)}("{d.id}<br>{d.classification}")'
    owner = {c.id: c for c in spec.components}
    ext = lambda c: bool(c and c.external)
    lay = lambda c: c.layer if c and c.layer in spec.system.layers else None
    dom = lambda c: c.domain if c and c.domain in {d.id for d in spec.system.domains} else None
    def group(layer, ind):
        out = []
        for g in [*(d.id for d in spec.system.domains), None]:
            body = [node(c) for c in spec.components if not ext(c) and lay(c) == layer and dom(c) == g]
            body += [data(d) for d in spec.data_models if not ext(owner.get(d.owner)) and lay(owner.get(d.owner)) == layer and dom(owner.get(d.owner)) == g]
            if body and g: out += [f'{ind}subgraph {mid("g", f"{layer}_{g}" if layer else g)}["{g}"]', *[f"{ind}  {x}" for x in body], f"{ind}end"]
            else: out += [f"{ind}{x}" for x in body]
        return out
    out, layers = ["graph LR", f'  subgraph {mid("s", spec.system.id)}["{spec.system.id} v{spec.system.version} ({spec.system.status})"]'], []
    for layer in spec.system.layers:
        if body := group(layer, "      "): out += [f'    subgraph {mid("l", layer)}["{layer}"]', *body, "    end"]; layers.append(mid("l", layer))
    out += group(None, "    ") + ["  end"]
    out += [f'  {mid("c", c.id)}{{{{"{c.id} v{c.version}<br>external {c.status}<br>API: {", ".join(c.provides)}"}}}}' for c in spec.components if c.external]
    out += [f"  {data(d)}" for d in spec.data_models if ext(owner.get(d.owner))]
    resolved = resolve_capabilities(spec)
    for c in spec.components:
        out += [f"  {mid('c', c.id)} -.->|{k} via {c.via[k]}| {mid('c', p)}" if k in c.via else f"  {mid('c', c.id)} -->|{k}| {mid('c', p)}" for k, p in resolved.get(c.id, {}).items()]
        out += [f"  {mid('c', c.id)} -.->|owns| {mid('d', d)}" for d in c.owns_data]
    for d in spec.data_models:
        out += [f"  {mid('d', d.id)} -.->|derived_from| {mid('d', x)}" for x in d.derived_from]
    colors = {"draft": "#eee,stroke:#999,color:#333", "specified": "#cce5ff,stroke:#36c", "verified": "#d5f5d5,stroke:#393", "deprecated": "#fdd,stroke:#c33,stroke-dasharray:4"}
    out += [f"  classDef {s} fill:{v}" for s, v in colors.items()] + ["  classDef logical stroke-dasharray:4"]
    out += [f"  style {l} stroke-dasharray:6 4" for l in layers]
    out += [f"  class {','.join(mid('c', c.id) for c in spec.components if c.status == s)} {s}" for s in STATUS if any(c.status == s for c in spec.components)]
    if lg := [mid("d", d.id) for d in spec.data_models if not stored(d)]: out += [f"  class {','.join(lg)} logical"]
    return "\n".join(out)

def residual_risks(spec: Spec) -> list[str]:
    return [r.id for r in spec.risks if r.residual_accepted]

def impact_analysis(spec: Spec, target_id: str) -> dict[str, list[str]]:
    seen, todo = set(), [target_id]
    while todo:
        x = todo.pop()
        if x in seen: continue
        seen.add(x)
        todo += [c.id for c in spec.components if x in {r.partition("@^")[0] for r in c.requires} or (x == target_id and x in c.provides)]
        todo += [d.id for d in spec.data_models if x in d.derived_from]
        todo += [y for c in spec.components if c.id == x for y in c.provides + c.owns_data]
        todo += [d.owner for d in spec.data_models if d.id == x]
    reqs = {r.id for r in spec.requirements if r.target in seen | {spec.system.id} or r.id == target_id}
    while new := {r.id for r in spec.requirements if r.id not in reqs and reqs & {*r.depends_on, *r.refines}}: reqs |= new
    ids = lambda xs: [x.id for x in xs if x.id in seen]
    return {
        "components": ids(spec.components),
        "capabilities": ids(spec.capabilities),
        "data_models": ids(spec.data_models),
        "requirements": [r.id for r in spec.requirements if r.id in reqs],
        "invalidated_evidence": [e.id for e in spec.evidence if e.requirement_id in reqs or e.subject_id in seen],
    }

class ReturnResult(M):
    """One verification result an agent returns for a requirement; maps 1:1 to an Evidence record."""
    requirement: str = F("Requirement ID.")
    requirement_version: str = F("Requirement version verified.")
    method: Literal["test", "proof", "model-check", "analysis", "inspection"] = F("Verification method.")
    passed: bool = F("Result.")
    provenance: str = F("Artifact or tool reference, e.g. 'tests/checkout/test_x.py::test_x'.")
    subject_revision: str | None = F("Source revision the result was produced against.", None)
    environment: str | None = F("Where it ran (host, OS or CI job).", None)
    criteria: list[str] = F("Requirement criteria this result covers; empty covers the whole requirement.", [])
    skipped: list[str] = F("Skipped check names/reasons; partial results cannot verify.", [])

class Unmet(M):
    """A requirement the handoff did not satisfy."""
    requirement: str = F("Requirement ID.")
    reason: str = Field(min_length=1, description="Why it is unmet.")

class Finding(M):
    """A new fact surfaced while implementing (feeds constitution self-improvement)."""
    kind: Literal["model-gap", "hidden-coupling", "failure-mode", "assumption"] = F("Finding type.")
    text: str = F("What was found.")

class HandoffReturn(M):
    """What an implementer returns for a bounded component handoff (constitution § Implementation Handoff)."""
    component: str = F("Component ID the handoff targeted.")
    base: str | None = F("Spec/source revision the handoff started from.", None)
    results: list[ReturnResult] = F("Requirement-linked verification results.", [])
    unmet: list[Unmet] = F("Requirements not satisfied, each with a reason.", [])
    changed: list[str] = F("Repository-relative paths changed.", [])
    findings: list[Finding] = F("Newly discovered facts or model gaps.", [])

def return_json_schema() -> dict:
    return {"$schema": "https://json-schema.org/draft/2020-12/schema", **HandoffReturn.model_json_schema()}

def check_return(spec: Spec, ret: HandoffReturn, allow: list[str] = ()) -> list[str]:
    """Check a handoff return against the spec: no silently dropped requirements, results match current versions and component scope, failures are listed unmet, changes stay in scope."""
    c = next((x for x in spec.components if x.id == ret.component), None)
    if c is None: return [f"return: unknown component '{ret.component}'"]
    scope = {c.id, *c.provides, *c.owns_data}
    reqs = {r.id: r for r in spec.requirements if r.status != "deprecated"}
    applicable = {i for i, r in reqs.items() if r.target in scope}
    errors, seen, unmet, cover = [], set(), {u.requirement for u in ret.unmet}, {}
    for r in ret.results:
        q = reqs.get(r.requirement)
        if q is None: errors.append(f"return: unknown or deprecated requirement '{r.requirement}'"); continue
        got = cover.setdefault(r.requirement, set()); mine = {q.ckey(c) or c for c in r.criteria} or set(q.crit)
        if got & mine: errors.append(f"return: duplicate result for '{r.requirement}'")
        if bad := [c for c in r.criteria if q.ckey(c) is None]: errors.append(f"return: result for '{r.requirement}' names unknown criteria {bad}")
        if r.passed: got |= mine
        seen.add(r.requirement)
        if r.requirement not in applicable: errors.append(f"return: requirement '{r.requirement}' does not apply to component '{c.id}'")
        if r.requirement_version != q.version: errors.append(f"return: result for '{r.requirement}' has version {r.requirement_version}, current is {q.version}")
        if r.passed and r.skipped: errors.append(f"return: passing result for '{r.requirement}' has skipped checks; partial results cannot verify")
        if not r.passed and r.requirement not in unmet: errors.append(f"return: failing result for '{r.requirement}' is not listed in unmet")
        if r.method == "test" and c.test_paths and not _under_tests(r.provenance, c.test_paths): errors.append(f"return: test provenance '{r.provenance}' for '{r.requirement}' is outside test_paths of '{c.id}'")
    errors += [f"return: criteria {missing} of '{i}' not covered by passing results (cover them or list '{i}' as unmet)" for i in sorted(seen - unmet)
               if i in applicable and (missing := [c for c in reqs[i].crit if c not in cover[i]]) and not any(r.requirement == i and not r.passed for r in ret.results)]
    for u in ret.unmet:
        if u.requirement not in reqs: errors.append(f"return: unmet requirement '{u.requirement}' is unknown or deprecated")
    errors += [f"return: requirement '{i}' is neither a result nor unmet" for i in sorted(applicable - seen - unmet)]
    return errors + check_scope(spec, c.id, ret.changed, allow)

def check_imports(spec: Spec, root: str | Path, base: str | Path) -> list[str]:
    """Verify each import pin and subsystem realization against the imported spec file (resolved from base, must stay under root)."""
    root, base, errors = Path(root).resolve(), Path(base).resolve(), []
    pcaps, pdms, dfr = {c.id: c for c in spec.capabilities}, {d.id: d for d in spec.data_models}, deferred_scope(spec)
    dreq, dent = set(dfr["requirements"]), set(dfr["entities"])
    for i in spec.system.imports:
        p = Path(i.path.replace("\\", "/")); t = (base / p).resolve()
        if p.is_absolute() or PurePosixPath(p.as_posix()).is_absolute() or PureWindowsPath(i.path).drive or not t.is_relative_to(root):
            errors.append(f"imports: unsafe or outside-root path '{i.path}' for '{i.id}'"); continue
        try: other = load_spec(t)
        except (OSError, yaml.YAMLError, ValidationError, IncludeError) as e: errors.append(f"imports: cannot load '{i.path}' for '{i.id}': {e}" + (f" (clone {i.link})" if i.link else "")); continue
        if other.system.version != i.version: errors.append(f"imports: '{i.id}' pins version {i.version}, spec is {other.system.version}")
        ocaps, odms = {c.id: c for c in other.capabilities}, {d.id: d for d in other.data_models}
        have, exported = {k: c.version for k, c in ocaps.items()}, set(other.system.exports) if other.system.exports is not None else set(ocaps)
        for k, v in i.capabilities.items():
            if k not in have: errors.append(f"imports: '{i.id}' capability '{k}' not found in {i.path}")
            elif k not in exported: errors.append(f"imports: '{i.id}' capability '{k}' is not exported by {i.path}")
            elif have[k] != v: errors.append(f"imports: '{i.id}' capability '{k}' pins {v}, spec has {have[k]}")
        if i.sha256 and hashlib.sha256(t.read_bytes()).hexdigest() != i.sha256: errors.append(f"imports: '{i.id}' sha256 mismatch for {i.path}")
        if i.mode == "baseline": errors += _check_baseline(spec, i, other, t.parent, root)
        oact, ook = {r.id: r for r in other.requirements if r.status != "deprecated"}, verified_requirements(other)
        for r in spec.requirements:
            if r.status == "deprecated": continue
            for x in [*r.depends_on, *r.refines]:
                if x.startswith(f"{i.id}:"):
                    rem = x.partition(":")[2]
                    if rem not in oact: errors.append(f"imports: '{i.id}' requirement '{rem}' referenced by '{r.id}' not found or deprecated in {i.path}")
                    elif r.id not in dreq and r.status == "verified" and (rem, oact[rem].version) not in ook:
                        errors.append(f"imports: '{i.id}' requirement '{rem}' referenced by '{r.id}' is not verified in {i.path}")
        oprov = {p for oc in other.components if not oc.external for p in oc.provides} & exported
        oext = {p.partition(":")[2] or p for oc in other.components if oc.external for p in oc.provides}
        oreq = {r.partition("@^")[0].partition(":")[2] or r.partition("@^")[0] for oc in other.components if not oc.external for r in oc.requires} & oext
        for c in [c for c in spec.components if c.realized_by == i.id]:
            for k in c.provides:
                if k not in oprov: errors.append(f"imports: '{i.id}' realizing '{c.id}' does not provide and export capability '{k}'")
                elif k in pcaps and k in ocaps:
                    if not _ver_ok(pcaps[k].version, ocaps[k].version): errors.append(f"imports: '{i.id}' realizing '{c.id}' capability '{k}' version {ocaps[k].version} is incompatible with {pcaps[k].version}")
                    errors += [f"imports: '{i.id}' realizing '{c.id}' capability '{k}': {ce}" for ce in check_compatibility(pcaps[k], ocaps[k])]
            creq = {r.partition("@^")[0].partition(":")[2] or r.partition("@^")[0] for r in c.requires}
            errors += [f"imports: '{i.id}' realizing '{c.id}' requires external capability '{k}' not required by '{c.id}'" for k in sorted(oreq - creq)]
            cdms = set(c.owns_data) | {x for k in c.provides if k in pcaps and pcaps[k].interface for xs in (pcaps[k].interface.request, pcaps[k].interface.response) for x in _list(xs)}
            for d in sorted(cdms):
                if d in pdms and d in odms:
                    errors += [f"imports: '{i.id}' realizing '{c.id}' data model '{d}': {de}" for de in check_data_compatibility(pdms[d], odms[d])]
            if c.status == "verified" and c.id not in dent and (other.system.status != "verified" or validate_spec(other, "release")):
                errors.append(f"imports: '{i.id}' realizing verified component '{c.id}' is not release-verified")
    return errors

def _safe(root: Path, base: Path, rel: str) -> Path | None:
    p = Path(rel.replace("\\", "/"))
    return None if p.is_absolute() or PureWindowsPath(rel).drive or not (t := (base / p).resolve()).is_relative_to(root) else t

def check_workspace(path: str | Path, stage: str = "release") -> list[str]:
    """Validate every member at stage and its imports; imports target members, links agree, system IDs are unique and import edges are acyclic. Offline: members must already be checked out."""
    path = Path(path).resolve(); root, errors, specs, at, link = path.parent, [], {}, {}, lambda u: u and u.rstrip("/").removesuffix(".git")
    try: ws = Workspace.model_validate(yaml.safe_load(path.read_text(encoding="utf-8")))
    except (OSError, yaml.YAMLError, ValidationError) as e: return [f"workspace: {e}"]
    if len({m.id for m in ws.members}) < len(ws.members): errors.append("workspace: duplicate member IDs")
    for m in ws.members:
        if (t := _safe(root, root, m.path)) is None: errors.append(f"workspace: member '{m.id}' has unsafe or outside-root path '{m.path}'"); continue
        try: specs[m.id] = load_spec(t)
        except (OSError, yaml.YAMLError, ValidationError, IncludeError) as e: errors.append(f"workspace: member '{m.id}' cannot load '{m.path}': {e}" + (f" (clone {m.link})" if m.link else "")); continue
        at[t] = m.id
        errors += [f"{m.id}: {e}" for e in validate_spec(specs[m.id], m.stage or stage)]
    sids = [s.system.id for s in specs.values()]
    errors += [f"workspace: system ID '{i}' is declared by more than one member" for i in sorted({i for i in sids if sids.count(i) > 1})]
    members, deps = {m.id: m for m in ws.members}, {k: set() for k in specs}
    for k, s in specs.items():
        base = (root / members[k].path).resolve().parent
        errors += [f"{k}: {e}" for e in check_imports(s, root, base)]
        for i in s.system.imports:
            t = _safe(root, base, i.path)
            if t is None or t not in at: errors.append(f"{k}: import '{i.id}' targets '{i.path}', which is not a workspace member" if t else f"{k}: import '{i.id}' has unsafe path"); continue
            deps[k].add(at[t])
            if i.link and members[at[t]].link and link(i.link) != link(members[at[t]].link): errors.append(f"{k}: import '{i.id}' link {i.link} differs from member '{at[t]}' link {members[at[t]].link}")
    while (free := [k for k, d in deps.items() if not d]):
        deps = {k: d - set(free) for k, d in deps.items() if k not in free}
    if deps: errors.append(f"workspace: import cycle among members {', '.join(sorted(deps))}")
    return errors

def _deviations(base, derived, path: str, out: list[str]) -> None:
    """Paths where baseline content is changed or removed; additions are ignored."""
    if isinstance(base, dict) and isinstance(derived, dict):
        for k, v in base.items():
            if k not in derived: out.append(f"{path}/{k}")
            else: _deviations(v, derived[k], f"{path}/{k}", out)
    elif isinstance(base, list) and isinstance(derived, list) and all(isinstance(x, dict) and "id" in x for x in base):
        idx = {x["id"]: x for x in derived if isinstance(x, dict) and "id" in x}
        for x in base:
            if x["id"] not in idx: out.append(f"{path}/{x['id']}")
            else: _deviations(x, idx[x["id"]], f"{path}/{x['id']}", out)
    elif isinstance(base, list) and isinstance(derived, list):
        if any(x not in derived for x in base): out.append(path)
    elif base != derived: out.append(path)

def _check_baseline(spec: Spec, i: SpecImport, other: Spec, base: Path, root: Path) -> list[str]:
    """Baseline import: artifact digests, then every changed or removed baseline element is covered by a refinement, and every refinement by a deviation."""
    errors, devs = [], []
    for rel, digest in i.artifacts.items():
        f = (base / rel).resolve()
        if not f.is_relative_to(root) or not f.is_file(): errors.append(f"imports: '{i.id}' baseline artifact '{rel}' missing or outside root")
        elif hashlib.sha256(f.read_bytes()).hexdigest() != digest: errors.append(f"imports: '{i.id}' baseline artifact '{rel}' modified")
    dump = lambda x: x.model_dump(mode="json", exclude_defaults=True)
    for kind, mine, theirs in [("requirements", spec.requirements, other.requirements), ("risks", spec.risks, other.risks)]:
        have, ok = {x.id: dump(x) for x in mine}, set(i.allow_fields.get(kind, [])) | {"id"}
        for x in theirs:
            d, b = have.get(i.id_prefix.get(kind, "") + x.id), dump(x)
            if d is None: devs.append(f"{kind}/{x.id}"); continue
            devs += [f"{kind}/{x.id}/{k}" for k in sorted(set(b) | set(d)) if k not in ok and b.get(k) != d.get(k)]
    for kind in ("views", "experiences"):
        ok = set(i.allow_fields.get(kind, []))
        strip = lambda xs, f: [{k: v for k, v in f(x).items() if k not in ok} for x in xs]
        _deviations(strip(getattr(other, kind), dump), strip(getattr(spec, kind), lambda x: x.model_dump(mode="json")), kind, devs)
    _deviations(other.state_sets, spec.state_sets, "state_sets", devs)
    declared, used = {r.path for r in i.refinements}, set()
    for d in devs:
        parts = d.split("/")
        hit = next(("/".join(parts[:n]) for n in range(len(parts), 0, -1) if "/".join(parts[:n]) in declared), None)
        if hit: used.add(hit)
        else: errors.append(f"imports: '{i.id}' undeclared deviation from baseline: {d}")
    errors += [f"imports: '{i.id}' refinement '{p}' names no deviation" for p in sorted(declared - used)]
    return errors

def _report(paths: list[str]) -> list[tuple[str, list[str], bool, ET.Element]]:
    """JUnit testcases as (test id, bound 'requirement@version' refs, passed, enclosing testsuite). The test id is the `test` property, else classname::name; refs are `verifies` properties."""
    out = []
    for path in paths:
        for ts in ET.parse(path).getroot().iter("testsuite"):
            for tc in ts.findall("testcase"):
                props = [(x.get("name"), x.get("value", "")) for x in tc.iter("property")]
                out.append((next((v for k, v in props if k == "test"), f"{tc.get('classname')}::{tc.get('name')}"), [v for k, v in props if k == "verifies"],
                            not any(tc.find(t) is not None for t in ("failure", "error", "skipped")), ts))
    return out

def check_report(spec: Spec, paths: list[str]) -> list[str]:
    """Every reported test binds exactly one current requirement version; non-superseded `test` evidence is present in the reports and agrees on requirement, version and result (§ 8)."""
    reqs, sup, errors, seen = {r.id: r for r in spec.requirements}, _superseded(spec), [], {}
    for tid, refs, passed, _ in _report(paths):
        seen[tid] = (refs, passed)
        if len(refs) != 1: errors.append(f"report: test '{tid}' must bind exactly one requirement, found {len(refs)}"); continue
        rid, _, ver = refs[0].partition("@")
        if rid not in reqs: errors.append(f"report: test '{tid}' binds unknown requirement '{rid}'")
        elif reqs[rid].version != ver: errors.append(f"report: test '{tid}' binds '{rid}@{ver}', current version is {reqs[rid].version}")
    for e in spec.evidence:
        if e.method != "test" or e.id in sup: continue
        if e.provenance not in seen: errors.append(f"report: evidence '{e.id}' provenance '{e.provenance}' is not in the run reports")
        else:
            refs, passed = seen[e.provenance]
            if refs != [f"{e.requirement_id}@{e.requirement_version}"]: errors.append(f"report: evidence '{e.id}' is for '{e.requirement_id}@{e.requirement_version}' but the test binds {refs}")
            if passed != e.passed: errors.append(f"report: evidence '{e.id}' passed={e.passed} but the run reports passed={passed}")
    return errors

def evidence_from_report(spec: Spec, paths: list[str]) -> list[dict]:
    """`test` evidence records derived from run reports; add subject_revision and verifier (EV-04) in the producing CI step."""
    reqs = {r.id: r for r in spec.requirements}
    return [{"id": "ev-" + re.sub(r"\W+", "-", tid).strip("-"), "requirement_id": refs[0].partition("@")[0], "requirement_version": refs[0].partition("@")[2],
             "method": "test", "passed": passed, "provenance": tid, **({"timestamp": ts.get("timestamp")} if ts.get("timestamp") else {}),
             **({"environment": ts.get("hostname")} if ts.get("hostname") else {})}
            for tid, refs, passed, ts in _report(paths) if len(refs) == 1 and refs[0].partition("@")[0] in reqs]

_LIFECYCLE = {"draft": "experimental", "specified": "experimental", "verified": "production", "deprecated": "deprecated"}

def export_backstage(spec: Spec, owner: str = "unknown") -> list[dict]:
    """Backstage catalog entities (System, Component, API, Resource) from the spec; owner is required by Backstage but not modeled, so it is a parameter."""
    n = lambda s: re.sub(r"[^A-Za-z0-9_.-]", "-", s)[:63]
    res, api = resolve_capabilities(spec), lambda ref: n(ref.partition("@^")[0])
    meta = lambda name, **extra: {"name": n(name), **extra}
    docs = [{"apiVersion": "backstage.io/v1alpha1", "kind": "System", "metadata": meta(spec.system.id, description=f"v{spec.system.version} {spec.system.status}"), "spec": {"owner": owner}}]
    for c in spec.components:
        labels = {k: v for k, v in (("layer", c.layer), ("domain", c.domain)) if v}
        docs.append({"apiVersion": "backstage.io/v1alpha1", "kind": "Component", "metadata": meta(c.id, description=f"v{c.version} {c.status}", **({"labels": labels} if labels else {})),
                     "spec": {"type": "external" if c.external else "service", "lifecycle": _LIFECYCLE[c.status], "owner": owner, "system": n(spec.system.id),
                              "providesApis": [api(p) for p in c.provides], "consumesApis": [api(r) for r in c.requires],
                              "dependsOn": [f"component:{n(p)}" for p in sorted(set(res.get(c.id, {}).values()))]}})
    caps = {c.id: c for c in spec.capabilities}
    for k in dict.fromkeys(p for c in spec.components for p in [*c.provides, *(r.partition("@^")[0] for r in c.requires)]):
        cap = caps.get(k)
        if cap:
            i = cap.interface
            kind = "asyncapi" if i and i.mode == "async" else "openapi" if i and re.match(r"(GET|POST|PUT|PATCH|DELETE) /", i.operation) else "other"
            definition = yaml.safe_dump({"capability": cap.id, "version": cap.version, "preconditions": cap.preconditions, "postconditions": cap.postconditions,
                                         "invariants": cap.invariants, "failure_modes": cap.failure_modes, **({"interface": i.model_dump(exclude_defaults=True)} if i else {})}, sort_keys=False)
            docs.append({"apiVersion": "backstage.io/v1alpha1", "kind": "API", "metadata": meta(k, description=f"v{cap.version} {cap.status}"),
                         "spec": {"type": kind, "lifecycle": _LIFECYCLE[cap.status], "owner": owner, "system": n(spec.system.id), "definition": definition}})
        else:
            imp = next((x for x in spec.system.imports if x.id == k.partition(":")[0]), None)
            docs.append({"apiVersion": "backstage.io/v1alpha1", "kind": "API", "metadata": meta(k, description=f"imported from {imp.path if imp else '?'}"),
                         "spec": {"type": "other", "lifecycle": "production", "owner": owner, "system": n(spec.system.id),
                                  "definition": f"imported capability {k} pinned at {_import_cap(spec, k)} from {imp.path if imp else '?'}\n"}})
    for d in spec.data_models:
        docs.append({"apiVersion": "backstage.io/v1alpha1", "kind": "Resource", "metadata": meta(d.id, description=f"v{d.version} {d.classification}"),
                     "spec": {"type": "database" if d.namespace or d.storage_role else "dataset", "owner": owner, "system": n(spec.system.id), "dependsOn": [f"resource:{n(x)}" for x in sorted(d.derived_from)]}})
    return docs

def export_structurizr(spec: Spec) -> str:
    """Structurizr DSL workspace: components as containers of the system (external ones as external software systems), requirements as relationships."""
    ident = lambda s: re.sub(r"[^A-Za-z0-9_]", "_", s) if re.match(r"[A-Za-z_]", s) else "c_" + re.sub(r"[^A-Za-z0-9_]", "_", s)
    res, sid = resolve_capabilities(spec), ident(spec.system.id)
    lines = ["workspace {", "    model {", f'        {sid} = softwareSystem "{spec.system.id}" "v{spec.system.version} {spec.system.status}" {{']
    lines += [f'            {ident(c.id)} = container "{c.id}" "v{c.version} {c.status}; provides {", ".join(c.provides) or "-"}" "{c.layer or ""}"' for c in spec.components if not c.external]
    lines.append("        }")
    lines += [f'        {ident(c.id)} = softwareSystem "{c.id}" "v{c.version} {c.status}; provides {", ".join(c.provides) or "-"}" "External"' for c in spec.components if c.external]
    edges = {}
    for c in spec.components:
        for k, p in res.get(c.id, {}).items(): edges.setdefault((c.id, p), []).append(k)
    lines += [f'        {ident(a)} -> {ident(b)} "requires {", ".join(sorted(ks))}"' for (a, b), ks in sorted(edges.items())]
    lines += ["    }", "    views {", f"        container {sid} {{", "            include *", "            autolayout lr", "        }", "        styles {",
              '            element "External" {', "                background #999999", "            }", "        }", "    }", "}"]
    return "\n".join(lines) + "\n"

def change_plan(spec: Spec, target_id: str) -> dict:
    """Delivery plan for a change to target_id: affected components in provider-first waves (parallel within a wave), their slices' bindings, requirements and evidence to revisit."""
    imp, res = impact_analysis(spec, target_id), resolve_capabilities(spec)
    affected, by = set(imp["components"]), {c.id: c for c in spec.components}
    deps = {c: {p for p in res.get(c, {}).values() if p in affected and p != c} for c in affected}
    waves, done = [], set()
    while done != affected:
        waves.append(wave := sorted(c for c in affected - done if deps[c] <= done) or sorted(affected - done)); done |= set(wave)
    return {"target": target_id, "waves": waves,
            "components": {c: {"requires": sorted(res.get(c, {})), "fakes": by[c].fakes, "source_paths": by[c].source_paths, "test_paths": by[c].test_paths} for c in sorted(affected)},
            "requirements": imp["requirements"], "invalidated_evidence": imp["invalidated_evidence"],
            **({"priorities": pri} if (pri := {r.id: r.priority for r in spec.requirements if r.id in imp["requirements"] and r.priority}) else {})}

def check_scope(spec: Spec, component_id: str, changed: list[str], allow: list[str] = ()) -> list[str]:
    """Path-only handoff scope check (no filesystem access): changed files must lie under the component's source/test paths, fakes, owned schemas or allow entries."""
    c = next(x for x in spec.components if x.id == component_id)
    owned = [*c.source_paths, *(t for t in c.test_paths if not _is_selector(t)), *(f for f in c.fakes.values() if not _is_selector(f)),
             *(d.schema_path for d in spec.data_models if d.owner == c.id and d.schema_path), *allow]
    ok, errors = [PurePosixPath(o.replace("\\", "/")) for o in owned], []
    for raw in changed:
        raw, p = raw.strip(), PurePosixPath(raw.strip().replace("\\", "/"))
        if not raw or p.is_absolute() or PureWindowsPath(raw).drive or ".." in p.parts: errors.append(f"scope: unsafe path '{raw}'")
        elif not any(p == o or o in p.parents for o in ok): errors.append(f"scope: '{raw}' is outside component '{c.id}' (source_paths, test_paths, fakes, owned schemas)")
    return errors

def json_schema() -> dict:
    return {"$schema": "https://json-schema.org/draft/2020-12/schema", **Spec.model_json_schema()}

def fragment_json_schema() -> dict:
    return {"$schema": "https://json-schema.org/draft/2020-12/schema", **SpecFragment.model_json_schema()}

def check_isolation(spec: Spec, root: str | Path, timeout: int = 30) -> list[str]:
    """Opt-in execution of trusted repository harnesses and shared real/mock contract suites."""
    root = Path(root).resolve()
    if not root.is_dir(): return [f"isolation: invalid root '{root}'"]
    errors = []
    def resolve(raw: str):
        p = PurePosixPath(raw.replace("\\\\", "/"))
        if not raw or p.is_absolute() or PureWindowsPath(raw).drive or ".." in p.parts:
            return None
        dest = (root / p).resolve()
        return dest if dest.is_relative_to(root) and dest.is_file() else None
    def run(command, label, mode, component, capability=None, mock_path=None):
        if not command or not all(isinstance(v, str) and v.strip() and "\\x00" not in v for v in command):
            errors.append(f"isolation: {label}: missing or invalid argv"); return
        if any(".." in PurePosixPath(v.replace("\\\\", "/")).parts for v in command):
            errors.append(f"isolation: {label}: unsafe argv"); return
        env = {"PATH": os.environ.get("PATH", ""), "PYTHONPATH": str(root),
               "SYSTEM_SPEC_MODE": mode, "SYSTEM_SPEC_COMPONENT": component}
        if capability: env["SYSTEM_SPEC_CAPABILITY"] = capability
        if mock_path: env["SYSTEM_SPEC_MOCK"] = str(mock_path)
        try:
            result = subprocess.run(command, cwd=root, env=env, timeout=timeout, capture_output=True, text=True, check=False)
            if result.returncode:
                output = (result.stderr or result.stdout).strip()[-1200:]
                errors.append(f"isolation: {label}: exit {result.returncode}: {output}")
        except (OSError, subprocess.TimeoutExpired) as exc:
            errors.append(f"isolation: {label}: {exc}")
    for component in spec.components:
        if component.external: continue
        if not component.standalone_harness:
            errors.append(f"isolation: {component.id}: missing standalone_harness")
        for required in component.requires:
            cap = required.partition("@^")[0]
            path = component.fakes.get(cap)
            if not path or not resolve(path):
                errors.append(f"isolation: {component.id}: missing or invalid dependency substitute for '{cap}'")
        if component.standalone_harness:
            run(component.standalone_harness, f"{component.id} standalone", "standalone", component.id)
        extra = set(component.provided_mocks) - set(component.provides)
        for cap in sorted(extra):
            errors.append(f"isolation: {component.id}: mock of undeclared provided capability '{cap}'")
        for cap in component.provides:
            binding = component.provided_mocks.get(cap)
            if binding is None:
                errors.append(f"isolation: {component.id}: missing provided mock for '{cap}'"); continue
            path = resolve(binding.path)
            if path is None:
                errors.append(f"isolation: {component.id}/{cap}: invalid mock path '{binding.path}'"); continue
            if not binding.contract_suite:
                errors.append(f"isolation: {component.id}/{cap}: missing contract_suite"); continue
            for mode in ("real", "mock"):
                run(binding.contract_suite, f"{component.id}/{cap} {mode} contract",
                    mode, component.id, cap, path)
    return errors

def check_artifacts(spec: Spec, root: str | Path) -> list[str]:
    """Optional local integrity checks; never execute artifacts or infer semantic correctness."""
    root, errors = Path(root).resolve(), []
    if not root.is_dir(): return [f"artifacts: repository root is not a directory: {root}"]
    def check(raw, digest=None, file=False):
        path = Path(raw.replace("\\", "/"))
        if not raw or path.is_absolute() or PureWindowsPath(raw).drive or ".." in path.parts:
            errors.append(f"artifacts: unsafe path '{raw}'"); return
        try:
            target = (root / path).resolve()
            if not target.is_relative_to(root):
                errors.append(f"artifacts: unsafe path '{raw}'"); return
            if not target.exists() or (file and not target.is_file()):
                errors.append(f"artifacts: missing file/path '{raw}'"); return
            if digest is not None:
                if len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
                    errors.append(f"artifacts: invalid SHA-256 for '{raw}'"); return
                with target.open("rb") as f: actual = hashlib.file_digest(f, "sha256").hexdigest()
                if actual != digest: errors.append(f"artifacts: hash mismatch for '{raw}'")
        except (OSError, ValueError): errors.append(f"artifacts: cannot inspect '{raw}'")
    for c in spec.components:
        for path in c.source_paths + [t for t in c.test_paths if not _is_selector(t)]: check(path)
        for path in c.fakes.values():
            if not _is_selector(path): check(path, file=True)
    for d in spec.data_models:
        if d.schema_path: check(d.schema_path, file=True)
    for b in spec.subject_bindings.values():
        for path, digest in b.artifacts.items(): check(path, digest, file=True)
    for r in [r for v in spec.views for r in v.references] + [r for x in spec.experiences for r in x.references]: check(r.path, r.sha256, file=True)
    for e in spec.evidence:
        if e.id not in _superseded(spec):
            if e.provenance:
                prov_file = e.provenance.partition("::")[0].strip()
                if (root / prov_file).exists() or Path(prov_file).suffix:
                    check(prov_file, file=True)
            for path, digest in e.artifacts.items(): check(path, digest, file=True)
    return errors

def _is_selector(ref: str) -> bool: return "::" in ref

def _under_tests(ref: str, test_paths: list[str]) -> bool:
    """True if a provenance/fake reference lies under a path entry, or matches a selector entry (`tool::selector`, exact or followed by `::`)."""
    f = PurePosixPath(ref.partition("::")[0].strip().replace("\\", "/"))
    return any(ref == t or ref.startswith(t + "::") if _is_selector(t) else f == PurePosixPath(t) or PurePosixPath(t) in f.parents for t in test_paths)

_JS_EXT = (".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs")
_JS_IMPORT = re.compile(r"""\b(?:import|export)\b[^'";]*?\bfrom\s*['"]([^'"]+)['"]|\bimport\s*['"]([^'"]+)['"]|\brequire\(\s*['"]([^'"]+)['"]\s*\)|\bimport\(\s*['"]([^'"]+)['"]\s*\)""")

def _js_specifiers(text: str) -> list[str]:
    """Module specifiers of import/export-from/require/dynamic import (comments stripped; heuristic, no JS parser)."""
    text = "\n".join(l for l in re.sub(r"/\*.*?\*/", "", text, flags=re.S).splitlines() if not l.lstrip().startswith("//"))
    return [next(g for g in m.groups() if g) for m in _JS_IMPORT.finditer(text)]

def check_drift(spec: Spec, root: str | Path) -> dict[str, list[str]]:
    """Optional static drift checks (parse only, never execute): Python import and Cargo edges vs requires/provides/bindings, JSON Schema properties vs model fields.
    Unchecked: test selectors, components without source_paths, external components' own sources, runtime-mediated (via) edges, in-root import/path-dependency targets owned by no component."""
    root, errors, unchecked = Path(root).resolve(), [], []
    def inside(raw):
        p = Path(raw.replace("\\", "/"))
        if not raw or p.is_absolute() or PureWindowsPath(raw).drive or ".." in p.parts: return None
        t = (root / p).resolve()
        return t if t.is_relative_to(root) else None
    rel = lambda f: f.relative_to(root).as_posix()
    unchecked += [f"{c.id}: test selector '{t}'" for c in spec.components for t in c.test_paths if _is_selector(t)]
    unchecked += [f"{c.id}: no source_paths" for c in spec.components if not c.source_paths]
    unchecked += [f"{c.id}: external, own sources not scanned" for c in spec.components if c.external and c.source_paths]
    unchecked += [f"{c.id}: '{k}' via '{v}' (runtime-mediated)" for c in spec.components for k, v in c.via.items()]
    paths = {c.id: [t for r in c.source_paths + [x for x in c.test_paths if not _is_selector(x)] if (t := inside(r))] for c in spec.components}
    tests = {c.id: [t for r in c.test_paths if not _is_selector(r) and (t := inside(r))] for c in spec.components}
    def owner(f):
        hits = [(len(t.parts), c) for c, ts in paths.items() for t in ts if f == t or t in f.parents]
        return max(hits)[1] if hits else None
    def find(base, parts):
        for n in range(len(parts), 0, -1):
            t = base.joinpath(*parts[:n])
            for f in (t.with_name(t.name + ".py"), t / "__init__.py"):
                if f.is_file() and f.resolve().is_relative_to(root): return f.resolve()
    provides, allowed = {c.id: set(c.provides) for c in spec.components}, {c.id: {r.partition("@^")[0] for r in c.requires} | set(c.bindings.values()) for c in spec.components}
    seen = set()
    tallowed = {c.id: {r.partition("@^")[0] for r in c.test_requires} for c in spec.components}
    def edge(c, other, is_test, src, dst, what, verb):
        if not other and dst and (c.id, dst) not in seen:
            seen.add((c.id, dst)); unchecked.append(f"{c.id}: '{dst}' not owned by any component")
        if other and other != c.id and (c.id, other, is_test) not in seen:
            seen.add((c.id, other, is_test))
            if is_test and c.test_scope == "unit": errors.append(f"drift: tests of '{c.id}' {verb} '{other}' ({src} -> {dst}); use a fake from fakes instead")
            elif not ((ok := allowed[c.id] | (tallowed[c.id] if is_test else set())) & provides[other] or other in ok):
                errors.append(f"drift: {'tests of' if is_test else 'component'} '{c.id}' {verb if is_test else what} '{other}' ({src} -> {dst}) without a required capability or binding provided by '{other}'")
    for c in spec.components:
        if c.external: continue
        files = [f for t in paths[c.id] for f in ([t] if t.is_file() else sorted(t.rglob("*.py"))) if f.suffix == ".py" and f.resolve().is_relative_to(root)]
        for f in files:
            try: tree = ast.parse(f.read_text(encoding="utf-8"))
            except (OSError, SyntaxError, ValueError): errors.append(f"drift: cannot parse '{rel(f)}'"); continue
            for n in ast.walk(tree):
                if isinstance(n, ast.Import): cands = [(b, a.name.split(".")) for a in n.names for b in (root, root / "src")]
                elif isinstance(n, ast.ImportFrom):
                    mod = n.module.split(".") if n.module else []
                    bases = [f.parent.joinpath(*[".."] * (n.level - 1))] if n.level else [root, root / "src"]
                    cands = [(b, mod + x) for b in bases for x in ([], *[[a.name] for a in n.names])]
                else: continue
                for base, parts in cands:
                    target = find(base.resolve(), [p for p in parts if p != ".."]) if parts else None
                    other = target and owner(target)
                    is_test = any(f.resolve() == t or t in f.resolve().parents for t in tests[c.id])
                    edge(c, other, is_test, rel(f), target and rel(target), "imports", "import")
        ws = {}
        if (root / "Cargo.toml").is_file():
            try: ws = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8")).get("workspace", {}).get("dependencies", {})
            except (OSError, ValueError): pass
        for manifest in sorted({m for t in paths[c.id] if t.is_dir() for m in t.rglob("Cargo.toml") if "target" not in m.relative_to(t).parts}):
            try: toml = tomllib.loads(manifest.read_text(encoding="utf-8"))
            except (OSError, ValueError): errors.append(f"drift: cannot parse '{rel(manifest)}'"); continue
            for section, is_test in (("dependencies", False), ("build-dependencies", False), ("dev-dependencies", True)):
                for name, dep in toml.get(section, {}).items():
                    if isinstance(dep, dict) and dep.get("workspace"): dep = ws.get(name)
                    if isinstance(dep, dict) and isinstance(dep.get("path"), str):
                        base = root if dep is ws.get(name) else manifest.parent
                        target = (base / dep["path"]).resolve()
                        if target.is_relative_to(root) and not (is_test and (c.id, owner(target), False) in seen):
                            edge(c, owner(target), is_test, rel(manifest), rel(target), "depends on", "depend on")
    pkgs = {}
    for ts in paths.values():
        for t in ts:
            for pj in ([t] if t.name == "package.json" else sorted(t.rglob("package.json")) if t.is_dir() else []):
                if "node_modules" in pj.parts: continue
                try: pkgs[json.loads(pj.read_text(encoding="utf-8"))["name"]] = pj.parent.resolve()
                except (OSError, ValueError, KeyError, TypeError): pass
    def js_find(base, name):
        t = base / name
        for f in (t, *(t.with_name(t.name + e) for e in _JS_EXT), *(t / f"index{e}" for e in _JS_EXT)):
            if f.is_file() and f.resolve().is_relative_to(root): return f.resolve()
    for c in spec.components:
        if c.external: continue
        for f in sorted({f for t in paths[c.id] for f in ([t] if t.is_file() else t.rglob("*")) if f.suffix in _JS_EXT and "node_modules" not in f.parts and f.is_file() and f.resolve().is_relative_to(root)}):
            try: specs = _js_specifiers(f.read_text(encoding="utf-8"))
            except (OSError, ValueError): errors.append(f"drift: cannot parse '{rel(f)}'"); continue
            is_test = any(f.resolve() == t or t in f.resolve().parents for t in tests[c.id])
            for sp in specs:
                target = None
                if sp.startswith("."): target = js_find(f.parent, sp)
                elif (pn := max((n for n in pkgs if sp == n or sp.startswith(n + "/")), key=len, default=None)):
                    sub_ = sp[len(pn):].lstrip("/")
                    target = (js_find(pkgs[pn], sub_) if sub_ else None) or pkgs[pn]
                if target: edge(c, owner(target), is_test, rel(f), rel(target), "imports", "import")
    for d in spec.data_models:
        if not d.schema_path: continue
        t = inside(d.schema_path)
        if t is None or not t.is_file(): errors.append(f"drift: cannot read schema '{d.schema_path}' for '{d.id}'"); continue
        if t.suffix != ".json": unchecked.append(d.schema_path); continue
        try: props = json.loads(t.read_text(encoding="utf-8")).get("properties")
        except (OSError, ValueError, AttributeError): props = None
        if not isinstance(props, dict): errors.append(f"drift: schema '{d.schema_path}' for '{d.id}' has no top-level properties"); continue
        if missing := sorted(set(d.fields) - set(props)): errors.append(f"drift: fields of '{d.id}' missing from schema '{d.schema_path}': {', '.join(missing)}")
        if extra := sorted(set(props) - set(d.fields)): errors.append(f"drift: properties of schema '{d.schema_path}' undeclared in model '{d.id}': {', '.join(extra)}")
    return {"errors": errors, "unchecked": unchecked}

class IncludeError(Exception): pass

def _merge_includes(path: Path, data: dict) -> dict:
    base, patterns = path.parent.resolve(), data["include"]
    if not isinstance(patterns, list) or not all(isinstance(p, str) for p in patterns): raise IncludeError("include must be a list of strings")
    out, seen = {k: (list(v) if isinstance(v, list) else dict(v) if isinstance(v, dict) else v) for k, v in data.items()}, {path.resolve()}
    out["include"] = []
    for pat in patterns:
        p = Path(pat.replace("\\", "/"))
        if not pat or p.is_absolute() or PurePosixPath(p.as_posix()).is_absolute() or PureWindowsPath(pat).drive or ".." in p.parts: raise IncludeError(f"include: unsafe pattern '{pat}'")
        files = sorted(f for f in base.glob(p.as_posix()) if f.is_file())
        if not files and not any(ch in pat for ch in "*?["): raise IncludeError(f"include: missing file '{pat}'")
        for f in files:
            if not f.resolve().is_relative_to(base): raise IncludeError(f"include: '{pat}' resolves outside the spec directory")
            if f.resolve() in seen: continue
            seen.add(f.resolve()); name = f.relative_to(base).as_posix()
            try: frag = SpecFragment.model_validate(yaml.safe_load(f.read_text(encoding="utf-8")) or {})
            except (OSError, yaml.YAMLError, ValidationError) as e: raise IncludeError(f"{name}: {e}") from e
            for k in SpecFragment.model_fields:
                v = getattr(frag, k)
                if isinstance(v, list): out[k] = out.get(k, []) + [x.model_dump() for x in v]
                elif isinstance(v, dict):
                    dup = sorted(set(out.get(k, {})) & set(v))
                    if dup: raise IncludeError(f"{name}: duplicate {k} {dup}")
                    out[k] = {**out.get(k, {}), **{i: x.model_dump() for i, x in v.items()}}
    return out

def load_spec(path: str | Path) -> Spec:
    path = Path(path); data = yaml.safe_load(path.read_text(encoding="utf-8"))
    if isinstance(data, dict) and data.get("include"): data = _merge_includes(path, data)
    return Spec.model_validate(data)

def package_version(dist_name: str = "system-spec-ref") -> str:
    try:
        from importlib.metadata import version as _dist_version
        return _dist_version(dist_name)
    except Exception:
        return "0.0.0+unknown"

def main(argv: list[str] | None = None, default_spec: str | Path | None = None, version: str | None = None) -> int:
    ap = argparse.ArgumentParser(description="Validate system specifications against the metamodel.")
    ap.add_argument("spec", nargs="?", default=default_spec or Path(__file__).parent / "example.yaml")
    ap.add_argument("--schema", action="store_true", help="print JSON Schema (committed as schema.json)")
    ap.add_argument("--fragment-schema", action="store_true", help="print JSON Schema for include fragments (committed as fragment.schema.json)")
    ap.add_argument("--stage", choices=STAGES, help="validation stage (default: release); with --check-* it also runs the stage")
    ap.add_argument("--compat", metavar="OLD", help="check compatibility of OLD spec's capabilities and data models with spec")
    ap.add_argument("--impact", metavar="ID", help="change-impact analysis for ID")
    ap.add_argument("--evidence-report", action="store_true", help="per requirement version: verified, failing, uncovered, blanket and skipped evidence")
    ap.add_argument("--release-scope", metavar="P0[,P1]", help="priorities in scope at release; overrides system.release_scope; other requirements are reported deferred")
    ap.add_argument("--context", metavar="COMPONENT", help="print the component's spec slice")
    ap.add_argument("--diagram", action="store_true", help="print a Mermaid diagram of the system")
    ap.add_argument("--json", action="store_true", help="JSON output")
    ap.add_argument("--check-artifacts", action="store_true", help="check declared source/schema/evidence files and hashes")
    ap.add_argument("--check-drift", action="store_true", help="check Python imports and JSON Schema properties against the spec")
    ap.add_argument("--check-isolation", action="store_true", help="execute opt-in standalone harnesses and shared real/mock capability suites (runs project code)")
    ap.add_argument("--return-schema", action="store_true", help="print JSON Schema for a handoff return (committed as handoff-return.schema.json)")
    ap.add_argument("--check-return", metavar="FILE", help="check an implementer's handoff return (YAML/JSON) against the spec; --allow permits extra changed paths")
    ap.add_argument("--check-imports", action="store_true", help="verify pinned imports against the imported specs (paths resolve from this spec's directory, inside --root)")
    ap.add_argument("--workspace", action="store_true", help="treat the positional argument as a workspace manifest: validate every member at --stage (default release) and check imports, links and cycles across members (offline)")
    ap.add_argument("--check-report", metavar="JUNIT", action="append", default=[], help="check JUnit run reports (repeatable): every test binds one requirement version via a 'verifies' property; test evidence agrees with the reports")
    ap.add_argument("--emit-evidence", metavar="JUNIT", action="append", default=[], help="print test Evidence records derived from JUnit run reports (repeatable)")
    ap.add_argument("--export", choices=["backstage", "structurizr"], help="print a Backstage catalog (YAML) or Structurizr DSL workspace for the spec")
    ap.add_argument("--owner", default="unknown", help="owner for --export backstage (Backstage requires one; not modeled)")
    ap.add_argument("--plan", metavar="ID", help="delivery plan for a change to ID: provider-first component waves, slices, evidence to revisit")
    ap.add_argument("--check-scope", metavar="COMPONENT", help="check --changed files stay inside COMPONENT's paths (handoff scope)")
    ap.add_argument("--changed", metavar="FILE", help="file listing changed paths, one per line ('-' for stdin), e.g. from git diff --name-only")
    ap.add_argument("--allow", metavar="PATH", action="append", default=[], help="extra path allowed by --check-scope (repeatable)")
    ap.add_argument("--root", type=Path, help="explicit repository root for --check-artifacts / --check-drift")
    ap.add_argument("--version", action="store_true", help="print installed version and exit")
    a = ap.parse_args(argv)
    if a.version:
        print(version or package_version()); return 0
    if a.check_scope and a.changed is None: ap.error("--check-scope requires --changed")
    if (a.check_artifacts or a.check_drift or a.check_imports or a.check_isolation) and a.root is None: ap.error("--check-artifacts, --check-drift, --check-imports and --check-isolation require --root")
    if a.workspace:
        errs = check_workspace(a.spec, a.stage or "release")
        out = {"valid": not errs, "errors": errs}
        print(json.dumps(out, indent=2) if a.json else "VALID" if not errs else "\n".join(errs)); return 0 if not errs else 1
    if a.schema:
        print(json.dumps(json_schema(), indent=2)); return 0
    if a.return_schema:
        print(json.dumps(return_json_schema(), indent=2)); return 0
    if a.fragment_schema:
        print(json.dumps(fragment_json_schema(), indent=2)); return 0
    try:
        spec = load_spec(a.spec)
        old = load_spec(a.compat) if a.compat else None
    except (OSError, yaml.YAMLError, ValidationError, IncludeError) as e:
        out = {"valid": False, "schema_errors": [str(e)]}
    else:
        known = {spec.system.id, *(x.id for x in spec.capabilities + spec.components + spec.data_models + spec.requirements)}
        scope = [s.strip() for s in a.release_scope.split(",") if s.strip()] if a.release_scope else None
        checks = a.check_artifacts or a.check_isolation or a.check_drift or a.check_imports
        if checks and (a.stage or old):
            errs = validate_spec(spec, a.stage or "release", release_scope=scope) + (compare_specs(old, spec) if old else [])
            extra = {}
            if a.check_artifacts: errs += check_artifacts(spec, a.root)
            if a.check_isolation: errs += check_isolation(spec, a.root)
            if a.check_drift: drift = check_drift(spec, a.root); errs += drift["errors"]; extra = {k: v for k, v in drift.items() if k != "errors"}
            if a.check_imports: errs += check_imports(spec, a.root, Path(a.spec).parent)
            out = {"valid": not errs, "stage": a.stage or "release", "errors": errs, **extra}
        elif (artifact_errors := check_artifacts(spec, a.root) if a.check_artifacts else []): out = {"valid": False, "errors": artifact_errors}
        elif a.check_isolation:
            errs = check_isolation(spec, a.root)
            out = {"valid": not errs, "errors": errs}
        elif a.check_drift:
            drift = check_drift(spec, a.root)
            out = {"valid": not drift["errors"], **drift}
        elif a.context:
            if a.context not in {c.id for c in spec.components}: out = {"valid": False, "errors": [f"Unknown --context component '{a.context}'"]}
            else:
                sl = context_slice(spec, a.context)
                print(json.dumps(sl, indent=2) if a.json else yaml.safe_dump(sl, sort_keys=False, default_flow_style=None), end=""); return 0
        elif a.export:
            print(yaml.safe_dump_all(export_backstage(spec, a.owner), sort_keys=False) if a.export == "backstage" else export_structurizr(spec), end=""); return 0
        elif a.check_report or a.emit_evidence:
            try:
                if a.emit_evidence:
                    ev = evidence_from_report(spec, a.emit_evidence)
                    print(json.dumps(ev, indent=2) if a.json else yaml.safe_dump(ev, sort_keys=False, default_flow_style=None), end=""); return 0
                errs = check_report(spec, a.check_report)
            except (OSError, ET.ParseError) as e: errs = [f"report: {e}"]
            out = {"valid": not errs, "errors": errs}
        elif a.check_imports:
            errs = check_imports(spec, a.root, Path(a.spec).parent)
            out = {"valid": not errs, "errors": errs}
        elif a.check_return:
            try: ret = HandoffReturn.model_validate(yaml.safe_load(Path(a.check_return).read_text(encoding="utf-8")))
            except (OSError, yaml.YAMLError, ValidationError) as e: out = {"valid": False, "errors": [f"return: {e}"]}
            else:
                errs = check_return(spec, ret, a.allow)
                out = {"valid": not errs, "errors": errs, "findings": [f.model_dump() for f in ret.findings]}
        elif a.plan:
            if a.plan not in known: out = {"valid": False, "errors": [f"Unknown --plan ID '{a.plan}'"]}
            else:
                plan = change_plan(spec, a.plan)
                print(json.dumps(plan, indent=2) if a.json else yaml.safe_dump(plan, sort_keys=False, default_flow_style=None), end=""); return 0
        elif a.check_scope:
            if a.check_scope not in {c.id for c in spec.components}: out = {"valid": False, "errors": [f"Unknown --check-scope component '{a.check_scope}'"]}
            else:
                try: lines = (sys.stdin.read() if a.changed == "-" else Path(a.changed).read_text(encoding="utf-8")).splitlines()
                except OSError as e: lines, out = None, {"valid": False, "errors": [f"cannot read --changed: {e}"]}
                if lines is not None:
                    errs = check_scope(spec, a.check_scope, [l for l in lines if l.strip()], a.allow)
                    out = {"valid": not errs, "errors": errs}
        elif a.evidence_report:
            rep = evidence_report(spec)
            print(json.dumps(rep, indent=2) if a.json else yaml.safe_dump(rep, sort_keys=False, default_flow_style=None), end=""); return 0
        elif a.diagram: print(diagram(spec)); return 0
        elif a.impact: out = impact_analysis(spec, a.impact) if a.impact in known else {"valid": False, "errors": [f"Unknown --impact ID '{a.impact}'"]}
        else:
            stage = a.stage or "release"
            errs = validate_spec(spec, stage, release_scope=scope) + (compare_specs(old, spec) if old else [])
            out = {"valid": not errs, "stage": stage, "errors": errs, "residual_risks": residual_risks(spec),
                   "waivers": [k.id for k in active_waivers(spec, stage)],
                   "omitted_categories": [c for c in CATEGORIES if spec.system.verification_categories and c not in spec.system.verification_categories]}
            if stage == "release" and (d := deferred_scope(spec, scope))["requirements"]: out["deferred"] = d
    if a.json: print(json.dumps(out, indent=2))
    elif "valid" not in out: print("\n".join(f"{k}: {', '.join(v)}" for k, v in out.items()))
    else:
        print("VALID" if out["valid"] else "\n".join(out.get("errors") or out["schema_errors"]))
        if out.get("omitted_categories"): print(f"note: verification categories not required by this project: {', '.join(out['omitted_categories'])}")
        if out.get("deferred"): print(f"note: deferred by release scope (not passed): {len(out['deferred']['requirements'])} requirements, {len(out['deferred']['risks'])} risks, entities {', '.join(out['deferred']['entities'])}")
    return 0 if out.get("valid", True) else 1

if __name__ == "__main__":
    sys.exit(main())
