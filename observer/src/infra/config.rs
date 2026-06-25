use serde::Deserialize;
use std::path::Path;

// TODO: receive this from args?
/// Default config file path
pub const CONFIG_PATH: &str = "config.toml";

/// App configuration, loaded from the TOML config file
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    /// Bitcoin Core RPC connection config
    pub rpc: RpcConfig,

    /// Bitcoin Core ZMQ publisher endpoints, keyed by stream type
    #[serde(default)]
    pub zmq: ZmqConfig,

    /// Minimal seconds between watcher poll cycles
    #[serde(default = "default_poll_interval_secs")]
    pub poll_interval_secs: u64,

    /// Tracing level filter (e.g. `trace`, `debug`, `info`, `warn`, `error`).
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Enable/disable specific watchers
    #[serde(default)]
    pub watchers: WatchersConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RpcConfig {
    /// Bitcoin Core RPC `host:port`
    pub host: String,
    /// RPC username
    pub user: String,
    /// RPC password
    pub pass: String,
    // TODO: add config to use cookie auth (optional)
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ZmqConfig {
    /// `hashblock` endpoint (e.g. `tcp://127.0.0.1:28332`) for the block watcher
    #[serde(default)]
    pub blocks_endpoint: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WatchersConfig {
    /// Enables the `mempool_delta` watcher (default: true)
    #[serde(default)]
    pub mempool_delta: bool,

    /// Enables the block watcher (default: true). Also requires `rpc.zmq_endpoint`
    #[serde(default)]
    pub block: bool,
}

impl Default for WatchersConfig {
    fn default() -> Self {
        Self {
            mempool_delta: true,
            block: true,
        }
    }
}

fn default_poll_interval_secs() -> u64 {
    10
}

fn default_log_level() -> String {
    "debug".to_string()
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
