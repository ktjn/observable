/// Unified configuration: config file + environment variable overlay.
///
/// Config file path defaults to `collectable.toml` in the working directory.
/// Override with `--config <path>` or `COLLECTABLE_CONFIG` env var.
/// Environment variables take the form `COLLECTABLE_<SECTION>_<KEY>`.
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

impl Config {
    pub fn load(path: Option<&str>) -> Result<Self> {
        let mut builder = config::Config::builder();
        let cfg_path = path.unwrap_or("collectable.toml");
        if std::path::Path::new(cfg_path).exists() {
            builder = builder.add_source(config::File::with_name(cfg_path));
        }
        builder = builder.add_source(
            config::Environment::with_prefix("COLLECTABLE").separator("_"),
        );
        Ok(builder.build()?.try_deserialize()?)
    }
}
