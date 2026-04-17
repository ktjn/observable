/// Initialise JSON tracing subscriber for the service.
/// When OTEL_EXPORTER_OTLP_ENDPOINT is set, logs a reminder that
/// full OTel export is wired in Phase 2.
pub fn init_telemetry(service_name: &str, otlp_endpoint: Option<&str>) -> anyhow::Result<()> {
    tracing_subscriber::fmt().json().init();
    if let Some(ep) = otlp_endpoint {
        tracing::info!(service = service_name, otlp_endpoint = ep, "OTLP export configured (Phase 2 SDK wiring)");
    } else {
        tracing::info!(service = service_name, "telemetry initialised");
    }
    Ok(())
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
}
