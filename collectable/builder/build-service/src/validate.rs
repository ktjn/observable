/// Semantic validation of a pipeline definition.
///
/// Enforces the value-source / mapping-target compatibility rules:
///
/// | Source  | resource_attributes | log_attributes |
/// |---------|---------------------|----------------|
/// | Field   | ❌                  | ✅             |
/// | Env     | ✅                  | ❌             |
/// | Command | ✅                  | ❌             |
/// | Literal | ✅                  | ❌             |
use crate::definition::{PipelineDefinition, ValueSource};
use anyhow::Result;

pub fn validate(def: &PipelineDefinition) -> Result<()> {
    let mut errors: Vec<String> = Vec::new();

    if let Some(resource_attrs) = &def.mapping.resource_attributes {
        for (key, source) in resource_attrs {
            if let ValueSource::Field { field, .. } = source {
                errors.push(format!(
                    "resource_attributes[\"{key}\"]: 'field' source (\"{field}\") is not allowed \
                     — resource attributes are set once at startup before any log lines are read. \
                     Use 'log_attributes' for per-record fields, or use 'env'/'command'/'literal' \
                     sources for resource attributes."
                ));
            }
        }
    }

    if let Some(log_attrs) = &def.mapping.log_attributes {
        for (key, source) in log_attrs {
            match source {
                ValueSource::Env { env } => {
                    errors.push(format!(
                        "log_attributes[\"{key}\"]: 'env' source (\"${{{env}}}\") is not allowed \
                         — environment variable values are constant for the process lifetime and \
                         belong in 'resource_attributes'."
                    ));
                }
                ValueSource::Command { command } => {
                    errors.push(format!(
                        "log_attributes[\"{key}\"]: 'command' source (\"{command}\") is not \
                         allowed — command output is evaluated once at startup and belongs in \
                         'resource_attributes'."
                    ));
                }
                ValueSource::Literal { literal } => {
                    errors.push(format!(
                        "log_attributes[\"{key}\"]: 'literal' source (\"{literal}\") is not \
                         allowed — constant values belong in 'resource_attributes'."
                    ));
                }
                ValueSource::Field { .. } => {}
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("{}", errors.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::*;
    use std::collections::HashMap;

    fn base_def() -> PipelineDefinition {
        PipelineDefinition {
            version: "1".into(),
            name: "test".into(),
            transport: TransportConfig {
                kind: "stdin".into(),
                params: HashMap::new(),
            },
            parser: ParserConfig {
                kind: "passthrough".into(),
                params: HashMap::new(),
            },
            mapping: MappingConfig {
                resource_attributes: None,
                log_attributes: None,
                body: None,
                severity_text: None,
                severity_number: None,
                trace_id: None,
                span_id: None,
                time_field: None,
            },
            output: OutputConfig {
                endpoint: "http://localhost:4317".into(),
                protocol: "grpc".into(),
                headers: None,
                batch_size: None,
                flush_interval_ms: None,
            },
        }
    }

    #[test]
    fn valid_field_in_log_attributes() {
        let mut def = base_def();
        def.mapping.log_attributes = Some(HashMap::from([(
            "host".into(),
            ValueSource::Field {
                field: "host".into(),
                r#type: None,
                map: None,
            },
        )]));
        assert!(validate(&def).is_ok());
    }

    #[test]
    fn valid_env_in_resource_attributes() {
        let mut def = base_def();
        def.mapping.resource_attributes = Some(HashMap::from([(
            "host.name".into(),
            ValueSource::Env {
                env: "HOSTNAME".into(),
            },
        )]));
        assert!(validate(&def).is_ok());
    }

    #[test]
    fn valid_command_in_resource_attributes() {
        let mut def = base_def();
        def.mapping.resource_attributes = Some(HashMap::from([(
            "host.name".into(),
            ValueSource::Command {
                command: "hostname -f".into(),
            },
        )]));
        assert!(validate(&def).is_ok());
    }

    #[test]
    fn valid_literal_in_resource_attributes() {
        let mut def = base_def();
        def.mapping.resource_attributes = Some(HashMap::from([(
            "deployment.environment".into(),
            ValueSource::Literal {
                literal: "production".into(),
            },
        )]));
        assert!(validate(&def).is_ok());
    }

    #[test]
    fn rejects_field_in_resource_attributes() {
        let mut def = base_def();
        def.mapping.resource_attributes = Some(HashMap::from([(
            "host".into(),
            ValueSource::Field {
                field: "host".into(),
                r#type: None,
                map: None,
            },
        )]));
        let err = validate(&def).unwrap_err().to_string();
        assert!(err.contains("resource_attributes[\"host\"]"));
        assert!(err.contains("'field' source"));
    }

    #[test]
    fn rejects_env_in_log_attributes() {
        let mut def = base_def();
        def.mapping.log_attributes = Some(HashMap::from([(
            "env_label".into(),
            ValueSource::Env {
                env: "LABEL".into(),
            },
        )]));
        let err = validate(&def).unwrap_err().to_string();
        assert!(err.contains("log_attributes[\"env_label\"]"));
        assert!(err.contains("'env' source"));
    }

    #[test]
    fn rejects_command_in_log_attributes() {
        let mut def = base_def();
        def.mapping.log_attributes = Some(HashMap::from([(
            "hostname".into(),
            ValueSource::Command {
                command: "hostname -f".into(),
            },
        )]));
        let err = validate(&def).unwrap_err().to_string();
        assert!(err.contains("log_attributes[\"hostname\"]"));
        assert!(err.contains("'command' source"));
    }

    #[test]
    fn rejects_literal_in_log_attributes() {
        let mut def = base_def();
        def.mapping.log_attributes = Some(HashMap::from([(
            "region".into(),
            ValueSource::Literal {
                literal: "eu-west-1".into(),
            },
        )]));
        let err = validate(&def).unwrap_err().to_string();
        assert!(err.contains("log_attributes[\"region\"]"));
        assert!(err.contains("'literal' source"));
    }

    #[test]
    fn collects_multiple_errors() {
        let mut def = base_def();
        def.mapping.resource_attributes = Some(HashMap::from([(
            "host".into(),
            ValueSource::Field {
                field: "host".into(),
                r#type: None,
                map: None,
            },
        )]));
        def.mapping.log_attributes = Some(HashMap::from([(
            "region".into(),
            ValueSource::Literal {
                literal: "eu-west-1".into(),
            },
        )]));
        let err = validate(&def).unwrap_err().to_string();
        assert!(err.contains("resource_attributes"));
        assert!(err.contains("log_attributes"));
    }
}
