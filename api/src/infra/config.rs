use observer::infra::config::Config as ObserverConfig;
use serde::Deserialize;
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

    /// Observer-side config, flattened into the same TOML document.
    #[serde(flatten)]
    pub observer: ObserverConfig,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            observer: ObserverConfig::default(),
        }
    }
}

fn default_bind() -> String {
    "127.0.0.1:3333".to_string()
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
