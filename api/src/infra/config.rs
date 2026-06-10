use serde::Deserialize;
use shared::nats::NatsConfig;
use std::path::Path;

// TODO: receive this from args?
/// Default config file path
pub const CONFIG_PATH: &str = "config.toml";

/// API server configuration, loaded from the TOML config file
#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    /// `host:port` the HTTP server binds to
    #[serde(default = "default_bind")]
    pub bind: String,

    /// Tracing level filter (e.g. `trace`, `debug`, `info`, `warn`, `error`).
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// NATS server connection config
    #[serde(default)]
    pub nats: NatsConfig,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            log_level: default_log_level(),
            nats: NatsConfig::default(),
        }
    }
}

fn default_bind() -> String {
    "127.0.0.1:3333".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

#[derive(Debug)]
pub enum ConfigError {
    Read(std::io::Error),
    Parse(toml::de::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Read(e) => write!(f, "failed to read config file: {e}"),
            ConfigError::Parse(e) => write!(f, "failed to parse config file: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl ApiConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path).map_err(ConfigError::Read)?;
        toml::from_str(&contents).map_err(ConfigError::Parse)
    }
}
