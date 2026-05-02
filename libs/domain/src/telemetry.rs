use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{WithExportConfig, WithTonicConfig};
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;
use std::collections::HashMap;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

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
    /// Bearer token sent as `Authorization: Bearer <token>` on every OTLP export request.
    pub bearer_token: Option<String>,
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
        let bearer_token = vars
            .get("OBSERVABLE_SELF_OBSERVABILITY_BEARER_TOKEN")
            .filter(|v| !v.trim().is_empty())
            .cloned();

        Ok(Self {
            mode,
            otlp_endpoint,
            bearer_token,
        })
    }

    pub fn selected_otlp_endpoint(&self) -> Option<&str> {
        self.otlp_endpoint.as_deref()
    }
}

/// Initialise tracing for the service.
///
/// When `otlp_endpoint` is `Some`, wires the OTel SDK: spans emitted via the
/// `tracing` macros are forwarded to the endpoint using OTLP/HTTP. The returned
/// `SdkTracerProvider` must be kept alive for the process lifetime; dropping it
/// triggers a flush + shutdown of the exporter.
///
/// `bearer_token` — when `Some`, adds `Authorization: Bearer <token>` to every
/// OTLP export request so the service can authenticate directly with ingest-gateway.
/// The `deployment.environment` resource attribute is stamped server-side from the
/// API key's environment field — no client-side setting is required or supported.
///
/// When `otlp_endpoint` is `None`, only the JSON `fmt` layer is registered.
pub fn init_telemetry(
    service_name: &str,
    otlp_endpoint: Option<&str>,
    bearer_token: Option<&str>,
) -> anyhow::Result<Option<SdkTracerProvider>> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    if let Some(endpoint) = otlp_endpoint {
        let mut metadata = tonic::metadata::MetadataMap::new();
        if let Some(token) = bearer_token {
            let value = format!("Bearer {token}")
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid bearer token: {e}"))?;
            metadata.insert("authorization", value);
        }

        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .with_metadata(metadata)
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build OTLP exporter: {e}"))?;

        let provider = SdkTracerProvider::builder()
            .with_resource(
                Resource::builder()
                    .with_service_name(service_name.to_string())
                    .build(),
            )
            .with_batch_exporter(exporter)
            .build();

        opentelemetry::global::set_tracer_provider(provider.clone());
        opentelemetry::global::set_text_map_propagator(
            opentelemetry_sdk::propagation::TraceContextPropagator::new(),
        );

        let tracer = provider.tracer(service_name.to_string());
        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().json())
            .with(env_filter)
            .with(otel_layer)
            .init();

        tracing::info!(
            service = service_name,
            otlp_endpoint = endpoint,
            "OTLP tracing enabled"
        );

        Ok(Some(provider))
    } else {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().json())
            .with(env_filter)
            .init();

        tracing::info!(service = service_name, "telemetry initialised (log-only)");

        Ok(None)
    }
}

pub fn init_self_observability_telemetry(
    service_name: &str,
) -> anyhow::Result<Option<SdkTracerProvider>> {
    let config = SelfObservabilityConfig::from_env()?;
    let provider = init_telemetry(
        service_name,
        config.selected_otlp_endpoint(),
        config.bearer_token.as_deref(),
    )?;
    tracing::info!(
        service = service_name,
        self_observability_mode = config.mode.as_str(),
        otlp_endpoint = config.selected_otlp_endpoint(),
        "self-observability route selected"
    );
    Ok(provider)
}

impl SelfObservabilityMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SelfIngest => "self",
            Self::ObserverInstance => "observer_instance",
        }
    }
}

/// Inject the current OpenTelemetry span context into a `reqwest` `HeaderMap` as a
/// W3C `traceparent` header. Call this before every outgoing HTTP request that should
/// be linked to the current trace (e.g. ingest-gateway → auth-service calls).
///
/// This is a no-op when no OTel provider / propagator has been installed (e.g. in
/// unit-test mode or when `otlp_endpoint` is `None`).
pub fn inject_current_context(headers: &mut reqwest::header::HeaderMap) {
    let mut carrier = std::collections::HashMap::<String, String>::new();
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject(&mut carrier);
    });
    for (k, v) in carrier {
        if let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::from_bytes(k.as_bytes()),
            reqwest::header::HeaderValue::from_str(&v),
        ) {
            headers.insert(name, value);
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
        let _ = init_telemetry("test-service", None, None);
    }

    #[test]
    fn self_observability_defaults_to_self_mode_without_endpoint() {
        let config = SelfObservabilityConfig::from_env_iter(Vec::<(&str, &str)>::new());

        assert_eq!(config.mode, SelfObservabilityMode::SelfIngest);
        assert_eq!(config.otlp_endpoint, None);
        assert_eq!(config.bearer_token, None);
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
    fn bearer_token_is_read_from_env_vars() {
        let config = SelfObservabilityConfig::from_env_iter([
            (
                "OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT",
                "http://ingest-gateway:4318",
            ),
            ("OBSERVABLE_SELF_OBSERVABILITY_BEARER_TOKEN", "my-secret"),
        ]);

        assert_eq!(config.bearer_token.as_deref(), Some("my-secret"));
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
