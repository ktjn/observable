/// Runtime configuration for the compiled mediator.
///
/// Connection details (OTLP endpoint, transport port, broker URL, auth token)
/// are resolved via `${ENV_VAR}` interpolation in the pipeline definition at
/// startup — not stored here. This struct covers operational settings only.
///
/// Standard well-known variables (resolved from the pipeline definition):
///   OTLP_ENDPOINT          — OTLP receiver URL (required)
///   OTLP_TOKEN             — Bearer auth token (optional)
///   OTLP_PROTOCOL          — "grpc" (default) or "http"
///   OTLP_INSECURE          — "true" to disable TLS (local dev)
///   TRANSPORT_LISTEN_HOST  — bind address (default 0.0.0.0)
///   TRANSPORT_PORT         — listen port for syslog/webhook transports
///   MQTT_BROKER / MQTT_TOPIC / MQTT_USERNAME / MQTT_PASSWORD
///   KAFKA_BROKERS / KAFKA_TOPIC / KAFKA_GROUP_ID
///   FILE_PATH              — file tail path or glob
///
/// Operational variables (read directly, not via pipeline definition):
///   COLLECTABLE_LOG_LEVEL          — default: info
///   COLLECTABLE_LOG_FORMAT         — "json" (default) or "text"
///   COLLECTABLE_HEALTH_PORT        — default: 9090
///   COLLECTABLE_SHUTDOWN_TIMEOUT_SECS — default: 10
///   COLLECTABLE_PID_FILE           — write PID here (init.d)
use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub log_level: Option<String>,
    pub log_format: Option<String>,
    pub shutdown_timeout_secs: Option<u64>,
    pub pid_file: Option<String>,
    pub health_port: Option<u16>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_level: Some("info".into()),
            log_format: Some("json".into()),
            shutdown_timeout_secs: Some(10),
            pid_file: None,
            health_port: Some(9090),
        }
    }
}

impl Config {
    pub fn load(path: Option<&str>) -> Result<Self> {
        let mut builder = config::Config::builder();
        let cfg_path = path.unwrap_or("collectable.toml");
        if std::path::Path::new(cfg_path).exists() {
            builder = builder.add_source(config::File::with_name(cfg_path));
        }
        // COLLECTABLE_LOG_LEVEL, COLLECTABLE_LOG_FORMAT, etc.
        builder = builder.add_source(
            config::Environment::with_prefix("COLLECTABLE").separator("_"),
        );
        Ok(builder.build()?.try_deserialize()?)
    }
}

/// Resolve `${VAR}` and `${VAR:-default}` references in a string value
/// from the process environment. Returns an error listing all missing
/// variables that have no default.
pub fn resolve_env(value: &str) -> Result<String> {
    let mut result = value.to_string();
    let mut missing: Vec<String> = Vec::new();

    // Match ${VAR} and ${VAR:-default}
    let re = regex::Regex::new(r"\$\{([A-Z_][A-Z0-9_]*)(?::-(.*?))?\}").unwrap();
    result = re.replace_all(value, |caps: &regex::Captures| {
        let var = &caps[1];
        let default = caps.get(2).map(|m| m.as_str());
        match std::env::var(var) {
            Ok(v) => v,
            Err(_) => match default {
                Some(d) => d.to_string(),
                None => {
                    missing.push(var.to_string());
                    String::new()
                }
            }
        }
    }).to_string();

    if !missing.is_empty() {
        anyhow::bail!(
            "missing required environment variable(s): {}",
            missing.join(", ")
        );
    }
    Ok(result)
}
