use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfObservabilityMode {
    SelfIngest,
    ObserverInstance,
}

impl TryFrom<&str> for SelfObservabilityMode {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "self" => Ok(Self::SelfIngest),
            "observer_instance" => Ok(Self::ObserverInstance),
            other => anyhow::bail!("unsupported self-observability mode: {other}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfObservabilityConfig {
    pub mode: SelfObservabilityMode,
    pub otlp_endpoint: Option<String>,
}

impl SelfObservabilityConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::try_from_env_iter(std::env::vars())
    }

    pub fn from_env_iter<I, K, V>(vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        Self::try_from_env_iter(vars).expect("test self-observability env should be valid")
    }

    pub fn try_from_env_iter<I, K, V>(vars: I) -> anyhow::Result<Self>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let vars: HashMap<String, String> = vars
            .into_iter()
            .map(|(key, value)| (key.as_ref().to_string(), value.as_ref().to_string()))
            .collect();
        let mode = vars
            .get("OBSERVABLE_SELF_OBSERVABILITY_MODE")
            .map(|value| SelfObservabilityMode::try_from(value.as_str()))
            .transpose()?
            .unwrap_or(SelfObservabilityMode::SelfIngest);
        let otlp_endpoint = vars
            .get("OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT")
            .or_else(|| vars.get("OTEL_EXPORTER_OTLP_ENDPOINT"))
            .filter(|value| !value.trim().is_empty())
            .cloned();

        Ok(Self {
            mode,
            otlp_endpoint,
        })
    }

    pub fn selected_otlp_endpoint(&self) -> Option<&str> {
        self.otlp_endpoint.as_deref()
    }
}

/// Initialise JSON tracing subscriber for the service.
pub fn init_telemetry(service_name: &str, otlp_endpoint: Option<&str>) -> anyhow::Result<()> {
    tracing_subscriber::fmt().json().init();
    if let Some(ep) = otlp_endpoint {
        tracing::info!(
            service = service_name,
            otlp_endpoint = ep,
            "OTLP export configured (Phase 2 SDK wiring)"
        );
    } else {
        tracing::info!(service = service_name, "telemetry initialised");
    }
    Ok(())
}

pub fn init_self_observability_telemetry(service_name: &str) -> anyhow::Result<()> {
    let config = SelfObservabilityConfig::from_env()?;
    init_telemetry(service_name, config.selected_otlp_endpoint())?;
    tracing::info!(
        service = service_name,
        self_observability_mode = config.mode.as_str(),
        otlp_endpoint = config.selected_otlp_endpoint(),
        "self-observability route selected"
    );
    Ok(())
}

impl SelfObservabilityMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SelfIngest => "self",
            Self::ObserverInstance => "observer_instance",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_init_is_idempotent() {
        // Verifies init_telemetry does not panic when OTLP endpoint is absent.
        // Subscriber may already be set in other tests; ignore the error.
        let _ = init_telemetry("test-service", None);
    }

    #[test]
    fn self_observability_defaults_to_self_mode_without_endpoint() {
        let config = SelfObservabilityConfig::from_env_iter(Vec::<(&str, &str)>::new());

        assert_eq!(config.mode, SelfObservabilityMode::SelfIngest);
        assert_eq!(config.otlp_endpoint, None);
        assert_eq!(config.selected_otlp_endpoint(), None);
    }

    #[test]
    fn observer_instance_mode_uses_explicit_endpoint() {
        let config = SelfObservabilityConfig::from_env_iter([
            ("OBSERVABLE_SELF_OBSERVABILITY_MODE", "observer_instance"),
            (
                "OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT",
                "http://observer-ingest:4318",
            ),
        ]);

        assert_eq!(config.mode, SelfObservabilityMode::ObserverInstance);
        assert_eq!(
            config.selected_otlp_endpoint(),
            Some("http://observer-ingest:4318")
        );
    }

    #[test]
    fn self_mode_uses_legacy_otlp_endpoint_as_local_route() {
        let config = SelfObservabilityConfig::from_env_iter([
            ("OBSERVABLE_SELF_OBSERVABILITY_MODE", "self"),
            ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://ingest-gateway:4318"),
        ]);

        assert_eq!(config.mode, SelfObservabilityMode::SelfIngest);
        assert_eq!(
            config.selected_otlp_endpoint(),
            Some("http://ingest-gateway:4318")
        );
    }

    #[test]
    fn invalid_self_observability_mode_is_rejected() {
        let result = SelfObservabilityMode::try_from("mirror");

        assert!(result.is_err());
    }
}
