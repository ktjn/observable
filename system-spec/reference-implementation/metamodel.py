from __future__ import annotations
import argparse, ast, hashlib, json, sys, yaml
from collections import Counter
from datetime import date, datetime
from pathlib import Path, PureWindowsPath
from typing import Literal
from pydantic import BaseModel, ConfigDict, Field, ValidationError

RANK = {"public": 0, "internal": 1, "confidential": 2, "restricted": 3}
STATUS = ["draft", "specified", "verified", "deprecated"]
STAGES = ["inventory", "discovery", "architecture", "verification", "release"]

# Invariant ID: (stage, waiver kind, rule); mirrored in spec.md § 11. COMPAT rules are checked by --compat.
INVARIANTS = {
    "REF-01": ("inventory", None, "IDs are unique within each entity type."),
    "REF-02": ("inventory", None, "Requirement and risk targets resolve to exactly one system, component, capability or data model."),
    "REF-03": ("inventory", None, "Provided capabilities exist."),
    "REF-04": ("inventory", None, "Owned data models exist."),
    "REF-05": ("inventory", None, "Bindings name required capabilities."),
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
    "REQ-01": ("verification", "verification-category", "Active requirements cover all verification categories."),
    "REQ-02": ("verification", None, "Requirement dependencies and refinements are acyclic."),
    "REQ-03": ("verification", None, "Requirement conflicts are resolved by an active requirement."),
    "REQ-04": ("release", None, "Obligated requirements have no unsuperseded failing evidence for their version."),
    "REQ-05": ("release", None, "Obligated (mandatory, declassifying, resolving) requirements have passing evidence for their version."),
    "REQ-06": ("release", None, "Dependencies of obligated and mitigating requirements are verified."),
    "REQ-07": ("verification", None, "Requirement kind matches its verification category."),
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

class SystemDef(M):
    """System: composition of components (§ 2)."""
    id: str = ID
    version: str = VER
    status: Status = ST
    layers: list[str] = F("Layers, highest first; dependencies point downward (§ 2).", [])
    external_sources: list[str] = F("Registered external data sources (§ 5).", [])
    quality_attributes: dict[str, str] = F("Measurable quality attributes (§ 9).", {})
    context_budget: Budget = F("Component size limits (§ 3).", Budget())

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
    preconditions: list[str] = F("Contract preconditions.", [])
    postconditions: list[str] = F("Contract postconditions.", [])
    invariants: list[str] = F("Contract invariants.", [])
    failure_modes: list[str] = F("Named failure modes.", [])
    interface: Interface | None = F("Interface contract; required when another component requires this capability.", None)

class Component(M):
    """Independent logical unit providing capabilities (§ 3)."""
    id: str = ID
    version: str = VER
    status: Status = ST
    layer: str | None = F("One of system.layers.", None)
    consistency: Literal["strong", "eventual"] = F("Consistency guarantee.", "strong")
    provides: list[str] = F("Provided capability IDs.", [])
    requires: list[str] = F("Required capability IDs or 'id@^version' (§ 10).", [])
    owns_data: list[str] = F("Owned DataModel IDs.", [])
    bindings: dict[str, str] = F("Required capability ID -> provider component ID.", {})
    source_paths: list[str] = F("Implementation directories or modules (§ 3).", [])
    context_budget: dict[Literal["max_provides", "max_requires", "max_owns_data"], int] = F("Per-component override of system.context_budget limits (§ 3).", {})

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

class Requirement(M):
    """Expected behavior or constraint with acceptance criteria (§ 6)."""
    id: str = ID
    version: str = VER
    target: str = F("System, component, capability or data model ID.")
    status: Status = F("Lifecycle status; 'deprecated' requirements are inactive (§ 6).", "draft")
    kind: Literal["functional", "non-functional"] = F("Property kind.", "functional")
    category: Literal[tuple(CATEGORIES)] | None = F("Verification category; all 10 must be covered (§ 11).", None)
    mandatory: bool = F("Mandatory requirements need evidence at release.", True)
    criteria: list[str] = F("Acceptance criteria.")
    depends_on: list[str] = F("Prerequisite requirement IDs.", [])
    refines: list[str] = F("Requirement IDs whose obligations this elaborates.", [])
    conflicts_with: list[str] = F("Incompatible requirement IDs.", [])
    resolved_by: str | None = F("Active requirement resolving this requirement's conflicts.", None)

class Risk(M):
    """Risk or tracked architectural exception (§ 7)."""
    id: str = ID
    target: str = F("System, component, capability or data model ID.")
    treatment: Literal["mitigation", "acceptance", "avoidance", "transfer"] = F("Risk treatment.")
    mitigated_by: list[str] = F("Mitigating or remediation requirement IDs; need evidence at release.", [])
    residual_accepted: bool = F("Residual risk explicitly accepted.", False)
    transferred_to: str | None = F("Counterparty for 'transfer'.", None)
    waives: Literal["layer-violation", "dependency-cycle", "shared-data-ownership", "context-budget", "verification-category",
                    "DATA-03", "COMP-03", "COMP-05", "COMP-06", "REQ-01"] | None = F("Waived violation kind or invariant ID (below release).", None)
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
    supersedes: str | None = F("Failed evidence ID this record supersedes.", None)
    supersession_reason: str | None = F("Required with supersedes.", None)

class SubjectBinding(M):
    """Reviewed contract/source binding; optional file checks establish artifact integrity."""
    version: str = VER
    revision: str = F("Reviewed source revision; distinct from contract version.")
    artifacts: dict[str, str] = Field(description="Repository-relative source files mapped to exact-byte SHA-256 digests.", min_length=1)

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

def _ver(v: str) -> tuple[int, ...] | None:
    try: return tuple(int(x) for x in v.split("."))
    except ValueError: return None

def _ver_ok(req: str | None, act: str) -> bool:
    if not req: return True
    r, a = _ver(req), _ver(act)
    return bool(r and a) and r[0] == a[0] and a[1:] >= r[1:]

def _resolve(spec: Spec, c: Component, ref: str) -> tuple[str | None, str | None]:
    k, _, v = ref.partition("@^")
    cap = next((x for x in spec.capabilities if x.id == k), None)
    ps = [p.id for p in spec.components if k in p.provides and p.id != c.id]
    if k in c.bindings: ps = [p for p in ps if p == c.bindings[k]]
    if not cap or not _ver_ok(v, cap.version) or not ps: return None, f"unresolved capability '{ref}'"
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

def verified_requirements(spec: Spec) -> set[tuple[str, str]]:
    ev: dict[tuple[str, str], list[bool]] = {}
    sup = _superseded(spec)
    for e in spec.evidence:
        if e.id not in sup: ev.setdefault((e.requirement_id, e.requirement_version), []).append(e.passed and not e.skipped)
    return {k for k, v in ev.items() if all(v)}

def _expired(k: Risk, stage: str, today: date | None) -> bool:
    return k.expires < (today or date.today()) if isinstance(k.expires, date) else STAGES.index(k.expires) < STAGES.index(stage)

def active_waivers(spec: Spec, stage: str = "release", today: date | None = None) -> list[Risk]:
    return [k for k in spec.risks if k.waives and k.treatment in ("mitigation", "acceptance") and k.mitigated_by and k.expires
            and stage != "release" and not _expired(k, stage, today)]

def _list(xs: str | list[str] | None) -> list[str]:
    return xs if isinstance(xs, list) else [xs] if xs else []

def validate_spec(spec: Spec, stage: str = "release", today: date | None = None) -> list[str]:
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
    evs = {e.id: e for e in spec.evidence}
    sid, sup, verified = spec.system.id, _superseded(spec), verified_requirements(spec)
    ok = {r.id for r in active.values() if (r.id, r.version) in verified}
    failed = {(e.requirement_id, e.requirement_version) for e in spec.evidence if not e.passed and e.id not in sup}
    targets = {"system": {sid}, "component": set(comps), "capability": set(caps), "data model": set(models)}
    required = {r.partition("@^")[0] for c in spec.components for r in c.requires}
    vers = {x.id: x.version for x in [spec.system, *spec.capabilities, *spec.components, *spec.data_models]}

    # inventory: identity and references
    for kind, xs in [("capability", spec.capabilities), ("component", spec.components), ("data model", spec.data_models),
                     ("requirement", spec.requirements), ("risk", spec.risks), ("evidence", spec.evidence)]:
        for i, n in Counter(x.id for x in xs).items():
            if n > 1: add("REF-01", f"Duplicate {kind} id '{i}'")
    for kind, xs in [("Requirement", spec.requirements), ("Risk", spec.risks)]:
        for x in xs:
            m = [t for t, ids in targets.items() if x.target in ids]
            if len(m) != 1: add("REF-02", f"{kind} '{x.id}' has {'ambiguous' if m else 'unknown'} target '{x.target}'")
    for c in spec.components:
        for p in c.provides:
            if p not in caps: add("REF-03", f"Component '{c.id}' provides unknown capability '{p}'")
        for d in c.owns_data:
            if d not in models: add("REF-04", f"Component '{c.id}' owns unknown data model '{d}'")
        for k in c.bindings:
            if k not in {r.partition("@^")[0] for r in c.requires}: add("REF-05", f"Component '{c.id}' binds capability '{k}' it does not require")
    for r in spec.requirements:
        for rel in ("depends_on", "refines", "conflicts_with", "resolved_by"):
            for x in _list(getattr(r, rel)):
                if x not in reqs: add("REF-06", f"Requirement '{r.id}' {rel} unknown requirement '{x}'")
    for e in spec.evidence:
        if e.requirement_id not in reqs: add("REF-07", f"Evidence '{e.id}' references unknown requirement '{e.requirement_id}'")
        if e.method == "model-check" and not e.bounds: add("EV-01", f"Evidence '{e.id}' model-check requires bounds")
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
        if layers and c.layer not in layers: add("COMP-02", f"Component '{c.id}' has undeclared layer '{c.layer}'")
        for f in ("provides", "requires", "owns_data"):
            n = c.context_budget.get(f"max_{f}", getattr(b, f"max_{f}"))
            if len(getattr(c, f)) > n: add("COMP-03", f"Component '{c.id}' exceeds context budget: {f} > {n}", c.id)
        for ref in c.requires:
            k = ref.partition("@^")[0]
            if c.status != "deprecated" and k in caps and caps[k].status == "deprecated":
                add("COMP-07", f"Component '{c.id}' requires deprecated capability '{k}'")
            p, err = _resolve(spec, c, ref)
            if err: add("COMP-04", f"Component '{c.id}' has {err}")
            elif c.layer in layers and comps[p].layer in layers and layers[c.layer] > layers[comps[p].layer]:
                add("COMP-05", f"Component '{c.id}' has layer violation depending on '{p}'", c.id, p)
    for cy in _cycles({k: list(v.values()) for k, v in resolved.items()}):
        add("COMP-06", f"Dependency cycle: {' -> '.join(cy)}", *cy)
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
    missing = [c for c in CATEGORIES if c not in {r.category for r in active.values()}]
    if missing: add("REQ-01", f"Missing verification categories: {missing}", sid)
    for cy in _cycles({r.id: [*r.depends_on, *r.refines] for r in spec.requirements}):
        add("REQ-02", f"Requirement cycle: {' -> '.join(cy)}")
    for r in active.values():
        if r.category and (r.kind == "non-functional") != (r.category in CATEGORIES[5:]):
            add("REQ-07", f"Requirement '{r.id}' kind '{r.kind}' does not match category '{r.category}'")
        for x in r.conflicts_with:
            if x in active and r.resolved_by not in active and active[x].resolved_by not in active:
                add("REQ-03", f"Requirement '{r.id}' has unresolved conflict with '{x}'")

    # release: status, evidence, risk disposition, no waivers
    need("STAT-04", "verified", [("System", [spec.system]), ("Capability", spec.capabilities), ("Component", spec.components)])
    need("STAT-04", "verified", [("Requirement", active.values())])
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
    for r in active.values():
        if r.id in oblig and (r.id, r.version) in failed: add("REQ-04", f"Requirement '{r.id}@{r.version}' has unresolved failing evidence")
        elif r.id in oblig and r.id not in ok: add("REQ-05", f"Requirement '{r.id}@{r.version}' missing valid verification evidence")
        if r.id in oblig | mitig:
            for d in r.depends_on:
                if d in reqs and d not in ok: add("REQ-06", f"Requirement '{r.id}' depends on unverified requirement '{d}'")
    for k in spec.risks:
        if k.treatment in ("mitigation", "avoidance") and not k.mitigated_by: add("RISK-03", f"Risk '{k.id}' has no mitigating requirements")
        for m in k.mitigated_by:
            if m in reqs and m not in ok: add("RISK-04", f"Risk '{k.id}' has unverified mitigation '{m}'")
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
    """Mermaid flowchart: components grouped by layer, resolved capability dependencies, data ownership/lineage, status styling."""
    mid = lambda s: "".join(c if c.isalnum() else "_" for c in s)
    node = lambda x: f'{mid(x.id)}["{x.id} v{x.version}<br>{x.status}"]'
    out, placed = ["graph LR", f'  subgraph {mid(spec.system.id)}["{spec.system.id} v{spec.system.version} ({spec.system.status})"]'], set()
    for layer in spec.system.layers:
        if cs := [c for c in spec.components if c.layer == layer]:
            out += [f'    subgraph {mid(layer)}["{layer}"]', *[f"      {node(c)}" for c in cs], "    end"]
            placed |= {c.id for c in cs}
    out += [f"    {node(c)}" for c in spec.components if c.id not in placed]
    out += [f'    {mid(d.id)}[("{d.id}<br>{d.classification}")]' for d in spec.data_models] + ["  end"]
    resolved = resolve_capabilities(spec)
    for c in spec.components:
        out += [f"  {mid(c.id)} -->|{k}| {mid(p)}" for k, p in resolved.get(c.id, {}).items()]
        out += [f"  {mid(c.id)} -.->|owns| {mid(d)}" for d in c.owns_data]
    for d in spec.data_models:
        out += [f"  {mid(d.id)} -.->|derived_from| {mid(x)}" for x in d.derived_from]
    colors = {"draft": "#eee,stroke:#999,color:#333", "specified": "#cce5ff,stroke:#36c", "verified": "#d5f5d5,stroke:#393", "deprecated": "#fdd,stroke:#c33,stroke-dasharray:4"}
    out += [f"  classDef {s} fill:{v}" for s, v in colors.items()]
    out += [f"  class {','.join(mid(c.id) for c in spec.components if c.status == s)} {s}" for s in STATUS if any(c.status == s for c in spec.components)]
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

def json_schema() -> dict:
    return {"$schema": "https://json-schema.org/draft/2020-12/schema", **Spec.model_json_schema()}

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
        for path in c.source_paths: check(path)
    for d in spec.data_models:
        if d.schema_path: check(d.schema_path, file=True)
    for b in spec.subject_bindings.values():
        for path, digest in b.artifacts.items(): check(path, digest, file=True)
    for e in spec.evidence:
        if e.id not in _superseded(spec):
            if e.provenance:
                prov_file = e.provenance.partition("::")[0].strip()
                if (root / prov_file).exists() or Path(prov_file).suffix:
                    check(prov_file, file=True)
            for path, digest in e.artifacts.items(): check(path, digest, file=True)
    return errors

def check_drift(spec: Spec, root: str | Path) -> dict[str, list[str]]:
    """Optional static drift checks (parse only, never execute): Python import edges vs requires/provides/bindings, JSON Schema properties vs model fields."""
    root, errors, unchecked = Path(root).resolve(), [], []
    def inside(raw):
        p = Path(raw.replace("\\", "/"))
        if not raw or p.is_absolute() or PureWindowsPath(raw).drive or ".." in p.parts: return None
        t = (root / p).resolve()
        return t if t.is_relative_to(root) else None
    rel = lambda f: f.relative_to(root).as_posix()
    paths = {c.id: [t for r in c.source_paths if (t := inside(r))] for c in spec.components}
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
    for c in spec.components:
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
                    if other and other != c.id and (c.id, other) not in seen:
                        seen.add((c.id, other))
                        if not (allowed[c.id] & provides[other] or other in allowed[c.id]):
                            errors.append(f"drift: component '{c.id}' imports '{other}' ({rel(f)} -> {rel(target)}) without a required capability or binding provided by '{other}'")
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

def load_spec(path: str | Path) -> Spec:
    return Spec.model_validate(yaml.safe_load(Path(path).read_text(encoding="utf-8")))

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
    ap.add_argument("--stage", choices=STAGES, default="release", help="validation stage (default: release)")
    ap.add_argument("--compat", metavar="OLD", help="check compatibility of OLD spec's capabilities and data models with spec")
    ap.add_argument("--impact", metavar="ID", help="change-impact analysis for ID")
    ap.add_argument("--context", metavar="COMPONENT", help="print the component's spec slice")
    ap.add_argument("--diagram", action="store_true", help="print a Mermaid diagram of the system")
    ap.add_argument("--json", action="store_true", help="JSON output")
    ap.add_argument("--check-artifacts", action="store_true", help="check declared source/schema/evidence files and hashes")
    ap.add_argument("--check-drift", action="store_true", help="check Python imports and JSON Schema properties against the spec")
    ap.add_argument("--root", type=Path, help="explicit repository root for --check-artifacts / --check-drift")
    ap.add_argument("--version", action="store_true", help="print installed version and exit")
    a = ap.parse_args(argv)
    if a.version:
        print(version or package_version()); return 0
    if (a.check_artifacts or a.check_drift) and a.root is None: ap.error("--check-artifacts and --check-drift require --root")
    if a.schema:
        print(json.dumps(json_schema(), indent=2)); return 0
    try:
        spec = load_spec(a.spec)
        old = load_spec(a.compat) if a.compat else None
    except (OSError, yaml.YAMLError, ValidationError) as e:
        out = {"valid": False, "schema_errors": [str(e)]}
    else:
        known = {spec.system.id, *(x.id for x in spec.capabilities + spec.components + spec.data_models + spec.requirements)}
        artifact_errors = check_artifacts(spec, a.root) if a.check_artifacts else []
        if artifact_errors: out = {"valid": False, "errors": artifact_errors}
        elif a.check_drift:
            drift = check_drift(spec, a.root)
            out = {"valid": not drift["errors"], **drift}
        elif a.context:
            if a.context not in {c.id for c in spec.components}: out = {"valid": False, "errors": [f"Unknown --context component '{a.context}'"]}
            else:
                sl = context_slice(spec, a.context)
                print(json.dumps(sl, indent=2) if a.json else yaml.safe_dump(sl, sort_keys=False, default_flow_style=None), end=""); return 0
        elif a.diagram: print(diagram(spec)); return 0
        elif a.impact: out = impact_analysis(spec, a.impact) if a.impact in known else {"valid": False, "errors": [f"Unknown --impact ID '{a.impact}'"]}
        else:
            errs = validate_spec(spec, a.stage) + (compare_specs(old, spec) if old else [])
            out = {"valid": not errs, "stage": a.stage, "errors": errs, "residual_risks": residual_risks(spec),
                   "waivers": [k.id for k in active_waivers(spec, a.stage)]}
    if a.json: print(json.dumps(out, indent=2))
    elif "valid" not in out: print("\n".join(f"{k}: {', '.join(v)}" for k, v in out.items()))
    else: print("VALID" if out["valid"] else "\n".join(out.get("errors") or out["schema_errors"]))
    return 0 if out.get("valid", True) else 1

if __name__ == "__main__":
    sys.exit(main())
