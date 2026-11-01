// Alloy formalization of the system specification metamodel (spec.md §§ 2-11; PLAN.md Milestone 5 step 5).
module metamodel

abstract sig Status {}
one sig Draft, Specified, Verified, Deprecated extends Status {}

abstract sig Treatment {}
one sig Mitigation, Acceptance, Avoidance, Transfer extends Treatment {}

abstract sig Method {}
one sig Test, Proof, ModelCheck, Analysis, Inspection extends Method {}

abstract sig Classification {
    higher: set Classification
}
one sig Public, Internal, Confidential, Restricted, Unknown extends Classification {}

fact ClassificationOrder {
    Public.higher = Internal + Confidential + Restricted
    Internal.higher = Confidential + Restricted
    Confidential.higher = Restricted
    no Restricted.higher
    no Unknown.higher
}

sig Layer {
    lower: set Layer
}

fact LayerOrder {
    no l: Layer | l in l.^lower
    all disj a, b: Layer | b in a.^lower or a in b.^lower
}

sig Capability {
    cap_status: one Status
}

sig Component {
    comp_status: one Status,
    layer: one Layer,
    provides: set Capability,
    requires: set Capability,
    owns_data: set DataModel
}

sig DataModel {
    owner: one Component,
    classification: one Classification,
    derived_from: set DataModel,
    declassified_by: lone Requirement
}

sig Requirement {
    req_status: one Status,
    mandatory: lone Requirement,
    depends_on: set Requirement,
    refines: set Requirement,
    conflicts_with: set Requirement,
    resolved_by: lone Requirement
}

sig Evidence {
    verifies: one Requirement,
    method: one Method,
    passed: lone Evidence,
    supersedes: lone Evidence
}

sig Risk {
    treatment: one Treatment,
    mitigated_by: set Requirement,
    waives: lone Risk
}

// Structural subset of release invariants; source/evidence hashes and version bindings are checked/reviewed separately.
pred ReleaseValid {
    // DATA-05: inventory-only unknown classifications cannot reach release
    no d: DataModel | d.classification = Unknown
    // COMP-01: every component provides at least one capability
    all c: Component | some c.provides
    // COMP-04 / COMP-05 / COMP-06 / COMP-07: required capabilities resolve to a non-deprecated provider in the same or lower layer without cycles
    all c: Component, cap: c.requires | {
        c.comp_status != Deprecated => cap.cap_status != Deprecated
        one p: Component - c | cap in p.provides and (p.layer = c.layer or p.layer in c.layer.^lower)
    }
    no c: Component | c in c.^(requires.~provides)
    // DATA-02 / DATA-03: bidirectional single ownership
    all d: DataModel | d in d.owner.owns_data and (all c: Component | d in c.owns_data => c = d.owner)
    // DATA-04: lineage preserves classification unless declassified by an active requirement
    all d: DataModel, src: d.derived_from |
        (d.classification = src.classification or d.classification in src.classification.higher)
        or (some r: d.declassified_by | r.req_status != Deprecated)
    // REQ-02 / REQ-03: acyclic requirement graph and resolved conflicts
    no r: Requirement | r in r.^(depends_on + refines)
    all r: Requirement, c: r.conflicts_with |
        r.req_status = Deprecated or c.req_status = Deprecated or
        (some q: r.resolved_by + c.resolved_by | q.req_status != Deprecated)
    // STAT-04: active entities are Verified or Deprecated at release
    all cap: Capability | cap.cap_status in Verified + Deprecated
    all c: Component | c.comp_status in Verified + Deprecated
    all r: Requirement | r.req_status in Verified + Deprecated
    // RISK-03 / RISK-08: mitigated/avoided risks have mitigating requirements; no waivers remain at release
    all k: Risk | (k.treatment in Mitigation + Avoidance => some k.mitigated_by) and no k.waives
    // REQ-04 / REQ-05: mandatory active requirements have passing evidence and no unsuperseded failures
    all r: Requirement | (r.req_status != Deprecated and some r.mandatory) => {
        some e: Evidence | e.verifies = r and some e.passed
        all f: Evidence | (f.verifies = r and no f.passed) => (some s: Evidence | s.supersedes = f and some s.passed)
    }
}

assert LineagePreservesClassificationWithoutDeclassification {
    (ReleaseValid and no DataModel.declassified_by) =>
        all d: DataModel, src: d.^derived_from |
            d.classification = src.classification or d.classification in src.classification.higher
}

assert NoUnsupersededFailuresForMandatoryRequirements {
    ReleaseValid =>
        all r: Requirement, f: Evidence |
            (r.req_status != Deprecated and some r.mandatory and f.verifies = r and no f.passed) =>
                (some s: Evidence | s.supersedes = f and some s.passed)
}

run SatisfiableReleaseSpec {
    ReleaseValid
    #Component >= 2
    some DataModel.derived_from
    some Requirement.mandatory
} for 4

check LineagePreservesClassificationWithoutDeclassification for 5
check NoUnsupersededFailuresForMandatoryRequirements for 5
