use std::env;

pub fn is_dev_mode() -> bool {
    env::var("OBSERVABLE_ENV").as_deref() == Ok("dev")
}

pub fn require_env(name: &str) -> anyhow::Result<String> {
    if is_dev_mode() {
        return Ok(env::var(name).unwrap_or_else(|_| dev_default(name)));
    }
    env::var(name).map_err(|_| {
        anyhow::anyhow!(
            "{name} must be set when OBSERVABLE_ENV is not \"dev\". \
             Set it explicitly or use OBSERVABLE_ENV=dev for local development."
        )
    })
}

pub fn require_env_or(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.into())
}

fn dev_default(name: &str) -> String {
    match name {
        "DATABASE_URL" => "postgres://observable:observable@localhost:5432/observable".into(),
        "CLICKHOUSE_URL" => "http://localhost:8123".into(),
        "CLICKHOUSE_USER" => "default".into(),
        "CLICKHOUSE_PASSWORD" => String::new(),
        "REDPANDA_BROKERS" => "localhost:9092".into(),
        "INGEST_TOPIC" => "telemetry.raw".into(),
        "AUTH_SERVICE_URL" => "http://localhost:4319".into(),
        "STORAGE_WRITER_URL" => "http://localhost:4320".into(),
        "ZITADEL_ISSUER" => "http://localhost:8082".into(),
        "ZITADEL_API_BASE" => "http://localhost:8082".into(),
        "ZITADEL_CLIENT_ID" => "dev-client-id".into(),
        "ZITADEL_REDIRECT_URI" => "http://localhost:5173/auth/callback".into(),
        _ => String::new(),
    }
}

pub fn warn_default(name: &str, value: &str) {
    if is_dev_mode() && env::var(name).is_err() {
        tracing::info!("{name} not set, using dev default: {value}");
    }
}
