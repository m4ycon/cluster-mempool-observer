use std::path::Path;

use serde::Deserialize;

// TODO: receive this from args?
/// Default config file path
pub const CONFIG_PATH: &str = "config.toml";

/// App configuration, loaded from the TOML config file
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Bitcoin Core RPC connection config
    pub rpc: RpcConfig,

    /// Minimal seconds between extractor poll cycles
    #[serde(default = "default_poll_interval_secs")]
    pub poll_interval_secs: u64,

    /// NATS server connection config
    #[serde(default)]
    pub nats: NatsConfig,

    /// Enable/disable specific extractors
    #[serde(default)]
    pub extractors: ExtractorsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RpcConfig {
    /// Bitcoin Core RPC `host:port`
    pub host: String,
    /// RPC username
    pub user: String,
    /// RPC password
    pub pass: String,
    // TODO: add config to use cookie auth (optional)
}

#[derive(Debug, Clone, Deserialize)]
pub struct NatsConfig {
    /// NATS server address (`host:port`) to publish events to
    #[serde(default = "default_nats_address")]
    pub address: String,
    /// NATS username for authentication (optional)
    #[serde(default)]
    pub username: Option<String>,
    /// NATS password for authentication (optional)
    #[serde(default)]
    pub password: Option<String>,
}

impl Default for NatsConfig {
    fn default() -> Self {
        Self {
            address: default_nats_address(),
            username: None,
            password: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExtractorsConfig {
    /// Enables the `getrawmempool` extractor (default: true)
    #[serde(default)]
    pub getrawmempool: bool,
}

impl Default for ExtractorsConfig {
    fn default() -> Self {
        Self {
            getrawmempool: true,
        }
    }
}

fn default_poll_interval_secs() -> u64 {
    10
}

fn default_nats_address() -> String {
    "127.0.0.1:4222".to_string()
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

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path).map_err(ConfigError::Read)?;
        toml::from_str(&contents).map_err(ConfigError::Parse)
    }
}

pub fn init_config() -> Result<Config, ConfigError> {
    Config::load(Path::new(CONFIG_PATH))
}
