//! Telemetry scope of one catalog service in one environment class (phase 1 C2.2).
//!
//! An observation counts only if it matches the tenant, a bound `service.name` and an exact
//! nonempty environment that the catalog classifies in the requested class. Empty and
//! unclassified bindings contribute to neither class. Ambiguous inputs make the scope
//! unavailable instead of guessing.

use std::collections::HashSet;

use uuid::Uuid;

use crate::ports::{EnvironmentClass, TelemetryIdentity, TenantInputs};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    /// A live service and its identities in the class; may be empty (no evidence possible).
    Bound(Vec<TelemetryIdentity>),
    /// Missing, hidden from the worker, planned or retired: nothing is evaluated.
    NotEvaluated,
    /// The catalog inputs are incomplete or ambiguous.
    Unavailable,
}

pub fn scope(inputs: &TenantInputs, service_id: Uuid, class: EnvironmentClass) -> Scope {
    // One record for the service, and no identity of it bound to any other record; anything
    // else is ambiguous. Conflicts elsewhere in the tenant do not affect this service.
    let mut records = inputs.services.iter().filter(|s| s.id == service_id);
    let Some(service) = records.next() else {
        return Scope::NotEvaluated;
    };
    if records.next().is_some() {
        return Scope::Unavailable;
    }
    let own: HashSet<(&str, &str)> = service
        .bindings
        .iter()
        .map(|b| (b.service_name.as_str(), b.environment.as_str()))
        .collect();
    if own.len() != service.bindings.len()
        || inputs
            .services
            .iter()
            .filter(|s| s.id != service_id)
            .flat_map(|s| &s.bindings)
            .any(|b| own.contains(&(b.service_name.as_str(), b.environment.as_str())))
    {
        return Scope::Unavailable;
    }
    match service.lifecycle.as_str() {
        // Only running services have current health: planned and retired are non-live.
        "planned" | "retired" => return Scope::NotEvaluated,
        "live" | "deprecated" => {}
        _ => return Scope::Unavailable,
    }
    let mut bound: Vec<TelemetryIdentity> = service
        .bindings
        .iter()
        .filter(|b| {
            !b.environment.trim().is_empty()
                && !b.service_name.trim().is_empty()
                && b.environment_class == Some(class)
        })
        .map(|b| TelemetryIdentity {
            service_name: b.service_name.clone(),
            environment: b.environment.clone(),
        })
        .collect();
    bound.sort_by(|a, b| (&a.service_name, &a.environment).cmp(&(&b.service_name, &b.environment)));
    Scope::Bound(bound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{BindingInput, ServiceInputs};
    use EnvironmentClass::*;

    fn binding(name: &str, env: &str, class: Option<EnvironmentClass>) -> BindingInput {
        BindingInput {
            service_name: name.into(),
            environment: env.into(),
            environment_class: class,
        }
    }

    fn service(id: Uuid, lifecycle: &str, bindings: Vec<BindingInput>) -> ServiceInputs {
        ServiceInputs {
            id,
            lifecycle: lifecycle.into(),
            criticality: "critical".into(),
            classification: "internal".into(),
            bindings,
        }
    }

    fn identity(name: &str, env: &str) -> TelemetryIdentity {
        TelemetryIdentity {
            service_name: name.into(),
            environment: env.into(),
        }
    }

    fn tenant(services: Vec<ServiceInputs>) -> TenantInputs {
        TenantInputs {
            tenant_id: Uuid::new_v4(),
            services,
        }
    }

    #[test]
    fn selects_exact_classified_environments_of_the_requested_class() {
        let id = Uuid::new_v4();
        let inputs = tenant(vec![service(
            id,
            "live",
            vec![
                binding("payments", "prod", Some(Production)),
                binding("payments", "staging", Some(NonProduction)),
                binding("payments", "", None),
                binding("payments", "lab", None),
            ],
        )]);
        assert_eq!(
            scope(&inputs, id, Production),
            Scope::Bound(vec![identity("payments", "prod")])
        );
        assert_eq!(
            scope(&inputs, id, NonProduction),
            Scope::Bound(vec![identity("payments", "staging")])
        );
    }

    #[test]
    fn empty_or_unclassified_bindings_leave_an_empty_scope() {
        let id = Uuid::new_v4();
        let inputs = tenant(vec![service(
            id,
            "live",
            vec![
                binding("payments", "", None),
                binding("payments", "lab", None),
            ],
        )]);
        assert_eq!(scope(&inputs, id, Production), Scope::Bound(vec![]));
        assert_eq!(scope(&inputs, id, NonProduction), Scope::Bound(vec![]));
        // A class on an empty environment is not trusted either.
        let inputs = tenant(vec![service(
            id,
            "live",
            vec![binding("payments", " ", Some(Production))],
        )]);
        assert_eq!(scope(&inputs, id, Production), Scope::Bound(vec![]));
    }

    #[test]
    fn same_name_in_production_and_non_production_stays_with_its_service() {
        let (live, test) = (Uuid::new_v4(), Uuid::new_v4());
        let inputs = tenant(vec![
            service(
                live,
                "live",
                vec![binding("payments", "prod", Some(Production))],
            ),
            service(
                test,
                "live",
                vec![binding("payments", "staging", Some(NonProduction))],
            ),
        ]);
        assert_eq!(
            scope(&inputs, live, Production),
            Scope::Bound(vec![identity("payments", "prod")])
        );
        assert_eq!(scope(&inputs, live, NonProduction), Scope::Bound(vec![]));
        assert_eq!(scope(&inputs, test, Production), Scope::Bound(vec![]));
    }

    #[test]
    fn retired_missing_and_ambiguous_inputs_are_not_evaluated() {
        let (id, other) = (Uuid::new_v4(), Uuid::new_v4());
        let prod = || vec![binding("payments", "prod", Some(Production))];
        assert_eq!(
            scope(
                &tenant(vec![service(id, "retired", prod())]),
                id,
                Production
            ),
            Scope::NotEvaluated
        );
        assert_eq!(scope(&tenant(vec![]), id, Production), Scope::NotEvaluated);
        assert_eq!(
            scope(&tenant(vec![service(id, "paused", prod())]), id, Production),
            Scope::Unavailable
        );
        assert_eq!(
            scope(
                &tenant(vec![
                    service(id, "live", prod()),
                    service(id, "live", vec![])
                ]),
                id,
                Production
            ),
            Scope::Unavailable
        );
        assert_eq!(
            scope(
                &tenant(vec![
                    service(id, "live", prod()),
                    service(other, "live", prod())
                ]),
                id,
                Production
            ),
            Scope::Unavailable
        );
        // A conflict between two other services does not affect this one.
        let third = Uuid::new_v4();
        assert_eq!(
            scope(
                &tenant(vec![
                    service(
                        id,
                        "live",
                        vec![binding("ledger", "prod", Some(Production))]
                    ),
                    service(other, "live", prod()),
                    service(third, "live", prod()),
                ]),
                id,
                Production
            ),
            Scope::Bound(vec![identity("ledger", "prod")])
        );
    }

    #[test]
    fn a_changed_binding_or_class_changes_the_scope() {
        let id = Uuid::new_v4();
        let before = tenant(vec![service(
            id,
            "live",
            vec![binding("payments", "prod", Some(Production))],
        )]);
        let reclassified = tenant(vec![service(
            id,
            "live",
            vec![binding("payments", "prod", Some(NonProduction))],
        )]);
        let rebound = tenant(vec![service(
            id,
            "live",
            vec![binding("payments-v2", "prod", Some(Production))],
        )]);
        assert_ne!(
            scope(&before, id, Production),
            scope(&reclassified, id, Production)
        );
        assert_eq!(scope(&reclassified, id, Production), Scope::Bound(vec![]));
        assert_eq!(
            scope(&rebound, id, Production),
            Scope::Bound(vec![identity("payments-v2", "prod")])
        );
    }
}
